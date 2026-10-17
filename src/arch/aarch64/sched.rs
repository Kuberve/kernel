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
    pub state: State,
    pub sleep_until: u64,
    pub priority: u8,
    pub id: u32,
    pub name: &'static str,
}

const EMPTY_THREAD: Thread = Thread {
    sp: 0,
    state: State::Dead,
    sleep_until: 0,
    priority: 0,
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
        state: State::Ready,
        sleep_until: 0,
        priority,
        id: id as u32,
        name,
    };

    println!("spawned '{}' id={} sp={:#x}", name, id, sp);
}

fn pick_next(current: usize) -> usize {
    unsafe {
        if COUNT == 0 { return IDLE; }

        // Найти максимальный приоритет среди Ready
        let mut best_prio: u8 = 0;
        let mut any = false;
        for i in 0..COUNT {
            if THREADS[i].state == State::Ready {
                any = true;
                if THREADS[i].priority > best_prio {
                    best_prio = THREADS[i].priority;
                }
            }
        }
        if !any { return IDLE; }

        // RR среди Ready с best_prio, начиная с current+1
        let start = if current == IDLE { 0 } else { (current + 1) % COUNT };
        for i in 0..COUNT {
            let idx = (start + i) % COUNT;
            if THREADS[idx].state == State::Ready && THREADS[idx].priority == best_prio {
                return idx;
            }
        }
        IDLE
    }
}

pub unsafe fn on_tick(old_sp: u64) -> u64 {
    // Сохранить контекст текущего
    if CURRENT == IDLE {
        IDLE_SP = old_sp;
    } else {
        THREADS[CURRENT].sp = old_sp;
        if THREADS[CURRENT].state == State::Running {
            THREADS[CURRENT].state = State::Ready;
        }
    }

    // Просыпаем тех, чьё время пришло
    for i in 0..COUNT {
        if THREADS[i].state == State::Sleeping && THREADS[i].sleep_until <= TICK {
            THREADS[i].state = State::Ready;
        }
    }
    TICK += 1;

    // Выбрать следующего
    let next = pick_next(CURRENT);
    CURRENT = next;

    if next == IDLE {
        IDLE_SP
    } else {
        THREADS[next].state = State::Running;
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
        unsafe { asm!("wfe") }
    }
}

pub fn tick_count() -> u64 {
    unsafe { TICK }
}