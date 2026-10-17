#![no_std]
#![no_main]

use core::arch::{asm, global_asm};

mod arch;
use crate::arch::aarch64::{gic, timer, vectors};

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

#[no_mangle]
pub extern "C" fn kmain() -> ! {
    // Разрешаем FP/SIMD на EL1 — иначе компилятор может случайно
    // сгенерировать SIMD и упасть с EC=0x07.
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
    timer::start(1);

    println!("  VBAR_EL1  = {:#x}", read_vbar());

    println!("Enabling interrupts...");
    unsafe { asm!("msr daifclr, #2") };

    println!("Kernel alive.");

    loop {
        unsafe { asm!("wfe") }
    }
}