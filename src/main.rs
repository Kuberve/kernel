#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

extern crate alloc;

use core::arch::{asm, global_asm};

mod allocator;

mod arch;
use crate::arch::aarch64::{gic, timer, vectors, sched};

mod device;

mod network;

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
    crate::network::socket::init();
    crate::network::tcp::init();
    unsafe { crate::network::tcp::listen(8080); }

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

    if !unsafe { device::nvme::create_io_queues(&mut nvme) } {
        panic!("I/O queues failed");
    }

    // Mount FAT32 (глобально)
    match unsafe { device::fat32::mount_global(&nvme) } {
        Ok(()) => {
            let fs = device::fat32::get().unwrap();
            println!("FAT32 mounted: label='{}'", fs.label_str());
        }
        Err(e) => panic!("FAT32 mount failed: {:?}", e),
    }

    // --- virtio-net ---
    println!("=== Network ===");
    let vn_init = unsafe { device::virtio_net::init() };
    if let Some(vn) = vn_init {
        println!("virtio-net initialized");
        unsafe { device::virtio_net::setup_rx(&vn); }
        unsafe { device::virtio_net::set_global(vn); }
    } else {
        println!("virtio-net NOT FOUND");
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

    println!("=== Graphics ===");
    let mut gpu_opt = unsafe { device::virtio_gpu::init() };
    if let Some(mut gpu) = gpu_opt.take() {
        println!("virtio-gpu initialized");

        // Настроить framebuffer
        let fb_addr: u64 = 0x48000000;
        let width = 640u32;
        let height = 480u32;

        unsafe {
            // Обнулить framebuffer
            core::ptr::write_bytes(fb_addr as *mut u8, 0, (width * height * 4) as usize);

            if device::virtio_gpu::setup_framebuffer(&mut gpu, fb_addr, width, height) {
                println!("Framebuffer setup OK");

                // Нарисовать
                let ptr = fb_addr as *mut u32;
                // Тёмно-синий фон
                for i in 0..(width * height) {
                    core::ptr::write_volatile(ptr.add(i as usize), 0x00203050);
                }
                // Красный квадрат (формат BGRX: 0x00RRGGBB)
                for y in 50..150u32 {
                    for x in 50..150u32 {
                        core::ptr::write_volatile(ptr.add((y * width + x) as usize), 0x000000FF);
                    }
                }
                // Зелёный
                for y in 50..150u32 {
                    for x in 200..300u32 {
                        core::ptr::write_volatile(ptr.add((y * width + x) as usize), 0x0000FF00);
                    }
                }
                // Синий
                for y in 50..150u32 {
                    for x in 350..450u32 {
                        core::ptr::write_volatile(ptr.add((y * width + x) as usize), 0x00FF0000);
                    }
                }
                // Белая полоса
                for y in 200..250u32 {
                    for x in 50..450u32 {
                        core::ptr::write_volatile(ptr.add((y * width + x) as usize), 0x00FFFFFF);
                    }
                }

                // Отправить в host
                device::virtio_gpu::flush(&mut gpu, width, height);
                println!("Framebuffer painted and flushed");
                // Текст поверх фона
                use device::text;
                text::draw_string_wrap(50, 300, "Hello, Kuberve!", 0x00FFFFFF, Some(0x00203050));
                text::draw_string_wrap(50, 330, "Framebuffer working!", 0x00FFFF00, Some(0x00203050));
                text::draw_string_wrap(50, 360, "virtio-gpu + 8x16 font", 0x00FF00FF, Some(0x00203050));

                device::virtio_gpu::flush(&mut gpu, width, height);
                println!("Text rendered");
            } else {
                println!("Framebuffer setup FAILED");
            }
        }

        // Сохранить gpu в глобальную для будущих flush
        // (пока просто drop — flush сделан вручную)
    } else {
        println!("virtio-gpu NOT FOUND");
    }

    // --- virtio-keyboard ---
    println!("=== Keyboard ===");
    let mut kbd_opt = unsafe { device::virtio_input::init() };
    if kbd_opt.is_some() {
        println!("virtio-keyboard initialized");
    } else {
        println!("virtio-keyboard NOT FOUND");
    }

    // --- DHCP ---
    if let Some(vn) = unsafe { device::virtio_net::get_global_mut() } {
        unsafe { crate::network::dhcp::run(vn); }
    }

    // Helper: send ARP request for target IP
    unsafe fn send_arp(vn: &mut device::virtio_net::VirtioNet, target_ip: [u8; 4]) {
        let our_mac = vn.mac;
        let mut arp = [0u8; 60];
        for i in 0..6 { arp[i] = 0xFF; }
        for i in 0..6 { arp[6 + i] = our_mac[i]; }
        arp[12] = 0x08; arp[13] = 0x06;
        arp[14] = 0; arp[15] = 1;
        arp[16] = 0x08; arp[17] = 0x00;
        arp[18] = 6;
        arp[19] = 4;
        arp[20] = 0; arp[21] = 1;
        for i in 0..6 { arp[22 + i] = our_mac[i]; }
        arp[28] = 10; arp[29] = 0; arp[30] = 2; arp[31] = 15;
        arp[38] = target_ip[0]; arp[39] = target_ip[1];
        arp[40] = target_ip[2]; arp[41] = target_ip[3];

        println!("=== ARP: who has {}.{}.{}.{}? ===",
                 target_ip[0], target_ip[1], target_ip[2], target_ip[3]);
        device::virtio_net::send_packet(vn, &arp);
    }

    if let Some(vn) = unsafe { device::virtio_net::get_global_mut() } {
        unsafe { send_arp(vn, [10, 0, 2, 2]); }   // gateway
        unsafe { send_arp(vn, [10, 0, 2, 3]); }   // DNS
    }

    // --- Poll loop ---
    println!("Entering network poll loop...");
    let mut ping_sent = false;
    loop {
        if let Some(vn) = unsafe { device::virtio_net::get_global_mut() } {
            let n = unsafe { device::virtio_net::poll_rx(vn) };
            if n > 0 {
                println!("poll: processed {} packets", n);
            }

            // Keyboard
            if let Some(kbd) = kbd_opt.as_mut() {
                while let Some(ev) = unsafe { device::virtio_input::poll_events(kbd) } {
                    if ev.typ == 1 && ev.value == 1 {   // EV_KEY, press
                        if let Some(c) = device::virtio_input::keycode_to_ascii(ev.code, false) {
                            if c == b'\n' {
                                println!("[KBD] ENTER");
                            } else if c == b' ' {
                                println!("[KBD] SPACE");
                            } else {
                                println!("[KBD] '{}' (code={})", c as char, ev.code);
                            }
                        } else {
                            println!("[KBD] code={} value={}", ev.code, ev.value);
                        }
                    }
                }
            }

            // Как только ARP cache знает gateway — шлём ping один раз
            if !ping_sent {
                let gw = [10u8, 0, 2, 2];
                if unsafe { device::arp::lookup(gw) }.is_some() {
                    println!("=== Sending ICMP echo request ===");
                    unsafe { network::ping::send_ping(vn, gw); }
                    ping_sent = true;
                }
            }
        } else {
            unsafe { asm!("wfe") };
        }
    }
}