const UART_BASE: usize = 0x0900_0000;
const UART_DR: *mut u32 = UART_BASE as *mut u32;
const UART_FR: *const u32 = (UART_BASE + 0x18) as *const u32;

pub fn putc(c: u8) {
    unsafe {
        while (*UART_FR & (1 << 5)) != 0 {}
        core::ptr::write_volatile(UART_DR, c as u32);
    }
}

pub fn puts(s: &str) {
    for b in s.bytes() {
        if b == b'\n' {
            putc(b'\r');
        }
        putc(b);
    }
}

pub struct Writer;

impl core::fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        puts(s);
        Ok(())
    }
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        let _ = core::fmt::Write::write_fmt(
            &mut $crate::uart::Writer,
            format_args!($($arg)*),
        );
    };
}

#[macro_export]
macro_rules! println {
    () => { $crate::uart::puts("\n") };
    ($($arg:tt)*) => {{
        $crate::print!($($arg)*);
        $crate::uart::puts("\n");
    }};
}