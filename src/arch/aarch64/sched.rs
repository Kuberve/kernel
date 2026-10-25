use core::arch::asm;
use crate::println;
use crate::ipc::{Queue, Message, EMPTY_QUEUE};

pub const STACK_SIZE: usize = 64 * 1024;
pub const MAX_THREADS: usize = 16;
pub const IDLE: usize = usize::MAX;

// Размер контекста: q0..q31 (512) + FPCR/FPSR (16) + ELR/SPSR (16) + 16 пар GPR (256) = 800
const CTX_SIZE: u64 = 800;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum State {
    Ready,
    Running,
    Sleeping,
    Blocked,
    Dead,
}

#[allow(dead_code)]
#[derive(Clone, Copy)]
pub struct Thread {
    pub sp: u64,
    pub sp_el0: u64,
    pub state: State,
    pub sleep_until: u64,
    pub priority: u8,
    pub wait_ticks: u32,
    pub blocked_on_send: usize,
    pub brk: u64,
    pub id: u32,
    pub name: &'static str,
    pub as_: crate::arch::aarch64::mmu::AddressSpace,
    pub exit_code: u64,     // SYS_WAIT return value; u64::MAX = killed
    pub waiting_on: usize,  // pid, которого ждём; usize::MAX = не ждёт
}

const EMPTY_THREAD: Thread = Thread {
    sp: 0,
    sp_el0: 0,
    state: State::Dead,
    sleep_until: 0,
    priority: 0,
    wait_ticks: 0,
    blocked_on_send: usize::MAX,
    brk: 0,
    id: 0,
    name: "",
    as_: crate::arch::aarch64::mmu::EMPTY_AS,
    exit_code: 0,
    waiting_on: usize::MAX,
};

static mut THREADS: [Thread; MAX_THREADS] = [EMPTY_THREAD; MAX_THREADS];
static mut STACKS: [[u8; STACK_SIZE]; MAX_THREADS] =
    [[0; STACK_SIZE]; MAX_THREADS];
pub static mut QUEUES: [Queue; MAX_THREADS] = [EMPTY_QUEUE; MAX_THREADS];
static mut BOOT_AS: crate::arch::aarch64::mmu::AddressSpace =
    crate::arch::aarch64::mmu::EMPTY_AS;
static mut COUNT: usize = 0;
static mut CURRENT: usize = IDLE;
static mut TICK: u64 = 0;
static mut IDLE_SP: u64 = 0;

pub fn init(boot_as: crate::arch::aarch64::mmu::AddressSpace) {
    unsafe {
        COUNT = 0;
        CURRENT = IDLE;
        TICK = 0;
        IDLE_SP = 0;
        BOOT_AS = boot_as;
        for i in 0..MAX_THREADS {
            QUEUES[i] = EMPTY_QUEUE;
        }
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
        blocked_on_send: usize::MAX,
        brk: 0,
        id: id as u32,
        name,
        as_: crate::arch::aarch64::mmu::create_as(id, (id + 1) as u16),
        exit_code: 0,
        waiting_on: usize::MAX,
    };

    println!("spawned '{}' id={} sp={:#x}", name, id, sp);
}

