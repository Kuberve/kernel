use core::arch::asm;
use crate::arch::aarch64::ipc::Message;

pub use kuberve_abi::sys as sys_num;

// Номера syscall'ов
pub const SYS_YIELD:    u64 = sys_num::YIELD;
pub const SYS_WRITE:    u64 = sys_num::WRITE;
pub const SYS_EXIT:     u64 = sys_num::EXIT;
pub const SYS_SLEEP:    u64 = sys_num::SLEEP;
pub const SYS_GETPID:   u64 = sys_num::GETPID;
pub const SYS_SEND:     u64 = sys_num::SEND;
pub const SYS_RECV:     u64 = sys_num::RECV;
pub const SYS_IRQ_ACK:  u64 = sys_num::IRQ_ACK;
pub const SYS_TRY_SEND: u64 = sys_num::TRY_SEND;
pub const SYS_TRY_RECV: u64 = sys_num::TRY_RECV;
pub const SYS_SBRK: u64 = sys_num::SBRK;
pub const SYS_OPEN:  u64 = sys_num::OPEN;
pub const SYS_READ:  u64 = sys_num::READ;
pub const SYS_CLOSE: u64 = sys_num::CLOSE;
pub const SYS_WRITE_FILE: u64 = sys_num::WRITE_FILE;
pub const SYS_SOCKET:   u64 = sys_num::SOCKET;
pub const SYS_BIND:     u64 = sys_num::BIND;
pub const SYS_SENDTO:   u64 = sys_num::SENDTO;
pub const SYS_RECVFROM: u64 = sys_num::RECVFROM;
pub const SYS_CONNECT:  u64 = sys_num::CONNECT;
pub const SYS_TCP_SEND: u64 = sys_num::TCP_SEND;
pub const SYS_TCP_RECV: u64 = sys_num::TCP_RECV;

/// Отдать CPU добровольно.
#[allow(dead_code)]
pub fn sys_yield() {
    unsafe {
        asm!("mov x8, {n}", "svc #0", n = const SYS_YIELD, out("x8") _, out("x0") _);
    }
}

/// Записать buf в fd. fd=1 — stdout, fd=2 — stderr. Возвращает число байт или -1.
pub fn sys_write(fd: u64, buf: *const u8, len: u64) -> i64 {
    let ret: i64;
    unsafe {
        asm!(
            "mov x8, {n}",
            "svc #0",
            n = const SYS_WRITE,
            inlateout("x0") fd => ret,
            in("x1") buf,
            in("x2") len,
            out("x8") _,
        );
    }
    ret
}

/// Спать `ticks` тиков.
pub fn sys_sleep(ticks: u64) {
    unsafe {
        asm!(
            "mov x8, {n}",
            "svc #0",
            n = const SYS_SLEEP,
            inlateout("x0") ticks => _,
            out("x8") _,
        );
    }
}

/// Завершить поток. Не возвращается.
pub fn sys_exit(code: u64) -> ! {
    unsafe {
        asm!(
            "mov x8, {n}",
            "svc #0",
            n = const SYS_EXIT,
            in("x0") code,
            options(noreturn),
        );
    }
}

/// Реализация write в ядре. Пока только UART.
pub fn write(fd: u64, buf: *const u8, len: u64) -> i64 {
    if fd != 1 && fd != 2 {
        return -1;
    }
    for i in 0..len as usize {
        let c = unsafe { *buf.add(i) };
        crate::drivers::uart_pl011::putc(c);
    }
    len as i64
}

/// Отправить сообщение процессу target. Возвращает 0 или -1.
#[allow(dead_code)]
pub fn sys_send(target: u64, msg: *const Message) -> i64 {
    let ret: i64;
    unsafe {
        asm!(
            "mov x8, {n}",
            "svc #0",
            n = const SYS_SEND,
            inlateout("x0") target => ret,
            in("x1") msg,
            out("x8") _,
        );
    }
    ret
}

/// Принять сообщение в msg. Блокируется, пока не придёт. Возвращает 0.
#[allow(dead_code)]
pub fn sys_recv(msg: *mut Message) -> i64 {
    let ret: i64;
    unsafe {
        asm!(
            "mov x8, {n}",
            "svc #0",
            n = const SYS_RECV,
            inlateout("x0") msg as u64 => ret,
            out("x8") _,
        );
    }
    ret
}

// Возвращает PID текущего потока. Пока просто индекс в массиве THREADS.
#[allow(dead_code)]
pub fn sys_getpid() -> u64 {
    let ret: u64;
    unsafe {
        asm!(
            "mov x8, {n}",
            "svc #0",
            n = const SYS_GETPID,
            lateout("x0") ret,
            out("x8") _,
        );
    }
    ret
}

