use core::arch::{asm, global_asm};
use crate::println;

global_asm!(
    ".section .text.vectors",
    ".align 11",
    ".global _vectors",
    "_vectors:",

    ".balign 0x80", "b sync_handler",
    ".balign 0x80", "b irq_handler",
    ".balign 0x80", "b fiq_handler",
    ".balign 0x80", "b serr_handler",

    ".balign 0x80", "b sync_handler",
    ".balign 0x80", "b irq_handler",
    ".balign 0x80", "b fiq_handler",
    ".balign 0x80", "b serr_handler",

    ".balign 0x80", "b sync_handler",
    ".balign 0x80", "b irq_handler",
    ".balign 0x80", "b fiq_handler",
    ".balign 0x80", "b serr_handler",

    ".balign 0x80", "b sync_handler",
    ".balign 0x80", "b irq_handler",
    ".balign 0x80", "b fiq_handler",
    ".balign 0x80", "b serr_handler",

    "sync_handler:",
    "  msr daifset, #2",

    // SIMD (q0..q31) — первыми, самое высокое место на стеке
    //  "  stp q0, q1, [sp, #-32]!",
    // "  stp q2, q3, [sp, #-32]!",
    // "  stp q4, q5, [sp, #-32]!",
    // "  stp q6, q7, [sp, #-32]!",
    // "  stp q8, q9, [sp, #-32]!",
    // "  stp q10, q11, [sp, #-32]!",
    // "  stp q12, q13, [sp, #-32]!",
    // "  stp q14, q15, [sp, #-32]!",
    // "  stp q16, q17, [sp, #-32]!",
    // "  stp q18, q19, [sp, #-32]!",
    // "  stp q20, q21, [sp, #-32]!",
    // "  stp q22, q23, [sp, #-32]!",
    // "  stp q24, q25, [sp, #-32]!",
    // "  stp q26, q27, [sp, #-32]!",
    // "  stp q28, q29, [sp, #-32]!",
    // "  stp q30, q31, [sp, #-32]!",

    // GPR (x0..x30) — сохраняются ПЕРЕД использованием x2/x3
    "  stp x0, x1, [sp, #-16]!",
    "  stp x2, x3, [sp, #-16]!",     // <- x2=len, x3=? сохраняются как есть
    "  stp x4, x5, [sp, #-16]!",
    "  stp x6, x7, [sp, #-16]!",
    "  stp x8, x9, [sp, #-16]!",
    "  stp x10, x11, [sp, #-16]!",
    "  stp x12, x13, [sp, #-16]!",
    "  stp x14, x15, [sp, #-16]!",
    "  stp x16, x17, [sp, #-16]!",
    "  stp x18, x19, [sp, #-16]!",
    "  stp x20, x21, [sp, #-16]!",
    "  stp x22, x23, [sp, #-16]!",
    "  stp x24, x25, [sp, #-16]!",
    "  stp x26, x27, [sp, #-16]!",
    "  stp x28, x29, [sp, #-16]!",
    "  stp x30, xzr, [sp, #-16]!",

    // ELR/SPSR
    "  mrs x0, ELR_EL1",
    "  mrs x1, SPSR_EL1",
    "  stp x0, x1, [sp, #-16]!",

    "  mov x0, sp",
    "  bl rust_sync_handler",
    "  mov sp, x0",

    // Restore в обратном порядке
    "  ldp x0, x1, [sp], #16",
    "  msr ELR_EL1, x0",
    "  msr SPSR_EL1, x1",

    "  ldp x30, xzr, [sp], #16",
    "  ldp x28, x29, [sp], #16",
    "  ldp x26, x27, [sp], #16",
    "  ldp x24, x25, [sp], #16",
    "  ldp x22, x23, [sp], #16",
    "  ldp x20, x21, [sp], #16",
    "  ldp x18, x19, [sp], #16",
    "  ldp x16, x17, [sp], #16",
    "  ldp x14, x15, [sp], #16",
    "  ldp x12, x13, [sp], #16",
    "  ldp x10, x11, [sp], #16",
    "  ldp x8, x9, [sp], #16",
    "  ldp x6, x7, [sp], #16",
    "  ldp x4, x5, [sp], #16",
    "  ldp x2, x3, [sp], #16",
    "  ldp x0, x1, [sp], #16",

    // "  ldp q30, q31, [sp], #32",
    //"  ldp q28, q29, [sp], #32",
    // "  ldp q26, q27, [sp], #32",
    // "  ldp q24, q25, [sp], #32",
    // "  ldp q22, q23, [sp], #32",
    // "  ldp q20, q21, [sp], #32",
    // "  ldp q18, q19, [sp], #32",
    // "  ldp q16, q17, [sp], #32",
    // "  ldp q14, q15, [sp], #32",
    // "  ldp q12, q13, [sp], #32",
    // "  ldp q10, q11, [sp], #32",
    // "  ldp q8, q9, [sp], #32",
    // "  ldp q6, q7, [sp], #32",
    // "  ldp q4, q5, [sp], #32",
    // "  ldp q2, q3, [sp], #32",
    // "  ldp q0, q1, [sp], #32",

    "  msr daifclr, #2",
    "  eret",

    "fiq_handler:",
    "serr_handler:",
    "  b rust_exception_handler",

    "irq_handler:",
    "  msr daifset, #2",
    "  stp x0, x1, [sp, #-16]!",
    "  stp x2, x3, [sp, #-16]!",
    "  stp x4, x5, [sp, #-16]!",
    "  stp x6, x7, [sp, #-16]!",
    "  stp x8, x9, [sp, #-16]!",
    "  stp x10, x11, [sp, #-16]!",
    "  stp x12, x13, [sp, #-16]!",
    "  stp x14, x15, [sp, #-16]!",
    "  stp x16, x17, [sp, #-16]!",
    "  stp x18, x19, [sp, #-16]!",
    "  stp x20, x21, [sp, #-16]!",
    "  stp x22, x23, [sp, #-16]!",
    "  stp x24, x25, [sp, #-16]!",
    "  stp x26, x27, [sp, #-16]!",
    "  stp x28, x29, [sp, #-16]!",
    "  stp x30, xzr, [sp, #-16]!",

    "  mrs x0, ELR_EL1",
    "  mrs x1, SPSR_EL1",
    "  stp x0, x1, [sp, #-16]!",

    "  mov x0, sp",
    "  bl rust_irq_handler",
    "  mov sp, x0",

    "  ldp x0, x1, [sp], #16",
    "  msr ELR_EL1, x0",
    "  msr SPSR_EL1, x1",

    "  ldp x30, xzr, [sp], #16",
    "  ldp x28, x29, [sp], #16",
    "  ldp x26, x27, [sp], #16",
    "  ldp x24, x25, [sp], #16",
    "  ldp x22, x23, [sp], #16",
    "  ldp x20, x21, [sp], #16",
    "  ldp x18, x19, [sp], #16",
    "  ldp x16, x17, [sp], #16",
    "  ldp x14, x15, [sp], #16",
    "  ldp x12, x13, [sp], #16",
    "  ldp x10, x11, [sp], #16",
    "  ldp x8, x9, [sp], #16",
    "  ldp x6, x7, [sp], #16",
    "  ldp x4, x5, [sp], #16",
    "  ldp x2, x3, [sp], #16",
    "  ldp x0, x1, [sp], #16",

    "  msr daifclr, #2",
    "  eret",
);

