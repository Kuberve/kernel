use core::arch::asm;
use crate::println;

pub const STACK_SIZE: usize = 64 * 1024;
pub const MAX_THREADS: usize = 8;
pub const IDLE: usize = usize::MAX;

// Размер контекста: q0..q31 (512) + FPCR/FPSR (16) + ELR/SPSR (16) + 16 пар GPR (256) = 800
const CTX_SIZE: u64 = 800;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum State {
    Ready,
    Running,
    Sleeping,
    Dead,
}

#[derive(Clone, Copy)]
pub struct Thread {
    pub sp: u64,
    pub sp_el0: u64,
    pub state: State,
    pub sleep_until: u64,
    pub priority: u8,
    pub wait_ticks: u32,
    pub id: u32,
    pub name: &'static str,
}

const EMPTY_THREAD: Thread = Thread {
    sp: 0,
    sp_el0: 0,
    state: State::Dead,
    sleep_until: 0,
    priority: 0,
    wait_ticks: 0,
    id: 0,
    name: "",
};

static mut THREADS: [Thread; MAX_THREADS] = [EMPTY_THREAD; MAX_THREADS];
static mut STACKS: [[u8; STACK_SIZE]; MAX_THREADS] =
    [[0; STACK_SIZE]; MAX_THREADS];
static mut COUNT: usize = 0;
static mut CURRENT: usize = IDLE;
static mut TICK: u64 = 0;
static mut IDLE_SP: u64 = 0;

pub fn init() {
    unsafe {
        COUNT = 0;
        CURRENT = IDLE;
        TICK = 0;
        IDLE_SP = 0;
    }
}

pub unsafe fn spawn(name: &'static str, priority: u8, entry: extern "C" fn() -> !) {
    let id = COUNT;
    if id >= MAX_THREADS {
        println!("sched: max threads");
        return;
    }
    COUNT += 1;

    let top = (STACKS[id].as_ptr() as u64 + STACK_SIZE as u64) & !0xF;
    let sp = top - CTX_SIZE;

    // Обнуляем контекст
    let p = sp as *mut u64;
    for i in 0..(CTX_SIZE / 8) {
        core::ptr::write_volatile(p.add(i as usize), 0);
    }

    // [sp+0]  = ELR   = entry
    // [sp+8]  = SPSR  = 0x345 (EL1h, I=0, D/A/F=1)
    core::ptr::write_volatile(p.add(0), entry as u64);
    core::ptr::write_volatile(p.add(1), 0x345u64);

    THREADS[id] = Thread {
        sp,
        sp_el0: 0,
        state: State::Ready,
        sleep_until: 0,
        priority,
        wait_ticks: 0,
        id: id as u32,
        name,
    };

    println!("spawned '{}' id={} sp={:#x}", name, id, sp);
}

pub unsafe fn spawn_el0(name: &'static str, priority: u8, entry: u64, usp: u64) {
    let id = COUNT;
    if id >= MAX_THREADS {
        println!("sched: max threads");
        return;
    }
    COUNT += 1;

    let top = (STACKS[id].as_ptr() as u64 + STACK_SIZE as u64) & !0xF;
    let sp = top - CTX_SIZE;
    let p = sp as *mut u64;
    for i in 0..(CTX_SIZE / 8) {
        core::ptr::write_volatile(p.add(i as usize), 0);
    }

    // SavedCtx: elr = entry (user code), spsr = 0x000 (EL0t, IRQ on)
    core::ptr::write_volatile(p.add(0), entry);
    core::ptr::write_volatile(p.add(1), 0x000u64);

    THREADS[id] = Thread {
        sp,
        sp_el0: usp,
        state: State::Ready,
        sleep_until: 0,
        priority,
        wait_ticks: 0,
        id: id as u32,
        name,
    };

    println!("spawned '{}' (EL0) id={} ksp={:#x} usp={:#x}", name, id, sp, usp);
}

fn pick_next(current: usize) -> usize {
    unsafe {
        if COUNT == 0 { return IDLE; }

        // Эффективный приоритет = базовый + aging
        let eff = |t: &Thread| -> u32 {
            t.priority as u32 + (t.wait_ticks / 5)
        };

        // Максимальный эффективный приоритет среди Ready
        let mut best_eff: u32 = 0;
        let mut any = false;
        for i in 0..COUNT {
            if THREADS[i].state == State::Ready {
                any = true;
                let e = eff(&THREADS[i]);
                if e > best_eff { best_eff = e; }
            }
        }
        if !any { return IDLE; }

        // RR среди Ready с best_eff, начиная с current+1
        let start = if current == IDLE { 0 } else { (current + 1) % COUNT };
        for i in 0..COUNT {
            let idx = (start + i) % COUNT;
            if THREADS[idx].state == State::Ready && eff(&THREADS[idx]) == best_eff {
                return idx;
            }
        }
        IDLE
    }
}