pub unsafe fn spawn_el0(
    name: &'static str,
    priority: u8,
    user_entry: u64,
    usp: u64,
    elf_bytes: &[u8],
) {
    let id = COUNT;
    if id >= MAX_THREADS {
        println!("sched: max threads");
        return;
    }
    COUNT += 1;

    // Сохранить caller's TTBR0 (boot AS при вызове из kmain,
    // init AS при вызове из sys_spawn SVC handler'а)
    let caller_ttbr0: u64;
    asm!("mrs {}, TTBR0_EL1", out(reg) caller_ttbr0);

    // Сохранить caller's TTBR0 (boot AS при вызове из kmain,
    // init AS при вызове из sys_spawn SVC handler'а)
    let caller_ttbr0: u64;
    asm!("mrs {}, TTBR0_EL1", out(reg) caller_ttbr0);

    // КРИТИЧНО: activate(&BOOT_AS) перед create_as.
    //
    // phys::alloc_page() уже раздал ~285 страниц (NVMe, FAT32, virtio-net 256 RX,
    // gpu, kbd, tablet), следующая выдача — PA > 0x40400000.
    // create_as пишет в l0/l1/l2 по VA = PA (identity), что верно только в BOOT AS.
    // В init AS (l2[2..5] per-thread) VA 0x404XXXXX → PA 0x490XXXXX →
    // page tables создаются в code region, activate(&as_) читает нули → fault.
    if BOOT_AS.valid {
        crate::arch::aarch64::mmu::activate(&BOOT_AS);
    }

    let as_ = crate::arch::aarch64::mmu::create_as(id, (id + 1) as u16);
    crate::arch::aarch64::mmu::activate(&as_);
    crate::lib::loader::load_elf(elf_bytes);

    asm!("msr ttbr0_el1, {}", in(reg) caller_ttbr0);
    asm!("dsb sy");
    asm!("isb");

    let top = (STACKS[id].as_ptr() as u64 + STACK_SIZE as u64) & !0xF;
    let sp = top - CTX_SIZE;
    let p = sp as *mut u64;
    for i in 0..(CTX_SIZE / 8) {
        core::ptr::write_volatile(p.add(i as usize), 0);
    }
    core::ptr::write_volatile(p.add(0), user_entry);
    core::ptr::write_volatile(p.add(1), 0x000u64);

    THREADS[id] = Thread {
        sp,
        sp_el0: usp,
        state: State::Ready,
        sleep_until: 0,
        priority,
        wait_ticks: 0,
        blocked_on_send: usize::MAX,
        brk: 0x40800000,
        id: id as u32,
        name,
        as_,
        exit_code: 0,
        waiting_on: usize::MAX,
    };

    println!("spawned '{}' (EL0) id={} ksp={:#x} usp={:#x}", name, id, sp, usp);
}

/// Спавнит поток из уже загруженного в .bss буфера USER.ELF.
/// Вызывается из SYS_SPAWN handler'а — использует pre-loaded буфер
/// (identity во всех AS через l2[1]), а не phys::alloc_page (который
/// в user AS мапится не в identity).
pub unsafe fn spawn_el0_from_buf(
    name: &'static str,
    priority: u8,
    elf_bytes: &[u8],
) -> Option<usize> {
    let id_before = COUNT;
    // Stack region: 0x40600000..0x40800000 (l2[3]), heap: 0x40800000 (l2[4]).
    // 128 KB per thread, рост вниз от 0x407E0000.
    // id=0 → 0x407E0000, id=1 → 0x407C0000, ..., id=15 → 0x40600000.
    let usp = 0x40800000u64 - ((id_before as u64) + 1) * 0x20000;
    spawn_el0(name, priority, 0x40400000, usp, elf_bytes);
    if COUNT > id_before { Some(id_before) } else { None }
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
        // ВАЖНО: kernel kmain работает в identity-маппинге boot AS.
        // После того как IRQ вытеснил kmain и scheduler ушёл в user AS,
        // при возврате в IDLE TTBR0 остался от user → kmain пишет в чужие PA.
        // Восстанавливаем boot AS.
        if BOOT_AS.valid {
            crate::arch::aarch64::mmu::activate(&BOOT_AS);
        }
        IDLE_SP
    } else {
        THREADS[next].state = State::Running;
        if THREADS[next].as_.valid {
            crate::arch::aarch64::mmu::activate(&THREADS[next].as_);
        }
        if THREADS[next].sp_el0 != 0 {
            asm!("msr SP_EL0, {}", in(reg) THREADS[next].sp_el0);
        }
        THREADS[next].sp
    }
}

