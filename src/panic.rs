use crate::uart;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    uart::puts("\n[PANIC] ");
    if let Some(loc) = info.location() {
        uart::puts(loc.file());
        uart::puts(":");
        // Somewhat inefficient way to print the line number, but it works without any heap allocation or formatting.
        let mut n = loc.line();
        let mut buf = [0u8; 10];
        let mut i = 0;
        if n == 0 { buf[0] = b'0'; i = 1; }
        while n > 0 {
            buf[i] = b'0' + (n % 10) as u8;
            n /= 10;
            i += 1;
        }
        while i > 0 {
            i -= 1;
            uart::putc(buf[i]);
        }
    }
    uart::puts("\n");
    loop {
        unsafe { core::arch::asm!("wfe") }
    }
}