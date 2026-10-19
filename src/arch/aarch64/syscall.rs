use core::arch::asm;
use crate::arch::aarch64::ipc::Message;

// Номера syscall'ов
pub const SYS_YIELD: u64 = 0;
pub const SYS_WRITE: u64 = 1;
pub const SYS_EXIT:  u64 = 2;
pub const SYS_SLEEP: u64 = 3;
pub const SYS_GETPID: u64 = 4;
pub const SYS_SEND: u64 = 5;
pub const SYS_RECV: u64 = 6;
pub const SYS_IRQ_ACK: u64 = 7;

/// Отдать CPU добровольно.
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
        crate::uart::putc(c);
    }
    len as i64
}

/// Отправить сообщение процессу target. Возвращает 0 или -1.
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