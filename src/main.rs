#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

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
            // 1. Нулим framebuffer
            core::ptr::write_bytes(fb_addr as *mut u8, 0, (width * height * 4) as usize);

            // 2. Создать resource + attach + scanout (КРИТИЧНО!)
            if !drivers::virtio::gpu::setup_framebuffer(&mut gpu, fb_addr, width, height) {
                panic!("Framebuffer setup FAILED");
            }
            println!("Framebuffer setup OK");

            // 3. Инициализировать консоль (заполнит фон)
            gfx::console::init();

            // 4. Нарисовать заголовок
            gfx::text::draw_string_wrap(10, 10, crate::version::CONSOLE_TITLE, 0x0000FF00, Some(0x00101010));
            gfx::text::draw_string_wrap(10, 30, "", 0x00A0A0A0, Some(0x00101010));
            gfx::console::console().cursor_y = 30;
            gfx::console::console().cursor_x = 10;

            gfx::shell::print_prompt();

            // 5. Отправить на хост (TRANSFER + FLUSH)
            if !drivers::virtio::gpu::flush(&mut gpu, width, height) {
                println!("Initial flush FAILED");
            }

            // 6. Сохранить gpu глобально для клавиатуры
            drivers::virtio::gpu::set_global(gpu);
        }
        println!("Graphics ready");

        // --- Windows demo ---
        unsafe {
            use gfx::wm;
            wm::init();
            // НЕпересекающиеся окна (x: 50-270 и 350-590)
            wm::add_window(50,  200, 220, 140, "About", &[
                "Kuberve v0.14.3",
                "aarch64 OS with GUI",
                "click title to drag",
                "click X to close",
            ]);
            wm::add_window(350, 200, 240, 160, "System", &[
                "IPC: Message (64B)",
                "net: ARP/TCP/DHCP",
                "fs:  NVMe + FAT32",
                "gfx: virtio-gpu 2D",
            ]);
            println!("Windows opened");

            // Явный flush, чтобы окна появились СРАЗУ
            if let Some(gpu) = drivers::virtio::gpu::get_global_mut() {
                drivers::virtio::gpu::flush(gpu, 640, 480);
            }
        }
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
    if let Some(vn) = unsafe { drivers::virtio::net::get_global_mut() } {
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

    if let Some(vn) = unsafe { drivers::virtio::net::get_global_mut() } {
        unsafe { send_arp(vn, [10, 0, 2, 2]); }   // gateway
        unsafe { send_arp(vn, [10, 0, 2, 3]); }   // DNS
    }

    // --- Poll loop ---
    println!("Entering main loop...");
    let mut net_counter = 0u32;
    loop {
        let mut need_flush = false;

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
                    if ev.code == 14 {
                        unsafe { gfx::shell::backspace(); }
                        need_flush = true;
                    } else if let Some(c) = drivers::virtio::input::keycode_to_ascii(ev.code, gfx::shell::shift()) {
                        unsafe { gfx::shell::input(c); }
                        need_flush = true;
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

            // 1. Обновить позицию курсора (визуально)
            // Позиция и флаг drag
            let drag_active = unsafe { gfx::wm::wm().drag_idx.is_some() };

            if moved && drag_active {
                let (fx, fy, fw, fh) = unsafe { gfx::wm::drag_to(mx, my) };
                if fw > 0 && fh > 0 {
                    if let Some(gpu) = unsafe { drivers::virtio::gpu::get_global_mut() } {
                        unsafe { drivers::virtio::gpu::flush_region(gpu, fx, fy, fw, fh); }
                    }
                }
            } else if moved {
                // Обычное движение курсора
                let old_x = unsafe { gfx::cursor::MOUSE_X };
                let old_y = unsafe { gfx::cursor::MOUSE_Y };
                let new_x = core::cmp::min(mx, 639);
                let new_y = core::cmp::min(my, 479);
                unsafe { gfx::cursor::move_to(new_x, new_y); }
                if let Some(gpu) = unsafe { drivers::virtio::gpu::get_global_mut() } {
                    let min_x = old_x.min(new_x).saturating_sub(3);
                    let min_y = old_y.min(new_y).saturating_sub(3);
                    let max_x = old_x.max(new_x) + 4;
                    let max_y = old_y.max(new_y) + 4;
                    let w = max_x.saturating_sub(min_x);
                    let h = max_y.saturating_sub(min_y);
                    unsafe { drivers::virtio::gpu::flush_region(gpu, min_x, min_y, w, h); }
                }
            }

            // 2. Кнопка вниз
            if btn_down {
                unsafe {
                    let (cx, cy) = (gfx::cursor::MOUSE_X, gfx::cursor::MOUSE_Y);
                    if let Some(idx) = gfx::wm::find_at(cx, cy) {
                        if gfx::wm::hit_close(idx, cx, cy) {
                            gfx::wm::close_window(idx);
                            if let Some(gpu) = drivers::virtio::gpu::get_global_mut() {
                                drivers::virtio::gpu::flush(gpu, 640, 480);
                            }
                        } else if gfx::wm::hit_title(idx, cx, cy) {
                            gfx::wm::focus(idx);
                            gfx::wm::start_drag(idx, cx, cy);
                            // focus redraw делает 2 окна — flush весь экран
                            if let Some(gpu) = drivers::virtio::gpu::get_global_mut() {
                                drivers::virtio::gpu::flush(gpu, 640, 480);
                            }
                        } else {
                            gfx::wm::focus(idx);
                            if let Some(gpu) = drivers::virtio::gpu::get_global_mut() {
                                drivers::virtio::gpu::flush(gpu, 640, 480);
                            }
                        }
                    }
                }
            }

            // 3. Кнопка вверх
            if btn_up {
                unsafe { gfx::wm::end_drag(); }
            }
        }

        // 3. Flush текста
        if need_flush {
            unsafe { gfx::cursor::hide(); }   // восстановить пиксели под курсором
            if let Some(gpu) = unsafe { drivers::virtio::gpu::get_global_mut() } {
                unsafe { gfx::console::flush(gpu); }
            }
            unsafe { gfx::cursor::show(); }   // снова нарисовать поверх текста
            if let Some(gpu) = unsafe { drivers::virtio::gpu::get_global_mut() } {
                let (cx, cy, cw, ch) = gfx::cursor::dirty_rect();
                unsafe { drivers::virtio::gpu::flush_region(gpu, cx, cy, cw, ch); }
            }
        }

        // 4. Сеть — реже
        net_counter = net_counter.wrapping_add(1);
        if net_counter % 20 == 0 {
            if let Some(vn) = unsafe { drivers::virtio::net::get_global_mut() } {
                let _ = unsafe { drivers::virtio::net::poll_rx(vn) };
            }
        }

        // 5. wfe — проснуться на IRQ (таймер 100 Гц → 10 мс)
        unsafe { core::arch::asm!("wfe") };
    }
}