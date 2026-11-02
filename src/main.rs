#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

// no_std с mutable static — архитектурное решение:
// WM, THREADS, QUEUES, драйверы живут в .bss, доступ через &mut.
// Для pre-alpha это норма, Rust 2024 warning игнорируем.
#![allow(static_mut_refs)]

// Pre-alpha: множество модулей написаны «на будущее»
// (console/shell — вернутся как GUI Terminal в v0.18.5,
//  ping/icmp — до production network в v0.17,
//  часть NVMe/PCI утилит — до multi-device).
// Запрещать dead_code сейчас дороже, чем польза от сигнала.
// TODO перед v1.0: убрать этот allow и почистить реально мёртвый код.
#![allow(dead_code)]

// mod lib есть в src/lib.rs — это внутренний модуль ядра
// (allocator, loader, panic), не Rust-library crate.
// Имя "lib" особое, но мы используем его намеренно.
#![allow(special_module_name)]

extern crate alloc;

use core::arch::{asm, global_asm};

mod lib;

mod arch;
use crate::arch::aarch64::{gic, timer, vectors, sched};

mod drivers;

mod gfx;

mod fs;

mod ipc;

mod network;

mod version;

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

pub static mut SERVICE_ELF_BUF: [u8; 131072] = [0; 131072];
pub static mut SERVICE_ELF_SIZE: usize = 0;

