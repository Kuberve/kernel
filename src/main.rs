#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

use core::arch::{asm, global_asm};

mod allocator;

mod arch;
use crate::arch::aarch64::{gic, timer, vectors, sched};

mod uart;
mod panic;
mod loader;

static USER_ELF: &[u8] =
    include_bytes!("../../user/target/aarch64-unknown-none/release/user");

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
    let msg = b"A via syscall\n";
    let mut n = 0u64;
    loop {
        n += 1;
        crate::arch::aarch64::syscall::sys_write(1, msg.as_ptr(), msg.len() as u64);
        crate::arch::aarch64::syscall::sys_sleep(1);
        if n == 3 {
            crate::arch::aarch64::syscall::sys_exit(0);
        }
    }
}

extern "C" fn thread_b() -> ! {
    let msg = b"B via syscall\n";
    let mut n = 0u64;
    loop {
        n += 1;
        crate::arch::aarch64::syscall::sys_write(1, msg.as_ptr(), msg.len() as u64);
        crate::arch::aarch64::syscall::sys_sleep(1);
        if n == 3 {
            crate::arch::aarch64::syscall::sys_exit(0);
        }
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
    unsafe { gic::init_spi(33); }
    timer::init();

    println!("Enabling MMU...");
    unsafe { crate::arch::aarch64::mmu::init(); }
    println!("MMU enabled.");

    // Загрузить userspace ELF
    println!("Loading user ELF ({} bytes)...", USER_ELF.len());
    let user_entry = unsafe { loader::load_elf(USER_ELF) };
    println!("user entry = {:#x}", user_entry);

    extern "C" {
        static __kernel_end: u8;
    }
    let kernel_end = unsafe { &__kernel_end as *const _ as usize };
    // QEMU virt: RAM 0x40000000 + 128MB (default -m). Если меняешь -m, поменяй здесь.
    let ram_end = 0x40000000 + 128 * 1024 * 1024;
    println!("phys: heap from {:#x} to {:#x}", kernel_end, ram_end);
    unsafe { crate::arch::aarch64::phys::init(kernel_end, ram_end); }

    // Тест
    unsafe {
        let p1 = crate::arch::aarch64::phys::alloc_page().unwrap();
        let p2 = crate::arch::aarch64::phys::alloc_page().unwrap();
        println!("alloc: p1={:#x} p2={:#x} diff={}", p1, p2, p2 - p1);
        crate::arch::aarch64::phys::free_page(p1);
        let p3 = crate::arch::aarch64::phys::alloc_page().unwrap();
        println!("free+alloc: p3={:#x} (should == p1)", p3);
    }

    {
        use alloc::boxed::Box;
        use alloc::vec::Vec;

        let b = Box::new(42u64);
        println!("Box: {}", *b);

        let mut v: Vec<u32> = Vec::new();
        for i in 0..10 { v.push(i * 10); }
        println!("Vec len={} last={}", v.len(), v[v.len()-1]);

        let s = alloc::format!("hello, {}!", 42);
        println!("format: {}", s);
    }
    println!("heap OK");

    sched::init();
    unsafe {
        sched::spawn("A", 1, thread_a);
        sched::spawn("B", 2, thread_b);           // понизили до 2, чтобы user получал CPU
        sched::spawn_el0("uart",    2, user_entry, 0x40700000);
        sched::spawn_el0("client1", 2, user_entry, 0x40780000);
        sched::spawn_el0("client2", 2, user_entry, 0x407C0000);
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