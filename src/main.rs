#![no_std]
#![no_main]

use core::arch::{asm, global_asm};

mod panic;
mod uart;

// Entry point for the kernel, written in assembly
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

#[no_mangle]
pub extern "C" fn kmain() -> ! {
    uart::puts("Kuberve booting...\n");
    uart::puts("Hello from aarch64!\n");
    uart::puts("Kernel alive.\n");

    loop {
        unsafe { asm!("wfe") }
    }
}