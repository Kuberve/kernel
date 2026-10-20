#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

use core::arch::{asm, global_asm};

mod allocator;

mod arch;
use crate::arch::aarch64::{gic, timer, vectors, sched};

mod device;

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

    // --- MMU + phys (порядок важен) ---
    println!("Initializing MMU...");
    unsafe { crate::arch::aarch64::mmu::init(); }

    extern "C" { static __kernel_end: u8; }
    let kernel_end = unsafe { &__kernel_end as *const _ as usize };
    // 96MB под kernel + heap (phys allocator). Stack regions (0x46000000+) — за пределами,
    // mmu::create_as использует их по фиксированным адресам.
    let ram_end = 0x46000000;
    println!("phys: heap from {:#x} to {:#x}", kernel_end, ram_end);
    unsafe { crate::arch::aarch64::phys::init(kernel_end, ram_end); }

    let boot_as = unsafe { crate::arch::aarch64::mmu::create_as(0) };
    unsafe { crate::arch::aarch64::mmu::activate(&boot_as); }
    unsafe { crate::arch::aarch64::mmu::enable(); }
    println!("MMU enabled.");

    let nvme_bar = unsafe { device::pci::init() };
    if let Some(bar) = nvme_bar {
        println!("NVMe BAR0 = {:#x}", bar);

        // CAP (offset 0x0), 64-bit
        let cap_lo = unsafe { core::ptr::read_volatile(bar as *const u32) };
        let cap_hi = unsafe { core::ptr::read_volatile((bar + 4) as *const u32) };
        let cap = ((cap_hi as u64) << 32) | (cap_lo as u64);
        println!("NVMe CAP = {:#x}", cap);

        // VS (offset 0x8): version register, 32-bit
        let vs = unsafe { core::ptr::read_volatile((bar + 8) as *const u32) };
        println!("NVMe VS = {:#x}", vs);

        // CSTS (offset 0x1C): Controller Status
        let csts = unsafe { core::ptr::read_volatile((bar + 0x1C) as *const u32) };
        println!("NVMe CSTS = {:#x}", csts);
    }
    if let Some(bar) = nvme_bar {
        let mut nvme = match unsafe { device::nvme::init(bar) } {
            Some(n) => n,
            None => {
                println!("NVMe init FAILED");
                panic!("NVMe init failed");
            }
        };
        println!("NVMe initialized OK");

        // Identify Controller (CNS=0x01)
        let id_ctrl_pa = unsafe { crate::arch::aarch64::phys::alloc_page().expect("no page for identify") };
        unsafe { core::ptr::write_bytes(id_ctrl_pa as *mut u8, 0, 4096); }
        if let Some(id) = unsafe { device::nvme::identify_controller(&mut nvme, id_ctrl_pa as u64) } {
            let model = core::str::from_utf8(&id.model).unwrap_or("?").trim_end();
            let serial = core::str::from_utf8(&id.serial).unwrap_or("?").trim_end();
            let fw = core::str::from_utf8(&id.firmware).unwrap_or("?").trim_end();
            println!("NVMe model: '{}'", model);
            println!("NVMe serial: '{}'", serial);
            println!("NVMe firmware: '{}'", fw);
            println!("NVMe namespaces: {}", id.nn);
        }

        // Identify Namespace 1 (CNS=0x00)
        let id_ns_pa = unsafe { crate::arch::aarch64::phys::alloc_page().expect("no page for ns") };
        unsafe { core::ptr::write_bytes(id_ns_pa as *mut u8, 0, 4096); }
        if let Some(ns) = unsafe { device::nvme::identify_namespace(&mut nvme, id_ns_pa as u64, 1) } {
            println!("NVMe NS1: nsze={} blocks, lbads={} (block size = {})",
                ns.nsze, ns.lbads, 1u64 << ns.lbads);
            println!("NVMe NS1: capacity = {} bytes", ns.nsze * (1u64 << ns.lbads));
        }

        // --- I/O queues ---
        if unsafe { device::nvme::create_io_queues(&mut nvme) } {
            println!("NVMe: I/O queues ready");

            // Read block 0
            let io_buf_pa = unsafe { crate::arch::aarch64::phys::alloc_page().expect("no page for io buf") };
            unsafe { core::ptr::write_bytes(io_buf_pa as *mut u8, 0xCC, 4096); }

            if unsafe { device::nvme::read_blocks(&nvme, 1, 0, 1, io_buf_pa as u64) } {
                println!("NVMe: read block 0 OK");
                // Вывести первые 32 байта в hex
                let buf = io_buf_pa as *const u8;
                print!("NVMe: block 0 = ");
                for i in 0..32 {
                    let b = unsafe { core::ptr::read_volatile(buf.add(i)) };
                    print!("{:02x}", b);
                }
                println!();
            } else {
                println!("NVMe: read block 0 FAILED");
            }

            // --- Тест записи + чтения ---
            let test_pa = unsafe { crate::arch::aarch64::phys::alloc_page().expect("no page") };
            let test_msg = b"Hello NVMe!";
            unsafe {
                core::ptr::write_bytes(test_pa as *mut u8, 0, 4096);
                for i in 0..test_msg.len() {
                    core::ptr::write_volatile((test_pa as *mut u8).add(i), test_msg[i]);
                }
            }
            if unsafe { device::nvme::write_blocks(&nvme, 1, 100, 1, test_pa as u64) } {
                println!("NVMe: write block 100 OK");
            } else {
                println!("NVMe: write block 100 FAILED");
            }

            // Read back
            unsafe { core::ptr::write_bytes(io_buf_pa as *mut u8, 0, 4096); }
            if unsafe { device::nvme::read_blocks(&nvme, 1, 100, 1, io_buf_pa as u64) } {
                let buf = io_buf_pa as *const u8;
                let mut ok = true;
                for i in 0..test_msg.len() {
                    let b = unsafe { core::ptr::read_volatile(buf.add(i)) };
                    if b != test_msg[i] { ok = false; }
                }
                if ok {
                    println!("NVMe: write+read verify OK");
                } else {
                    println!("NVMe: verify FAILED");
                }
            }
        }
    }

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

    sched::init(boot_as);
    unsafe {
        sched::spawn("A", 1, thread_a);
        sched::spawn("B", 2, thread_b);
        sched::spawn_el0("uart",    2, 0x40400000, 0x40700000, USER_ELF);
        sched::spawn_el0("client1", 2, 0x40400000, 0x40700000, USER_ELF);
        sched::spawn_el0("client2", 2, 0x40400000, 0x40700000, USER_ELF);
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