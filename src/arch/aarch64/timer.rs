use core::sync::atomic::{AtomicU64, Ordering};

static TICKS: AtomicU64 = AtomicU64::new(0);

fn freq() -> u64 {
    let f: u64;
    unsafe { core::arch::asm!("mrs {}, CNTFRQ_EL0", out(reg) f) };
    f
}

pub fn init() {
    let f = freq();
    unsafe {
        // Отсчёт на 1 секунду вперёд
        core::arch::asm!("msr CNTP_TVAL_EL0, {}", in(reg) f);
        // ENABLE=1, IMASK=0
        core::arch::asm!("msr CNTP_CTL_EL0, {}", in(reg) 1u64);
    }
}

pub fn tick() {
    let n = TICKS.fetch_add(1, Ordering::Relaxed) + 1;

    // Перезаряжаем
    let f = freq();
    unsafe {
        core::arch::asm!("msr CNTP_TVAL_EL0, {}", in(reg) f);
    }

    // Печатаем "tick N"
    crate::uart::puts("tick ");
    print_num(n);
    crate::uart::puts("\n");
}

fn print_num(mut n: u64) {
    if n == 0 {
        crate::uart::putc(b'0');
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = 0;
    while n > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    while i > 0 {
        i -= 1;
        crate::uart::putc(buf[i]);
    }
}