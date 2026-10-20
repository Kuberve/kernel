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
    let mut nvme_opt: Option<device::nvme::Nvme> = None;
    let mut fat32_fs: Option<device::fat32::Fat32> = None;

    if let Some(bar) = nvme_bar {
        println!("NVMe BAR0 = {:#x}", bar);

        let cap_lo = unsafe { core::ptr::read_volatile(bar as *const u32) };
        let cap_hi = unsafe { core::ptr::read_volatile((bar + 4) as *const u32) };
        let cap = ((cap_hi as u64) << 32) | (cap_lo as u64);
        println!("NVMe CAP = {:#x}", cap);
        let vs = unsafe { core::ptr::read_volatile((bar + 8) as *const u32) };
        println!("NVMe VS = {:#x}", vs);
        let csts = unsafe { core::ptr::read_volatile((bar + 0x1C) as *const u32) };
        println!("NVMe CSTS = {:#x}", csts);

        let mut nvme = match unsafe { device::nvme::init(bar) } {
            Some(n) => n,
            None => panic!("NVMe init failed"),
        };
        println!("NVMe initialized OK");

        // Identify Controller
        let id_ctrl_pa = unsafe {
            crate::arch::aarch64::phys::alloc_page().expect("no page for identify")
        };
        unsafe { core::ptr::write_bytes(id_ctrl_pa as *mut u8, 0, 4096); }
        if let Some(id) = unsafe {
            device::nvme::identify_controller(&mut nvme, id_ctrl_pa as u64)
        } {
            let model = core::str::from_utf8(&id.model).unwrap_or("?").trim_end();
            let serial = core::str::from_utf8(&id.serial).unwrap_or("?").trim_end();
            println!("NVMe model: '{}'", model);
            println!("NVMe serial: '{}'", serial);
        }

        // Identify Namespace 1
        let id_ns_pa = unsafe {
            crate::arch::aarch64::phys::alloc_page().expect("no page for ns")
        };
        unsafe { core::ptr::write_bytes(id_ns_pa as *mut u8, 0, 4096); }
        if let Some(ns) = unsafe {
            device::nvme::identify_namespace(&mut nvme, id_ns_pa as u64, 1)
        } {
            println!("NVMe NS1: nsze={} lbads={}", ns.nsze, ns.lbads);
        }

        // I/O queues
        if unsafe { device::nvme::create_io_queues(&mut nvme) } {
            println!("NVMe: I/O queues ready");

            // Read block 0
            let io_buf_pa = unsafe {
                crate::arch::aarch64::phys::alloc_page().expect("no page for io buf")
            };
            unsafe { core::ptr::write_bytes(io_buf_pa as *mut u8, 0, 4096); }

            if unsafe { device::nvme::read_blocks(&nvme, 1, 0, 1, io_buf_pa as u64) } {
                println!("NVMe: read block 0 OK");
            }

            // Write + verify: используем последний сектор (безопасно, не в FAT)
            let test_lba = (131072 - 1) as u64;  // total_sectors - 1 из mount
            let test_pa = unsafe {
                crate::arch::aarch64::phys::alloc_page().expect("no page")
            };
            let test_msg = b"Hello NVMe!";
            unsafe {
                core::ptr::write_bytes(test_pa as *mut u8, 0, 4096);
                for i in 0..test_msg.len() {
                    core::ptr::write_volatile((test_pa as *mut u8).add(i), test_msg[i]);
                }
            }
            if unsafe { device::nvme::write_blocks(&nvme, 1, test_lba, 1, test_pa as u64) } {
                println!("NVMe: write LBA {} OK", test_lba);
            }

            unsafe { core::ptr::write_bytes(io_buf_pa as *mut u8, 0, 4096); }
            if unsafe { device::nvme::read_blocks(&nvme, 1, test_lba, 1, io_buf_pa as u64) } {
                let buf = io_buf_pa as *const u8;
                let mut ok = true;
                for i in 0..test_msg.len() {
                    let b = unsafe { core::ptr::read_volatile(buf.add(i)) };
                    if b != test_msg[i] { ok = false; }
                }
                if ok {
                    println!("NVMe: write+read LBA {} verify OK", test_lba);
                } else {
                    println!("NVMe: verify FAILED");
                }
            }
        }

        // Mount FAT32
        match unsafe { device::fat32::mount(&nvme) } {
            Ok(fs) => {
                println!("FAT32 mounted: label='{}'", fs.label_str());

                println!("--- Root directory ---");
                unsafe { fs.readdir(fs.root_cluster); }
                println!("--- End of root ---");

                println!("--- Reading HELLO.TXT ---");
                if let Some((cluster, size, is_dir)) = unsafe {
                    fs.find_entry(fs.root_cluster, "HELLO.TXT")
                } {
                    if !is_dir {
                        let buf_pa = unsafe {
                            crate::arch::aarch64::phys::alloc_page().expect("no page for file")
                        };
                        unsafe { core::ptr::write_bytes(buf_pa as *mut u8, 0, 4096); }
                        match unsafe { fs.read_file(cluster, size, buf_pa as u64) } {
                            Some(n) => {
                                let bytes = unsafe {
                                    core::slice::from_raw_parts(buf_pa as *const u8, n)
                                };
                                let s = core::str::from_utf8(bytes).unwrap_or("<binary>");
                                print!("HELLO.TXT content: '");
                                print!("{}", s);
                                println!("'");
                            }
                            None => println!("read_file failed"),
                        }
                    }
                } else {
                    println!("HELLO.TXT not found");
                }
                println!("--- End ---");

                fat32_fs = Some(fs);
            }
            Err(e) => {
                println!("FAT32 mount failed: {:?}", e);
            }
        }

        nvme_opt = Some(nvme);
    }

    // --- Загрузить user.elf с диска (только в буфер, не в AS) ---
    let mut user_elf: &[u8] = USER_ELF;   // fallback
    if let Some(ref fs) = fat32_fs {
        if let Some((cluster, size, is_dir)) = unsafe {
            fs.find_entry(fs.root_cluster, "USER.ELF")
        } {
            if !is_dir && size as usize <= 131072 {
                let buf_pa = &raw const USER_ELF_BUF as u64;
                println!("Loading user.elf from disk: {} bytes", size);
                match unsafe { fs.read_file(cluster, size, buf_pa) } {
                    Some(n) => {
                        user_elf = unsafe {
                            core::slice::from_raw_parts(buf_pa as *const u8, n)
                        };
                        println!("Loaded {} bytes from disk", n);
                    }
                    None => println!("read_file failed, using include_bytes fallback"),
                }
            }
        }
    }

    // (спать не надо, но переменные живы до конца kmain)
    let _ = &nvme_opt;
    let _ = &fat32_fs;

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