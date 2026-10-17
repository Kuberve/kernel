#![no_std]
#![no_main]

use core::arch::{asm, global_asm};

mod arch;
use crate::arch::aarch64::{gic, timer, vectors, sched};

mod uart;
mod panic;

global_asm!(
    ".section .text.boot",
    ".global _start",
    "_start:",
    "  ldr x0, =_stack_top",
    "  mov sp, x0",
    "  bl kmain",
    "1:",
    "  wfe",
    "  b 1b",
);

fn read_el() -> u64 {
    let el: u64;
    unsafe { asm!("mrs {}, CurrentEL", out(reg) el) };
    (el >> 2) & 3
}

fn read_vbar() -> u64 {
    let v: u64;
    unsafe { asm!("mrs {}, VBAR_EL1", out(reg) v) };
    v
}

extern "C" fn thread_a() -> ! {
    let mut n = 0u64;
    loop {
        n += 1;
        println!("A {}", n);
        sched::yield_now();
    }
}

extern "C" fn thread_b() -> ! {
    let mut n = 0u64;
    loop {
        n += 1;
        println!("B {}", n);
        sched::yield_now();
    }
}

#[no_mangle]
pub extern "C" fn kmain() -> ! {
    unsafe {
        let cpacr: u64 = 3 << 20;
        asm!("msr CPACR_EL1, {}", in(reg) cpacr);
        asm!("isb");
    }

    println!("Kuberve booting...");
    println!("  CurrentEL = EL{}", read_el());

    vectors::init();
    gic::init();
    timer::init();

    sched::init();
    unsafe {
        sched::spawn("A", 1, thread_a);
        sched::spawn("B", 3, thread_b);
    }

    timer::start(1);

    println!("  VBAR_EL1  = {:#x}", read_vbar());
    println!("Enabling interrupts...");
    unsafe { asm!("msr daifclr, #2") };

    println!("Kernel alive. Main becomes idle.");

    // kmain превращается в idle-поток
    loop {
        unsafe { asm!("wfe") }
    }
}