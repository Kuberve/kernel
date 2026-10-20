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

static mut USER_ELF_BUF: [u8; 131072] = [0; 131072];

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

    // --- PCIe + NVMe ---
    let nvme_bar = unsafe { device::pci::init() };
    let mut nvme = match nvme_bar {
        Some(bar) => {
            println!("NVMe BAR0 = {:#x}", bar);
            match unsafe { device::nvme::init(bar) } {
                Some(n) => n,
                None => panic!("NVMe init failed"),
            }
        }
        None => panic!("no NVMe found"),
    };
    println!("NVMe initialized OK");

    // Identify Controller
    let id_ctrl_pa = unsafe { crate::arch::aarch64::phys::alloc_page().expect("no page") };
    unsafe { core::ptr::write_bytes(id_ctrl_pa as *mut u8, 0, 4096); }
    if let Some(id) = unsafe { device::nvme::identify_controller(&mut nvme, id_ctrl_pa as u64) } {
        let model = core::str::from_utf8(&id.model).unwrap_or("?").trim_end();
        println!("NVMe model: '{}'", model);
    }

    // Identify Namespace 1
    let id_ns_pa = unsafe { crate::arch::aarch64::phys::alloc_page().expect("no page") };
    unsafe { core::ptr::write_bytes(id_ns_pa as *mut u8, 0, 4096); }
    if let Some(ns) = unsafe { device::nvme::identify_namespace(&mut nvme, id_ns_pa as u64, 1) } {
        println!("NVMe NS1: nsze={} lbads={}", ns.nsze, ns.lbads);
    }

    // I/O queues
    if !unsafe { device::nvme::create_io_queues(&mut nvme) } {
        panic!("I/O queues failed");
    }
    println!("NVMe: I/O queues ready");

    // Mount FAT32 (глобально)
    match unsafe { device::fat32::mount_global(&nvme) } {
        Ok(()) => {
            let fs = device::fat32::get().unwrap();
            println!("FAT32 mounted: label='{}'", fs.label_str());
        }
        Err(e) => panic!("FAT32 mount failed: {:?}", e),
    }

    // Загрузить user.elf с диска
    static mut USER_ELF_BUF: [u8; 131072] = [0; 131072];
    let mut user_elf: &[u8] = USER_ELF;
    {
        let fs = device::fat32::get().unwrap();
        if let Some((cluster, size, is_dir)) = unsafe { fs.find_entry(fs.root_cluster, "USER.ELF") } {
            if !is_dir && size as usize <= 131072 {
                println!("Loading user.elf from disk: {} bytes", size);
                let buf_pa = &raw const USER_ELF_BUF as u64;
                match unsafe { fs.read_file(cluster, size, buf_pa) } {
                    Some(n) => {
                        user_elf = unsafe { core::slice::from_raw_parts(buf_pa as *const u8, n) };
                        println!("Loaded {} bytes from disk", n);
                    }
                    None => println!("read_file failed, using include_bytes"),
                }
            }
        }
    }

    sched::init(boot_as);
    unsafe {
        sched::spawn("A", 1, thread_a);
        sched::spawn("B", 2, thread_b);
        sched::spawn_el0("uart",    2, 0x40400000, 0x40700000, user_elf);
        sched::spawn_el0("client1", 2, 0x40400000, 0x40700000, user_elf);
        sched::spawn_el0("client2", 2, 0x40400000, 0x40700000, user_elf);
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