#[no_mangle]
pub extern "C" fn kmain() -> ! {
    unsafe {
        let cpacr: u64 = 3 << 20;
        asm!("msr CPACR_EL1, {}", in(reg) cpacr);
        asm!("isb");
    }

    println!("{} booting...", crate::version::OS_VERSION);
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

    let boot_as = unsafe { crate::arch::aarch64::mmu::create_as(0, 0) };
    unsafe { crate::arch::aarch64::mmu::activate(&boot_as); }
    unsafe { crate::arch::aarch64::mmu::enable(); }
    println!("MMU enabled.");
    crate::network::socket::init();
    crate::network::tcp::init();
    unsafe { crate::network::tcp::listen(8080); }

    // --- PCIe + NVMe ---
    let nvme_bar = unsafe { drivers::pci::init() };
    let mut nvme = match nvme_bar {
        Some(bar) => {
            println!("NVMe BAR0 = {:#x}", bar);
            match unsafe { drivers::nvme::init(bar) } {
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
    if let Some(id) = unsafe { drivers::nvme::identify_controller(&mut nvme, id_ctrl_pa as u64) } {
        let model = core::str::from_utf8(&id.model).unwrap_or("?").trim_end();
        println!("NVMe model: '{}'", model);
    }

    // Identify Namespace 1
    let id_ns_pa = unsafe { crate::arch::aarch64::phys::alloc_page().expect("no page") };
    unsafe { core::ptr::write_bytes(id_ns_pa as *mut u8, 0, 4096); }
    if let Some(ns) = unsafe { drivers::nvme::identify_namespace(&mut nvme, id_ns_pa as u64, 1) } {
        println!("NVMe NS1: nsze={} lbads={}", ns.nsze, ns.lbads);
    }

    if !unsafe { drivers::nvme::create_io_queues(&mut nvme) } {
        panic!("I/O queues failed");
    }

    // Mount FAT32 (глобально)
    match unsafe { fs::fat32::mount_global(&nvme) } {
        Ok(()) => {
            let fs = fs::fat32::get().unwrap();
            println!("FAT32 mounted: label='{}'", fs.label_str());
        }
        Err(e) => panic!("FAT32 mount failed: {:?}", e),
    }

    // --- virtio-net ---
    println!("=== Network ===");
    let vn_init = unsafe { drivers::virtio::net::init() };
    if let Some(vn) = vn_init {
        println!("virtio-net initialized");
        unsafe { drivers::virtio::net::setup_rx(&vn); }
        unsafe { drivers::virtio::net::set_global(vn); }
    } else {
        println!("virtio-net NOT FOUND");
    }

    // Загрузить user.elf с диска
    let mut user_elf: &[u8] = USER_ELF;
    {
        let fs = fs::fat32::get().unwrap();
        if let Some((cluster, size, is_dir)) = unsafe { fs.find_entry_by_path("/system/bin/init") } {
            if !is_dir && size as usize <= 131072 {
                println!("Loading /system/bin/init: {} bytes", size);
                let buf_pa = core::ptr::addr_of_mut!(SERVICE_ELF_BUF) as u64;
                match unsafe { fs.read_file(cluster, size, buf_pa) } {
                    Some(n) => {
                        user_elf = unsafe { core::slice::from_raw_parts(buf_pa as *const u8, n) };
                        unsafe { SERVICE_ELF_SIZE = n; }
                        println!("Loaded {} bytes", n);
                    }
                    None => println!("read_file failed, using include_bytes"),
                }
            } else {
                println!("/system/bin/init missing or too big, using include_bytes");
            }
        } else {
            println!("/system/bin/init not found, using include_bytes");
        }
    }

    sched::init(boot_as);
    unsafe {
        sched::spawn_el0("init", 2, 0x40400000, 0x40700000, user_elf);
    }

    timer::start();

    println!("  VBAR_EL1  = {:#x}", read_vbar());
    println!("Enabling interrupts...");
    unsafe { asm!("msr daifclr, #2") };

    println!("Kernel alive. Main becomes idle.");

    // --- virtio-gpu ---
    println!("=== Graphics ===");
    let mut gpu_opt = unsafe { drivers::virtio::gpu::init() };
    if let Some(mut gpu) = gpu_opt.take() {
        println!("virtio-gpu initialized");

        let fb_addr: u64 = 0x48000000;
        let width = 640u32;
        let height = 480u32;

        unsafe {
            core::ptr::write_bytes(fb_addr as *mut u8, 0, (width * height * 4) as usize);
            if !drivers::virtio::gpu::setup_framebuffer(&mut gpu, fb_addr, width, height) {
                println!("Framebuffer setup FAILED, but continuing...");
            }
            println!("Framebuffer setup OK");
            drivers::virtio::gpu::set_global(gpu);

            // Инициализация рабочего стола и создание окон
            gfx::window::init_desktop();
            
            let id1 = gfx::window::create_window(100, 80, 320, 240, "Terminal");
            
            let win = &mut gfx::window::wm().windows[id1];
            win.fill_rect(2, 22, 316, 216, gfx::window::COLOR_WIN_BG);
            win.draw_text(10, 30, "Kuberve W.5", 0x000095DF); 
            win.draw_text(10, 50, "Compositor is working.", 0x00FFFFFF); 
            win.draw_text(10, 70, "We can draw text", 0x00AAAAAA); 
            win.draw_text(10, 90, "inside windows now!", 0x00AAAAAA);
            
            win.cursor_x = 10;
            win.cursor_y = 120; 
            
            win.blit_to_back();

            gfx::window::create_window(200, 150, 320, 240, "Window 2");
            gfx::window::focus(1);

            gfx::compositor::present_full();
            gfx::cursor::show();
            gfx::compositor::present_damage();
        }
        println!("Graphics ready");
    } else {
        println!("virtio-gpu NOT FOUND");
    }

    // --- virtio-keyboard ---
    println!("=== Keyboard ===");
    let mut kbd_opt = unsafe { drivers::virtio::input::init() };
    if kbd_opt.is_some() {
        println!("virtio-keyboard initialized");
    } else {
        println!("virtio-keyboard NOT FOUND");
    }

    // --- virtio-tablet ---
    println!("=== Mouse ===");
    let mut tablet_opt = unsafe { drivers::virtio::tablet::init() };
    if tablet_opt.is_some() {
        println!("virtio-tablet initialized");
    } else {
        println!("virtio-tablet NOT FOUND");
    }

    // --- DHCP ---
    if let Some(vn) = drivers::virtio::net::get_global_mut() {
        unsafe { crate::network::dhcp::run(vn); }
    }

    // Helper: send ARP request for target IP
    unsafe fn send_arp(vn: &mut drivers::virtio::net::VirtioNet, target_ip: [u8; 4]) {
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
        drivers::virtio::net::send_packet(vn, &arp);
    }

    if let Some(vn) = drivers::virtio::net::get_global_mut() {
        unsafe { send_arp(vn, [10, 0, 2, 2]); }   // gateway
        unsafe { send_arp(vn, [10, 0, 2, 3]); }   // DNS
    }

    // --- Poll loop ---
    println!("Entering main loop...");
    let mut net_counter = 0u32;
    loop {
        // 1. Keyboard — обработать все события, БЕЗ per-event flush
        if let Some(kbd) = kbd_opt.as_mut() {
            while let Some(ev) = unsafe { drivers::virtio::input::poll_events(kbd) } {
                if ev.typ != 1 { continue; }
                if drivers::virtio::input::is_shift_code(ev.code) {
                    if ev.value == 1 { gfx::shell::shift_down(); }
                    else if ev.value == 0 { gfx::shell::shift_up(); }
                    continue;
                }
                if ev.value == 1 {
                    if let Some(win_id) = unsafe { gfx::window::get_focused() } {
                        let win = &mut gfx::window::wm().windows[win_id];
                        unsafe { gfx::cursor::hide(); }
                        if ev.code == 14 {
                            win.put_char(8);
                        } else if let Some(c) = drivers::virtio::input::keycode_to_ascii(ev.code, gfx::shell::shift()) {
                            win.put_char(c);
                        }
                        unsafe { gfx::cursor::show(); }
                    }
                }
            }
        }


        // 2. Mouse — coalesce, только последняя позиция
        if let Some(tablet) = tablet_opt.as_mut() {
            let mut mx = unsafe { gfx::cursor::MOUSE_X };
            let mut my = unsafe { gfx::cursor::MOUSE_Y };
            let mut moved = false;
            let mut btn_down = false;
            let mut btn_up = false;

            while let Some(ev) = unsafe { drivers::virtio::tablet::poll_events(tablet) } {
                if ev.typ == 3 {
                    if ev.code == 0 && tablet.abs_max_x > 0 {
                        mx = ((ev.value as u64 * 640) / tablet.abs_max_x as u64) as u32;
                        moved = true;
                    } else if ev.code == 1 && tablet.abs_max_y > 0 {
                        my = ((ev.value as u64 * 480) / tablet.abs_max_y as u64) as u32;
                        moved = true;
                    }
                } else if ev.typ == 1 && ev.code == 0x110 {
                    // BTN_LEFT
                    if ev.value == 1 { btn_down = true; }
                    else if ev.value == 0 { btn_up = true; }
                }
            }

            // W.4: Drag and Drop
            if btn_down {
                if let Some(win_id) = unsafe { gfx::window::find_at(mx, my) } {
                    unsafe { gfx::window::focus(win_id); }
                    unsafe { gfx::window::start_drag(win_id, mx, my); }
                }
            }
            
            if btn_up {
                unsafe { gfx::window::end_drag(); }
            }

            if moved {
                let new_x = core::cmp::min(mx, 639);
                let new_y = core::cmp::min(my, 479);
                unsafe { gfx::cursor::move_to(new_x, new_y); }
                
                // Если тащим — двигаем окно
                if unsafe { gfx::window::is_dragging() } {
                    unsafe { gfx::window::do_drag(new_x, new_y); }
                }
            }
        }

        // 3. Сеть — реже
        net_counter = net_counter.wrapping_add(1);
        if net_counter % 2 == 0 {
            if let Some(vn) = unsafe { drivers::virtio::net::get_global_mut() } {
                let _ = unsafe { drivers::virtio::net::poll_rx(vn) };
            }
        }

        // 4. wfe — проснуться на IRQ (таймер 100 Гц → 10 мс)
        unsafe { gfx::compositor::present_damage(); }
        unsafe { asm!("wfe"); }
    }
}