pub unsafe fn on_tick(old_sp: u64) -> u64 {
    // 1. Сохранить текущий
    if CURRENT == IDLE {
        IDLE_SP = old_sp;
    } else {
        THREADS[CURRENT].sp = old_sp;
        let tmp: u64;
        asm!("mrs {}, SP_EL0", out(reg) tmp);
        THREADS[CURRENT].sp_el0 = tmp;
        if THREADS[CURRENT].state == State::Running {
            THREADS[CURRENT].state = State::Ready;
        }
    }

    // 2. Просыпаем тех, чьё время пришло
    for i in 0..COUNT {
        if THREADS[i].state == State::Sleeping && THREADS[i].sleep_until <= TICK {
            THREADS[i].state = State::Ready;
        }
    }
    TICK += 1;

    // 3. Выбрать следующего
    let next = pick_next(CURRENT);
    CURRENT = next;

    // 4. Aging: тем, кто ждёт, +1 к wait_ticks; выбранному сброс
    for i in 0..COUNT {
        if THREADS[i].state == State::Ready && i != next {
            THREADS[i].wait_ticks = THREADS[i].wait_ticks.saturating_add(1);
        } else if i == next {
            THREADS[i].wait_ticks = 0;
        }
    }

    // 5. Вернуть SP того, кого запускать
    if next == IDLE {
        IDLE_SP
    } else {
        THREADS[next].state = State::Running;
        if THREADS[next].sp_el0 != 0 {
            asm!("msr SP_EL0, {}", in(reg) THREADS[next].sp_el0);
        }
        THREADS[next].sp
    }
}

/// Поток засыпает на `ticks` тиков. Не занимает CPU.
pub unsafe fn sleep(ticks: u64) {
    if CURRENT == IDLE {
        return;
    }
    let me = CURRENT;
    THREADS[me].state = State::Sleeping;
    THREADS[me].sleep_until = TICK + ticks;

    // Ждём, пока планировщик не сменит наш state на Ready
    loop {
        asm!("msr daifclr, #2");
        asm!("wfe");
        if THREADS[me].state != State::Sleeping {
            return;
        }
    }
}

/// Завершить текущий поток. Стек не освобождается (пока).
pub fn exit() -> ! {
    unsafe {
        if CURRENT != IDLE {
            THREADS[CURRENT].state = State::Dead;
        }
    }
    loop {
        unsafe { 
            asm!("msr daifclr, #2");
            asm!("wfe") 
        }
    }
}

pub fn tick_count() -> u64 {
    unsafe { TICK }
}

pub unsafe fn on_yield(old_sp: u64) -> u64 {
    if COUNT == 0 { return old_sp; }

    if CURRENT == IDLE {
        IDLE_SP = old_sp;
    } else {
        THREADS[CURRENT].sp = old_sp;
        if THREADS[CURRENT].state == State::Running {
            THREADS[CURRENT].state = State::Ready;
        }
    }

    let next = pick_next(CURRENT);
    CURRENT = next;

    for i in 0..COUNT {
        if THREADS[i].state == State::Ready && i != next {
            THREADS[i].wait_ticks = THREADS[i].wait_ticks.saturating_add(1);
        } else if i == next {
            THREADS[i].wait_ticks = 0;
        }
    }

    if next == IDLE {
        IDLE_SP
    } else {
        THREADS[next].state = State::Running;
        THREADS[next].sp
    }
}

/// Поток отдаёт CPU добровольно.
pub fn yield_now() {
    unsafe { asm!("svc #0"); }
}

/// Layout сохранённого контекста на стеке.
/// Соответствует порядку stp в sync_handler.
/// sp+0 — ELR, sp+8 — SPSR, sp+16 — x30, ..., sp+256 — x0, sp+264 — x1.
#[repr(C)]
pub struct SavedCtx {
    pub elr: u64,     // sp+0
    pub spsr: u64,    // sp+8
    pub x30: u64,     // sp+16
    pub pad: u64,     // sp+24 (xzr)
    pub x28: u64,
    pub x29: u64,
    pub x26: u64,
    pub x27: u64,
    pub x24: u64,
    pub x25: u64,
    pub x22: u64,
    pub x23: u64,
    pub x20: u64,
    pub x21: u64,
    pub x18: u64,
    pub x19: u64,
    pub x16: u64,
    pub x17: u64,
    pub x14: u64,
    pub x15: u64,
    pub x12: u64,
    pub x13: u64,
    pub x10: u64,
    pub x11: u64,
    pub x8: u64,      // sp+192
    pub x9: u64,
    pub x6: u64,
    pub x7: u64,
    pub x4: u64,
    pub x5: u64,
    pub x2: u64,
    pub x3: u64,
    pub x0: u64,      // sp+256
    pub x1: u64,      // sp+264
}