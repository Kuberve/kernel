use crate::gfx::console;

pub const LINE_MAX: usize = 256;

pub struct ShellState {
    pub line: [u8; LINE_MAX],
    pub len: usize,
}

pub static mut SHELL: ShellState = ShellState {
    line: [0; LINE_MAX],
    len: 0,
};

static mut SHIFT: bool = false;

pub fn shell() -> &'static mut ShellState {
    unsafe { &mut SHELL }
}

pub fn shift() -> bool { unsafe { SHIFT } }
pub fn shift_down() { unsafe { SHIFT = true; } }
pub fn shift_up() { unsafe { SHIFT = false; } }

unsafe fn puts(s: &[u8]) {
    for &b in s { console::putchar(b); }
}

unsafe fn put_num(mut n: u64) {
    if n == 0 { console::putchar(b'0'); return; }
    let mut buf = [0u8; 20];
    let mut i = 0;
    while n > 0 { buf[i] = b'0' + (n % 10) as u8; n /= 10; i += 1; }
    while i > 0 { i -= 1; console::putchar(buf[i]); }
}

pub unsafe fn print_prompt() {
    let s = shell();
    puts(b"kuberve> ");
    s.len = 0;
}

pub unsafe fn backspace() {
    let s = shell();
    if s.len > 0 {
        s.len -= 1;
        console::backspace();
    }
}

pub unsafe fn input(c: u8) {
    let s = shell();
    if c == b'\n' {
        console::putchar(b'\n');
        execute();
        print_prompt();
        return;
    }
    if c == b'\r' { return; }
    if s.len >= LINE_MAX - 1 { return; }
    s.line[s.len] = c;
    s.len += 1;
    console::putchar(c);
}

unsafe fn execute() {
    let s = shell();
    let cmd = &s.line[..s.len];

    // Trim
    let mut start = 0;
    let mut end = cmd.len();
    while start < end && (cmd[start] == b' ' || cmd[start] == b'\t') { start += 1; }
    while end > start && (cmd[end-1] == b' ' || cmd[end-1] == b'\t') { end -= 1; }
    let trimmed = &cmd[start..end];

    if trimmed.is_empty() { return; }

    let mut space = 0;
    while space < trimmed.len() && trimmed[space] != b' ' { space += 1; }
    let name = &trimmed[..space];
    let args = if space < trimmed.len() { &trimmed[space+1..] } else { &[] };

    match name {
        b"larpfetch" => cmd_larpfetch(),
        b"fetch"     => cmd_larpfetch(),
        b"help"   => cmd_help(),
        b"clear"  => cmd_clear(),
        b"echo"   => cmd_echo(args),
        b"ls"     => cmd_ls(),
        b"cat"    => cmd_cat(args),
        b"uptime" => cmd_uptime(),
        b"info"   => cmd_info(),
        b"reboot" => cmd_reboot(),
        _ => {
            puts(b"unknown command: ");
            puts(name);
            puts(b"\n");
            puts(b"type 'help' for commands\n");
        }
    }
}

unsafe fn cmd_help() {
    puts(b"Commands:\n");
    puts(b"  larpfetch - system info with logo\n");
    puts(b"  help    - this message\n");
    puts(b"  clear   - clear screen\n");
    puts(b"  echo X  - print X\n");
    puts(b"  ls      - list root of FAT32\n");
    puts(b"  cat F   - print file F\n");
    puts(b"  uptime  - ticks since boot\n");
    puts(b"  info    - kernel info\n");
    puts(b"  reboot  - reboot\n");
}

unsafe fn cmd_clear() {
    console::init();
    puts(b"Kuberve Console v0.10.6\n");
    puts(b"\n");
}

unsafe fn cmd_echo(args: &[u8]) {
    puts(args);
    console::putchar(b'\n');
}

unsafe fn cmd_ls() {
    if let Some(fs) = crate::fs::fat32::get() {
        puts(b"(listing to UART, see terminal)\n");
        fs.readdir(fs.root_cluster);
    } else {
        puts(b"no FAT32 mounted\n");
    }
}

