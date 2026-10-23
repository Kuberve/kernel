use core::sync::atomic::{AtomicU64, Ordering};
use core::arch::asm;
use crate::println;

static TICKS: AtomicU64 = AtomicU64::new(0);
static PERIOD_TICKS: AtomicU64 = AtomicU64::new(0);

pub const TICK_HZ: u64 = 100;

fn freq() -> u64 {
    let f: u64;
    unsafe { asm!("mrs {}, CNTFRQ_EL0", out(reg) f) };
    f
}

pub fn init() {
    let f = freq();
    PERIOD_TICKS.store(f / TICK_HZ, Ordering::Relaxed);
    TICKS.store(0, Ordering::Relaxed);
}

pub fn start() {
    let tval = PERIOD_TICKS.load(Ordering::Relaxed);
    unsafe {
        asm!("msr CNTP_TVAL_EL0, {}", in(reg) tval);
        asm!("msr CNTP_CTL_EL0, {}", in(reg) 1u64);
    }
}

fn recharge() {
    let tval = PERIOD_TICKS.load(Ordering::Relaxed);
    unsafe { asm!("msr CNTP_TVAL_EL0, {}", in(reg) tval); }
}

pub fn handle_irq() {
    TICKS.fetch_add(1, Ordering::Relaxed);
    recharge();
}

pub fn tick_count() -> u64 { TICKS.load(Ordering::Relaxed) }