extern "C" {
    static _vectors: u8;
}

pub fn init() {
    unsafe {
        let v = &_vectors as *const _ as u64;
        asm!("msr VBAR_EL1, {}", in(reg) v);
        asm!("isb");
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_sync_handler(old_sp: u64) -> u64 {
    use crate::arch::aarch64::sched::SavedCtx;
    use crate::arch::aarch64::{sched, syscall};

    let esr: u64;
    asm!("mrs {}, ESR_EL1", out(reg) esr);
    let ec = (esr >> 26) & 0x3F;

    if ec != 0x15 {
        let far: u64; asm!("mrs {}, FAR_EL1", out(reg) far);
        let elr: u64; asm!("mrs {}, ELR_EL1", out(reg) elr);
        let spsr: u64; asm!("mrs {}, SPSR_EL1", out(reg) spsr);
        let pid = crate::arch::aarch64::sched::current_pid();
        crate::println!("!! sync exception EC={:#x} ESR={:#x}", ec, esr);
        crate::println!("   FAR={:#x}", far);
        crate::println!("   ELR={:#x}", elr);
        crate::println!("   SPSR={:#x} mode={}", spsr, spsr & 0xF);
        crate::println!("   pid={}", pid);
        loop { asm!("wfe"); }
    }

    let ctx = &mut *(old_sp as *mut SavedCtx);
    let num = ctx.x8;
    let a0  = ctx.x0;
    let a1  = ctx.x1;
    let a2  = ctx.x2;

    match num {
        syscall::SYS_YIELD => sched::on_yield(old_sp),
        syscall::SYS_WRITE => {
            let n = syscall::write(a0, a1 as *const u8, a2);
            ctx.x0 = n as u64;
            old_sp
        }
        syscall::SYS_SLEEP => {
            sched::sleep(a0);
            old_sp
        }
        syscall::SYS_GETPID => {
            ctx.x0 = sched::current_pid();
            old_sp
        }
        syscall::SYS_SEND => {
            let ptr = a1 as *const crate::arch::aarch64::ipc::Message;
            let mut msg = *ptr;

            // Если WRITE (tag=1): скопировать байты из user ptr в msg.data
            // data[0] = ptr, data[1] = len → после: data[0] = len, data[1..7] = bytes
            if msg.tag == 1 {
                let src = msg.data[0] as *const u8;
                let len = msg.data[1] as usize;
                if len > 48 {
                    ctx.x0 = u64::MAX;
                    return old_sp;
                }
                let mut new_data = [0u64; 7];
                new_data[0] = len as u64;
                let dst = &mut new_data[1] as *mut u64 as *mut u8;
                for i in 0..len {
                    *dst.add(i) = *src.add(i);
                }
                msg.data = new_data;
            }

            let ok = sched::ipc_send_blocking(a0 as usize, msg);
            ctx.x0 = if ok { 0 } else { u64::MAX };
            old_sp
        }
        syscall::SYS_RECV => {
            let msg = sched::ipc_recv_block();
            let ptr = a0 as *mut crate::arch::aarch64::ipc::Message;
            *ptr = msg;
            ctx.x0 = 0;
            old_sp
        }
        syscall::SYS_IRQ_ACK => {
            unsafe { crate::arch::aarch64::gic::enable_irq(a0 as u32); }
            ctx.x0 = 0;
            old_sp
        }
        syscall::SYS_TRY_SEND => {
            let ptr = a1 as *const crate::arch::aarch64::ipc::Message;
            let mut msg = *ptr;

            if msg.tag == 1 {
                let src = msg.data[0] as *const u8;
                let len = msg.data[1] as usize;
                if len > 48 {
                    ctx.x0 = u64::MAX;
                    return old_sp;
                }
                let mut new_data = [0u64; 7];
                new_data[0] = len as u64;
                let dst = &mut new_data[1] as *mut u64 as *mut u8;
                for i in 0..len {
                    *dst.add(i) = *src.add(i);
                }
                msg.data = new_data;
            }

            let ok = sched::ipc_send(a0 as usize, msg);
            ctx.x0 = if ok { 0 } else { u64::MAX };
            old_sp
        }
        syscall::SYS_TRY_RECV => {
            let ptr = a0 as *mut crate::arch::aarch64::ipc::Message;
            match sched::ipc_recv_nonblock() {
                Some(m) => { *ptr = m; ctx.x0 = 0; }
                None => { ctx.x0 = u64::MAX; }
            }
            old_sp
        }
        syscall::SYS_SBRK => {
            let delta = a0 as i64;
            ctx.x0 = sbrk_impl(delta);
            old_sp
        }
        syscall::SYS_OPEN => {
            let path_ptr = a0 as *const u8;
            let path_len = a1 as usize;
            if path_len == 0 || path_len > 256 {
                ctx.x0 = u64::MAX;
                return old_sp;
            }
            let path_slice = core::slice::from_raw_parts(path_ptr, path_len);
            let path = match core::str::from_utf8(path_slice) {
                Ok(s) => s,
                Err(_) => { ctx.x0 = u64::MAX; return old_sp; }
            };
            ctx.x0 = unsafe { crate::fs::fd::fd_open(path, a2) } as u64;
            old_sp
        }
        syscall::SYS_READ => {
            ctx.x0 = unsafe { crate::fs::fd::fd_read(a0, a1, a2) } as u64;
            old_sp
        }
        syscall::SYS_CLOSE => {
            ctx.x0 = unsafe { crate::fs::fd::fd_close(a0) } as u64;
            old_sp
        }
        syscall::SYS_WRITE_FILE => {
            ctx.x0 = unsafe { crate::fs::fd::fd_write(a0, a1, a2) } as u64;
            old_sp
        }
        syscall::SYS_SOCKET => {
            if a0 != 2 {
                ctx.x0 = u64::MAX;
                return old_sp;
            }
            if a1 == 1 {
                // SOCK_STREAM (TCP)
                let fd = unsafe { crate::fs::fd::fd_alloc_tcp_socket() };
                ctx.x0 = if fd < 0 { u64::MAX } else { fd as u64 };
                old_sp
            } else if a1 == 2 {
                // SOCK_DGRAM (UDP) — existing
                let sid = match unsafe { crate::network::socket::create() } {
                    Some(s) => s,
                    None => { ctx.x0 = u64::MAX; return old_sp; }
                };
                let fd = unsafe { crate::fs::fd::fd_alloc_socket(sid) };
                if fd < 0 {
                    unsafe { crate::network::socket::close(sid); }
                    ctx.x0 = u64::MAX;
                    return old_sp;
                }
                ctx.x0 = fd as u64;
                old_sp
            } else {
                ctx.x0 = u64::MAX;
                old_sp
            }
        }
        syscall::SYS_BIND => {
            let sid = match unsafe { crate::fs::fd::fd_socket_id(a0) } {
                Some(s) => s,
                None => { ctx.x0 = u64::MAX; return old_sp; }
            };
            let ok = unsafe { crate::network::socket::bind(sid, a1 as u16) };
            ctx.x0 = if ok { 0 } else { u64::MAX };
            old_sp
        }
        syscall::SYS_SENDTO => {
            let a3 = ctx.x3;
            let a4 = ctx.x4;
            let sid = match unsafe { crate::fs::fd::fd_socket_id(a0) } {
                Some(s) => s,
                None => { ctx.x0 = u64::MAX; return old_sp; }
            };
            let src_port = unsafe { crate::network::socket::bound_port(sid) };
            if src_port == 0 { ctx.x0 = u64::MAX; return old_sp; }

            // dst_ip в a3 (u32: [a,b,c,d]), dst_port в a4
            let ip_u32 = a3 as u32;
            let dst_ip = [
                (ip_u32 >> 24) as u8,
                (ip_u32 >> 16) as u8,
                (ip_u32 >> 8) as u8,
                ip_u32 as u8,
            ];
            let dst_port = a4 as u16;

            // buf в a1, len в a2
            let buf_ptr = a1 as *const u8;
            let len = a2 as usize;
            if len > 1400 { ctx.x0 = u64::MAX; return old_sp; }

            let payload = core::slice::from_raw_parts(buf_ptr, len);

            let result = send_udp_raw(dst_ip, dst_port, src_port, payload);
            ctx.x0 = result as u64;
            old_sp
        }
        syscall::SYS_RECVFROM => {
            let a3 = ctx.x3;
            let a4 = ctx.x4;
            let sid = match unsafe { crate::fs::fd::fd_socket_id(a0) } {
                Some(s) => s,
                None => { ctx.x0 = u64::MAX; return old_sp; }
            };
            let pkt = match unsafe { crate::network::socket::dequeue(sid) } {
                Some(p) => p,
                None => { ctx.x0 = 0; return old_sp; }   // 0 = no data (nonblocking)
            };
            let buf_ptr = a1 as *mut u8;
            let max_len = a2 as usize;
            let copy_n = core::cmp::min(max_len, pkt.len as usize);
            for i in 0..copy_n {
                core::ptr::write_volatile(buf_ptr.add(i), pkt.data[i]);
            }
            // src_ip_out = a3, src_port_out = a4
            if a3 != 0 {
                let ip_out = a3 as *mut u8;
                for i in 0..4 {
                    core::ptr::write_volatile(ip_out.add(i), pkt.src_ip[i]);
                }
            }
            if a4 != 0 {
                let port_out = a4 as *mut u16;
                core::ptr::write_volatile(port_out, pkt.src_port);
            }
            ctx.x0 = copy_n as u64;
            old_sp
        }
        syscall::SYS_CONNECT => {
            let fd = a0 as i64;
            let ip_u32 = a1 as u32;
            let port = a2 as u16;
            let ip = [(ip_u32>>24) as u8, (ip_u32>>16) as u8, (ip_u32>>8) as u8, ip_u32 as u8];
            ctx.x0 = unsafe { crate::network::tcp::connect(fd, ip, port) } as u64;
            old_sp
        }
        syscall::SYS_TCP_SEND => {
            let fd = a0 as i64;
            let buf_ptr = a1 as *const u8;
            let len = a2 as usize;
            if len > 1400 { ctx.x0 = u64::MAX; return old_sp; }
            let data = unsafe { core::slice::from_raw_parts(buf_ptr, len) };
            ctx.x0 = unsafe { crate::network::tcp::tcp_send(fd, data) } as u64;
            old_sp
        }
        syscall::SYS_TCP_RECV => {
            let fd = a0 as i64;
            let buf_ptr = a1 as *mut u8;
            let len = a2 as usize;
            if len > 1024 { ctx.x0 = u64::MAX; return old_sp; }
            let mut tmp = [0u8; 1024];
            let n = unsafe { crate::network::tcp::tcp_recv(fd, &mut tmp[..len]) };
            if n > 0 {
                for i in 0..n as usize {
                    unsafe { core::ptr::write_volatile(buf_ptr.add(i), tmp[i]); }
                }
            }
            ctx.x0 = n as u64;
            old_sp
        }
        syscall::SYS_EXIT => sched::exit(),
        _ => {
            crate::println!("unknown syscall {}", num);
            ctx.x0 = u64::MAX;
            old_sp
        }
    }
}

unsafe fn sbrk_impl(delta: i64) -> u64 {
    use crate::arch::aarch64::sched;
    let cur = sched::current_pid();
    if cur == 0 {
        return u64::MAX;  // idle не имеет heap
    }
    let old_brk = sched::thread_brk(cur as usize);
    if old_brk == 0 {
        return u64::MAX;
    }

    let new_brk = (old_brk as i64).wrapping_add(delta) as u64;
    // Проверка границ: heap region 0x40800000..0x40A00000
    if new_brk < 0x40800000 || new_brk > 0x40A00000 {
        return u64::MAX;
    }

    sched::set_thread_brk(cur as usize, new_brk);
    old_brk
}

#[no_mangle]
pub extern "C" fn rust_exception_handler() -> ! {
    let esr: u64; unsafe { asm!("mrs {}, ESR_EL1", out(reg) esr) };
    let elr: u64; unsafe { asm!("mrs {}, ELR_EL1", out(reg) elr) };
    let far: u64; unsafe { asm!("mrs {}, FAR_EL1", out(reg) far) };
    let spsr: u64; unsafe { asm!("mrs {}, SPSR_EL1", out(reg) spsr) };

    println!();
    println!("!! EXCEPTION !!");
    println!("  EC       = {:#x}", (esr >> 26) & 0x3F);
    println!("  ESR_EL1  = {:#x}", esr);
    println!("  ELR_EL1  = {:#x}", elr);
    println!("  FAR_EL1  = {:#x}", far);
    println!("  SPSR_EL1 = {:#x}", spsr);
    println!("Halted.");
    loop { unsafe { asm!("wfe") } }
}

#[no_mangle]
pub unsafe extern "C" fn rust_irq_handler(old_sp: u64) -> u64 {
    let iar: u64;
    asm!("mrs {}, ICC_IAR1_EL1", out(reg) iar);
    let intid = (iar & 0xFFFFFF) as u32;

    if intid >= 1020 {
        return old_sp;
    }

    match intid {
        30 => crate::arch::aarch64::timer::handle_irq(),
        33 => {
            // UART IRQ: disable, EOI, сообщение в uart_server (pid=2)
            use crate::arch::aarch64::{gic, sched, ipc::Message};
            unsafe {
                gic::disable_irq(33);
            }
            // EOI делаем здесь, чтобы GIC не держал IRQ pending.
            // User прочитает UART DR и вызовет sys_irq_ack(33).
            unsafe {
                asm!("msr ICC_EOIR1_EL1, {}", in(reg) iar);
            }
            let msg = Message {
                tag: 2,   // 2 = IRQ
                data: [33, 0, 0, 0, 0, 0, 0],
            };
            unsafe { sched::ipc_send(2, msg); }
            return old_sp;
        }
        _ => {}
    }

    asm!("msr ICC_EOIR1_EL1, {}", in(reg) iar);

    crate::arch::aarch64::sched::on_tick(old_sp)
}

unsafe fn send_udp_raw(dst_ip: [u8; 4], dst_port: u16, src_port: u16, payload: &[u8]) -> i64 {
    use crate::drivers::virtio::net;
    use crate::device::arp;

    let vn = match crate::drivers::virtio::net::get_global_mut() {
        Some(v) => v,
        None => return -1,
    };

    let dst_mac = match arp::lookup(dst_ip) {
        Some(m) => m,
        None => return -1,
    };

    let our_ip = crate::network::dhcp::our_ip();

    // UDP
    let mut udp_buf = [0u8; 1500];
    let udp_len = crate::network::udp::build_udp_packet(
        &mut udp_buf, our_ip, dst_ip, src_port, dst_port, payload);

    // IPv4
    let total_len = 20 + udp_len;
    let mut packet = [0u8; 1500];
    crate::network::ip::build_ipv4_header(
        &mut packet[0..20], total_len as u16, 0, 17, our_ip, dst_ip);
    packet[20..20 + udp_len].copy_from_slice(&udp_buf[..udp_len]);

    // Ethernet
    let frame_len = 14 + total_len;
    let mut frame = [0u8; 1520];
    for i in 0..6 { frame[i] = dst_mac[i]; }
    for i in 0..6 { frame[6 + i] = vn.mac[i]; }
    frame[12] = 0x08;
    frame[13] = 0x00;
    frame[14..14 + total_len].copy_from_slice(&packet[..total_len]);

    let send_len = if frame_len < 60 { 60 } else { frame_len };

    if crate::drivers::virtio::net::send_packet(vn, &frame[..send_len]) {
        payload.len() as i64
    } else {
        -1
    }
}