unsafe fn cmd_cat(args: &[u8]) {
    if args.is_empty() {
        puts(b"usage: cat <file>\n");
        return;
    }
    let mut end = args.len();
    while end > 0 && args[end-1] == b' ' { end -= 1; }
    let path = &args[..end];

    if let Some(fs) = crate::fs::fat32::get() {
        let path_str = match core::str::from_utf8(path) {
            Ok(s) => s,
            Err(_) => { puts(b"invalid path\n"); return; }
        };
        if let Some((cluster, size, is_dir)) = fs.find_entry(fs.root_cluster, path_str) {
            if is_dir { puts(b"is a directory\n"); return; }
            let buf_pa = match crate::arch::aarch64::phys::alloc_page() {
                Some(p) => p,
                None => { puts(b"alloc failed\n"); return; }
            };
            let read_size = core::cmp::min(size, 4096u32);
            match fs.read_file(cluster, read_size, buf_pa as u64) {
                Some(n) => {
                    let bytes = core::slice::from_raw_parts(buf_pa as *const u8, n);
                    for &b in bytes {
                        console::putchar(b);
                    }
                    if size > 4096 { puts(b"\n[truncated to 4096]\n"); }
                    else if n > 0 && bytes[n-1] != b'\n' { console::putchar(b'\n'); }
                }
                None => puts(b"read failed\n"),
            }
        } else {
            puts(b"file not found\n");
        }
    } else {
        puts(b"no FAT32 mounted\n");
    }
}

unsafe fn cmd_uptime() {
    let ticks = crate::arch::aarch64::timer::tick_count();
    put_num(ticks);
    puts(b" ticks, ~");
    put_num(ticks / 100);
    puts(b" sec\n");
}

unsafe fn cmd_info() {
    puts(b"Kuberve v0.10.6\n");
    puts(b"  arch:  aarch64 (ARMv8-A)\n");
    puts(b"  sched: round-robin + priority + aging\n");
    puts(b"  mmu:   4-level page tables\n");
    puts(b"  ipc:   Message (64B), Queue (8)\n");
    puts(b"  net:   ARP, IPv4, ICMP, UDP, TCP, DHCP\n");
    puts(b"  fs:    NVMe + FAT32\n");
    puts(b"  gfx:   virtio-gpu, 8x16 font, keyboard, mouse\n");
    puts(b"  shell: this\n");
}

unsafe fn cmd_reboot() {
    puts(b"reboot not implemented (yet)\n");
}

// ============================================================
// larpfetch (neofetch-style)
// ============================================================

unsafe fn fmt_u64(mut n: u64, buf: &mut [u8]) -> usize {
    if n == 0 { buf[0] = b'0'; return 1; }
    let mut tmp = [0u8; 20];
    let mut i = 0;
    while n > 0 { tmp[i] = b'0' + (n % 10) as u8; n /= 10; i += 1; }
    for j in 0..i { buf[j] = tmp[i - 1 - j]; }
    i
}

unsafe fn fmt_ip(ip: [u8; 4], buf: &mut [u8]) -> usize {
    let mut pos = 0usize;
    for k in 0..4 {
        let mut n = ip[k] as u64;
        if n == 0 {
            buf[pos] = b'0'; pos += 1;
        } else {
            let mut tmp = [0u8; 3];
            let mut i = 0;
            while n > 0 { tmp[i] = b'0' + (n % 10) as u8; n /= 10; i += 1; }
            while i > 0 { i -= 1; buf[pos] = tmp[i]; pos += 1; }
        }
        if k < 3 { buf[pos] = b'.'; pos += 1; }
    }
    pos
}

unsafe fn draw_kv(x: u32, y: u32, key: &str, val: &str, label_c: u32, val_c: u32) {
    use crate::gfx::text;
    text::draw_string(x, y, key, label_c, None);
    text::draw_string(x + 80, y, val, val_c, None);
}

