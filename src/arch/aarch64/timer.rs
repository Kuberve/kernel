use core::sync::atomic::{AtomicU64, Ordering};
use core::arch::asm;
use crate::println;

static TICKS: AtomicU64 = AtomicU64::new(0);

fn freq() -> u64 {
    let f: u64;
    unsafe { asm!("mrs {}, CNTFRQ_EL0", out(reg) f) };
    f
}

pub fn init() {
    // Пока ничего не настраиваем — start() делает всю работу.
    TICKS.store(0, Ordering::Relaxed);
}

/// Запустить таймер с интервалом в секундах.
pub fn start(seconds: u64) {
    let tval = freq() * seconds;
    unsafe {
        asm!("msr CNTP_TVAL_EL0, {}", in(reg) tval);
        asm!("msr CNTP_CTL_EL0, {}", in(reg) 1u64);
    }
}

fn recharge() {
    unsafe {
        asm!("msr CNTP_TVAL_EL0, {}", in(reg) freq());
    }
}

pub fn handle_irq() {
    let n = TICKS.fetch_add(1, Ordering::Relaxed) + 1;
    println!("tick {}", n);
    recharge();
}