/// Поток засыпает на `ticks` тиков. Не занимает CPU.
pub unsafe fn sleep(ticks: u64) {
    if CURRENT == IDLE { return; }
    let me = CURRENT;
    THREADS[me].state = State::Sleeping;
    THREADS[me].sleep_until = TICK + ticks;

    loop {
        asm!("msr daifclr, #2");
        asm!("wfe");
        if THREADS[me].state != State::Sleeping {
            asm!("msr daifset, #2");
            return;
        }
    }
}

/// Завершить текущий поток.
pub fn exit(code: u64) -> ! {
    unsafe {
        if CURRENT != IDLE {
            let me = CURRENT;
            THREADS[me].state = State::Dead;
            THREADS[me].exit_code = code;
            QUEUES[me] = EMPTY_QUEUE;
            // разбудить всех, кто ждал отправки к нам — они увидят Dead и вернут false
            for i in 0..COUNT {
                if THREADS[i].state == State::Blocked && THREADS[i].blocked_on_send == me {
                    THREADS[i].state = State::Ready;
                }
            }
            // разбудить всех, кто ждёт нашего завершения через sys_wait
            for i in 0..COUNT {
                if THREADS[i].state == State::Blocked && THREADS[i].waiting_on == me {
                    THREADS[i].state = State::Ready;
                }
            }
        }
    }
    loop {
        unsafe {
            asm!("msr daifclr, #2");
            asm!("wfe");
        }
    }
}

#[allow(dead_code)]
pub fn tick_count() -> u64 {
    unsafe { TICK }
}

pub fn current_pid() -> u64 {
    unsafe {
        if CURRENT == IDLE { 0 } else { CURRENT as u64 }
    }
}

pub unsafe fn on_yield(old_sp: u64) -> u64 {
    if COUNT == 0 { return old_sp; }

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
        if BOOT_AS.valid {
            crate::arch::aarch64::mmu::activate(&BOOT_AS);
        }
        IDLE_SP
    } else {
        THREADS[next].state = State::Running;
        if THREADS[next].as_.valid {
            crate::arch::aarch64::mmu::activate(&THREADS[next].as_);
        }
        if THREADS[next].sp_el0 != 0 {
            asm!("msr SP_EL0, {}", in(reg) THREADS[next].sp_el0);
        }
        THREADS[next].sp
    }
}

/// Поток отдаёт CPU добровольно.
#[allow(dead_code)]
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

/// Заблокировать текущий поток до прихода IPC-сообщения.
/// Вызывается из sync_handler с замаскированным IRQ.
pub unsafe fn block_on_ipc() {
    if CURRENT == IDLE {
        return;
    }
    THREADS[CURRENT].state = State::Blocked;
    loop {
        asm!("msr daifclr, #2");
        asm!("wfe");
        if THREADS[CURRENT].state != State::Blocked {
            asm!("msr daifset, #2");
            return;
        }
    }
}

/// Разбудить поток, если он Blocked. Если Ready/Running/Sleeping — ничего.
pub unsafe fn wake_thread(pid: usize) {
    if pid >= MAX_THREADS {
        return;
    }
    if THREADS[pid].state == State::Blocked {
        THREADS[pid].state = State::Ready;
    }
}

/// Отправить сообщение. Не блокируется. Возвращает false, если очередь полна
/// или получатель мёртв.
pub unsafe fn ipc_send(target: usize, msg: Message) -> bool {
    if target >= MAX_THREADS { return false; }
    if THREADS[target].state == State::Dead { return false; }
    if !QUEUES[target].push(msg) { return false; }
    wake_thread(target);
    true
}