unsafe fn cmd_larpfetch() {
    use crate::gfx::{console, text};
    use crate::arch::aarch64::{phys, timer};
    use crate::network::dhcp;

    console::init();

    let cyan    = 0x00FFFF00u32;
    let green   = 0x0000FF00u32;
    let white   = 0x00FFFFFFu32;
    let yellow  = 0x0000FFFFu32;
    let dim     = 0x00808080u32;

    // === Logo: isometric cube with nerve pulse inside ===
    // 15 chars wide, 7 lines tall
    let art: [&str; 7] = [
        "   +-----------+",
        "  /           /|",
        " +-----------+ |",
        " |  _/\\_/\\_  | |",
        " |           | +",
        " |           |/",
        " +-----------+",
    ];
    for (i, line) in art.iter().enumerate() {
        text::draw_string(40, 70 + i as u32 * 16, line, cyan, None);
    }

    // "KUBERVE" wordmark под кубом
    text::draw_string(40, 70 + 7 * 16 + 8, "  K U B E R V E", yellow, None);

    // === Info panel ===
    let mut y = 40u32;
    text::draw_string(400, y, "user@kuberve", yellow, None);
    y += 18;
    text::draw_string(400, y, "-------------", yellow, None);
    y += 20;

    draw_kv(400, y, "OS:      ", "Kuberve v0.10.6", green, white);
    y += 18;
    draw_kv(400, y, "Kernel:  ", "kuberve-0.1 aarch64", green, white);
    y += 18;

    // Uptime
    let ticks = timer::tick_count();
    let secs = ticks / 100;
    let mut up_buf = [0u8; 24];
    let n = fmt_u64(secs, &mut up_buf);
    up_buf[n] = b's';
    let up_s = core::str::from_utf8(&up_buf[..n+1]).unwrap_or("?");
    draw_kv(400, y, "Uptime:  ", up_s, green, white);
    y += 18;

    draw_kv(400, y, "Shell:   ", "kvsh", green, white);
    y += 18;
    draw_kv(400, y, "Res:     ", "640x480", green, white);
    y += 18;

    // Memory
    let total = phys::total_pages();
    let used = phys::used_pages();
    let used_mb = (used as u64) * 4 / 1024;
    let total_mb = (total as u64) * 4 / 1024;
    let mut m1 = [0u8; 16]; let n1 = fmt_u64(used_mb, &mut m1);
    let mut m2 = [0u8; 16]; let n2 = fmt_u64(total_mb, &mut m2);
    let mut mem_buf = [0u8; 40];
    let mut p = 0;
    for &b in &m1[..n1] { mem_buf[p] = b; p += 1; }
    for &b in b"MB / " { mem_buf[p] = b; p += 1; }
    for &b in &m2[..n2] { mem_buf[p] = b; p += 1; }
    for &b in b"MB" { mem_buf[p] = b; p += 1; }
    let mem_s = core::str::from_utf8(&mem_buf[..p]).unwrap_or("?");
    draw_kv(400, y, "Memory:  ", mem_s, green, white);
    y += 18;

    // IP
    let ip = dhcp::our_ip();
    let mut ip_buf = [0u8; 16];
    let n = fmt_ip(ip, &mut ip_buf);
    let ip_s = core::str::from_utf8(&ip_buf[..n]).unwrap_or("?");
    draw_kv(400, y, "IP:      ", ip_s, green, white);
    y += 18;

    // Disk
    if let Some(fs) = crate::fs::fat32::get() {
        let total_bytes = (fs.total_sectors as u64) * (fs.bytes_per_sector as u64);
        let total_mb = total_bytes / (1024 * 1024);
        let mut d1 = [0u8; 16]; let n1 = fmt_u64(total_mb, &mut d1);
        let mut disk_buf = [0u8; 32];
        let mut p = 0;
        for &b in &d1[..n1] { disk_buf[p] = b; p += 1; }
        for &b in b" MB FAT32" { disk_buf[p] = b; p += 1; }
        let disk_s = core::str::from_utf8(&disk_buf[..p]).unwrap_or("?");
        draw_kv(400, y, "Disk:    ", disk_s, green, white);
    }
    y += 24;

    // Palette: 8 colored blocks (16x16 px each)
    let palette = [
        0x00000000u32,  // black
        0x000000FFu32,  // red
        0x0000FF00u32,  // green
        0x0000FFFFu32,  // yellow
        0x00FF0000u32,  // blue
        0x00FF00FFu32,  // magenta
        0x00FFFF00u32,  // cyan
        0x00FFFFFFu32,  // white
    ];
    let mut px = 400u32;
    for &color in palette.iter() {
        text::draw_string(px, y, "  ", color, Some(color));
        px += 18;   // 16px block + 2px gap
    }

    // Cursor внизу
    console::console().cursor_y = 460;
    console::console().cursor_x = 10;

    if let Some(gpu) = crate::drivers::virtio::gpu::get_global_mut() {
        crate::drivers::virtio::gpu::flush(gpu, 640, 480);
    }
}