// Подтвердить обработку IRQ. intid — номер прерывания.
#[allow(dead_code)]
pub fn sys_irq_ack(intid: u64) {
    unsafe {
        asm!(
            "mov x8, {n}",
            "svc #0",
            n = const SYS_IRQ_ACK,
            inlateout("x0") intid => _,
            out("x8") _,
        );
    }
}

#[allow(dead_code)]
pub fn sys_try_send(target: u64, msg: *const Message) -> i64 {
    let ret: i64;
    unsafe {
        asm!("mov x8, {n}", "svc #0",
            n = const SYS_TRY_SEND,
            inlateout("x0") target => ret,
            in("x1") msg, out("x8") _);
    }
    ret
}

#[allow(dead_code)]
pub fn sys_try_recv(msg: *mut Message) -> i64 {
    let ret: i64;
    unsafe {
        asm!("mov x8, {n}", "svc #0",
            n = const SYS_TRY_RECV,
            inlateout("x0") msg as u64 => ret,
            out("x8") _);
    }
    ret
}

#[allow(dead_code)]
pub fn sys_sbrk(delta: i64) -> u64 {
    let ret: u64;
    unsafe {
        asm!("mov x8, {n}", "svc #0",
            n = const sys_num::SBRK,
            inlateout("x0") delta => ret,
            out("x8") _);
    }
    ret
}

#[allow(dead_code)]
pub fn sys_open(path: &str, flags: u64) -> i64 {
    let ret: i64;
    unsafe {
        asm!("mov x8, {n}", "svc #0",
            n = const sys_num::OPEN,
            inlateout("x0") path.as_ptr() as u64 => ret,
            in("x1") path.len() as u64,
            in("x2") flags,
            out("x8") _);
    }
    ret
}

#[allow(dead_code)]
pub fn sys_read(fd: i64, buf: *mut u8, len: u64) -> i64 {
    let ret: i64;
    unsafe {
        asm!("mov x8, {n}", "svc #0",
            n = const sys_num::READ,
            inlateout("x0") fd as u64 => ret,
            in("x1") buf as u64,
            in("x2") len,
            out("x8") _);
    }
    ret
}

#[allow(dead_code)]
pub fn sys_close(fd: i64) -> i64 {
    let ret: i64;
    unsafe {
        asm!("mov x8, {n}", "svc #0",
            n = const sys_num::CLOSE,
            inlateout("x0") fd as u64 => ret,
            out("x8") _);
    }
    ret
}

#[allow(dead_code)]
pub fn sys_write_file(fd: i64, buf: *const u8, len: u64) -> i64 {
    let ret: i64;
    unsafe {
        asm!("mov x8, {n}", "svc #0",
            n = const sys_num::WRITE_FILE,
            inlateout("x0") fd as u64 => ret,
            in("x1") buf as u64,
            in("x2") len,
            out("x8") _);
    }
    ret
}

#[allow(dead_code)]
pub fn sys_socket(domain: u64, kind: u64) -> i64 {
    let ret: i64;
    unsafe {
        asm!("mov x8, {n}", "svc #0", n = const sys_num::SOCKET,
            inlateout("x0") domain => ret,
            in("x1") kind, out("x8") _);
    }
    ret
}

#[allow(dead_code)]
pub fn sys_bind(fd: i64, port: u64) -> i64 {
    let ret: i64;
    unsafe {
        asm!("mov x8, {n}", "svc #0", n = const sys_num::BIND,
            inlateout("x0") fd as u64 => ret,
            in("x1") port, out("x8") _);
    }
    ret
}

#[allow(dead_code)]
pub fn sys_sendto(fd: i64, buf: *const u8, len: u64, dst_ip: u64, dst_port: u64) -> i64 {
    let ret: i64;
    unsafe {
        asm!("mov x8, {n}", "svc #0", n = const sys_num::SENDTO,
            inlateout("x0") fd as u64 => ret,
            in("x1") buf as u64, in("x2") len,
            in("x3") dst_ip, in("x4") dst_port, out("x8") _);
    }
    ret
}

#[allow(dead_code)]
pub fn sys_recvfrom(fd: i64, buf: *mut u8, len: u64, src_ip_out: *mut u8, src_port_out: *mut u16) -> i64 {
    let ret: i64;
    unsafe {
        asm!("mov x8, {n}", "svc #0", n = const sys_num::RECVFROM,
            inlateout("x0") fd as u64 => ret,
            in("x1") buf as u64, in("x2") len,
            in("x3") src_ip_out as u64, in("x4") src_port_out as u64, out("x8") _);
    }
    ret
}