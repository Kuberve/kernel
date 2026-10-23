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