pub unsafe fn ipc_send_blocking(target: usize, msg: Message) -> bool {
    if target >= MAX_THREADS { return false; }
    if THREADS[target].state == State::Dead { return false; }
    loop {
        if QUEUES[target].push(msg) {
            wake_thread(target);
            return true;
        }
        // Очередь полна: блокируемся, пока recv не освободит место
        if CURRENT == IDLE { return false; }
        THREADS[CURRENT].state = State::Blocked;
        THREADS[CURRENT].blocked_on_send = target;
        loop {
            asm!("msr daifclr, #2");
            asm!("wfe");
            if THREADS[CURRENT].state != State::Blocked { break; }
        }
        asm!("msr daifset, #2");
        THREADS[CURRENT].blocked_on_send = usize::MAX;
        // target мог умереть, пока мы ждали
        if THREADS[target].state == State::Dead { return false; }
        // пытаемся снова
    }
}

unsafe fn wake_one_sender(target: usize) {
    for i in 0..COUNT {
        if THREADS[i].state == State::Blocked && THREADS[i].blocked_on_send == target {
            THREADS[i].state = State::Ready;
            return;
        }
    }
}

/// Попытаться получить сообщение без блокировки.
pub unsafe fn ipc_recv_nonblock() -> Option<Message> {
    if CURRENT == IDLE { return None; }
    let m = QUEUES[CURRENT].pop()?;
    // место освободилось — будим одного ожидающего отправителя
    wake_one_sender(CURRENT);
    Some(m)
}

/// Получить сообщение, блокируясь до прихода.
pub unsafe fn ipc_recv_block() -> Message {
    loop {
        if let Some(m) = ipc_recv_nonblock() {
            return m;
        }
        block_on_ipc();
    }
}

pub unsafe fn thread_brk(pid: usize) -> u64 {
    if pid >= MAX_THREADS { return 0; }
    THREADS[pid].brk
}

pub unsafe fn set_thread_brk(pid: usize, new: u64) {
    if pid >= MAX_THREADS { return; }
    THREADS[pid].brk = new;
}

/// Принудительно завершить поток pid.
/// Возвращает true если убит, false если pid невалиден / Dead / CURRENT / IDLE.
pub unsafe fn kill(pid: usize) -> bool {
    if pid >= COUNT { return false; }
    if pid == IDLE { return false; }
    if pid == CURRENT { return false; }   // self-kill не поддерживаем
    if THREADS[pid].state == State::Dead { return false; }

    THREADS[pid].state = State::Dead;
    THREADS[pid].exit_code = u64::MAX;   // -1 = killed (не exit)
    QUEUES[pid] = EMPTY_QUEUE;

    // разбудить blocked senders
    for i in 0..COUNT {
        if THREADS[i].state == State::Blocked && THREADS[i].blocked_on_send == pid {
            THREADS[i].state = State::Ready;
        }
    }
    // разбудить всех ждущих завершения pid
    for i in 0..COUNT {
        if THREADS[i].state == State::Blocked && THREADS[i].waiting_on == pid {
            THREADS[i].state = State::Ready;
        }
    }
    true
}

/// Заблокироваться до завершения потока target_pid. Возвращает exit_code.
/// u64::MAX — ошибка (pid невалиден, self-wait, CURRENT == IDLE).
pub unsafe fn wait(target_pid: usize) -> u64 {
    if target_pid >= COUNT { return u64::MAX; }
    if target_pid == IDLE { return u64::MAX; }
    if target_pid == CURRENT { return u64::MAX; }
    if CURRENT == IDLE { return u64::MAX; }

    if THREADS[target_pid].state == State::Dead {
        return THREADS[target_pid].exit_code;
    }

    let me = CURRENT;
    THREADS[me].waiting_on = target_pid;
    THREADS[me].state = State::Blocked;

    loop {
        asm!("msr daifclr, #2");
        asm!("wfe");
        if THREADS[me].state != State::Blocked {
            break;
        }
    }
    asm!("msr daifset, #2");   // перемаскировать перед возвратом в sync_handler

    THREADS[me].waiting_on = usize::MAX;
    THREADS[target_pid].exit_code
}