use crate::gfx::compositor::{back_canvas, mark_dirty, FB_W, FB_H};
use crate::gfx::text;

pub const BG: u32 = 0x00101010;
pub const FG: u32 = 0x00E0E0E0;

pub struct Console {
    pub cursor_x: u32,
    pub cursor_y: u32,
}

pub static mut CONSOLE: Console = Console { cursor_x: 10, cursor_y: 10 };

static mut BLINK_VISIBLE: bool = true;
static mut BLINK_LAST: u64 = 0;
const BLINK_INTERVAL: u64 = 50;   // 0.5s при TICK_HZ=100

pub fn console() -> &'static mut Console { unsafe { &mut CONSOLE } }

const LINE_H: u32 = 16;
const CHAR_W: u32 = 8;
const LEFT: u32 = 10;

unsafe fn blink_draw(on: bool) {
    let mut cv = back_canvas();
    let x = CONSOLE.cursor_x;
    let y = CONSOLE.cursor_y + 16;   // под символом
    let color = if on { FG } else { BG };
    for dx in 0..8 {
        cv.put_pixel(x + dx, y, color);
        cv.put_pixel(x + dx, y + 1, color);
    }
    mark_dirty(x, y, 8, 2);
}

pub unsafe fn blink() {
    let now = crate::arch::aarch64::timer::tick_count();
    if now.wrapping_sub(BLINK_LAST) < BLINK_INTERVAL { return; }
    BLINK_LAST = now;
    if BLINK_VISIBLE {
        blink_draw(false);
        BLINK_VISIBLE = false;
    } else {
        blink_draw(true);
        BLINK_VISIBLE = true;
    }
}

pub unsafe fn init() {
    let mut cv = back_canvas();
    cv.fill_rect(0, 0, FB_W, FB_H, BG);
    mark_dirty(0, 0, FB_W, FB_H);
    CONSOLE.cursor_x = LEFT;
    CONSOLE.cursor_y = 10;
    BLINK_VISIBLE = false;
    BLINK_LAST = 0;
}

pub unsafe fn putchar(c: u8) {
    crate::gfx::cursor::hide();

    if BLINK_VISIBLE { blink_draw(false); }

    if c == b'\n' {
        CONSOLE.cursor_x = LEFT;
        CONSOLE.cursor_y += LINE_H;
        while CONSOLE.cursor_y + LINE_H > FB_H {
            scroll();
        }
    } else if c == b'\r' {
        CONSOLE.cursor_x = LEFT;
    } else {
        if CONSOLE.cursor_x + CHAR_W > FB_W {
            CONSOLE.cursor_x = LEFT;
            CONSOLE.cursor_y += LINE_H;
        }
        while CONSOLE.cursor_y + LINE_H > FB_H {
            scroll();
        }

        let dx = CONSOLE.cursor_x;
        let dy = CONSOLE.cursor_y;
        let mut cv = back_canvas();
        text::draw_char(&mut cv, dx, dy, c, FG, Some(BG));
        CONSOLE.cursor_x += CHAR_W;
        mark_dirty(dx, dy, CHAR_W, LINE_H);
    }

    blink_draw(true);
    BLINK_VISIBLE = true;
    BLINK_LAST = crate::arch::aarch64::timer::tick_count();

    crate::gfx::cursor::show();
}

pub unsafe fn backspace() {
    crate::gfx::cursor::hide();

    if BLINK_VISIBLE { blink_draw(false); }

    if CONSOLE.cursor_x > LEFT {
        CONSOLE.cursor_x -= CHAR_W;
        let mut cv = back_canvas();
        text::draw_char(&mut cv, CONSOLE.cursor_x, CONSOLE.cursor_y, b' ', FG, Some(BG));
        mark_dirty(CONSOLE.cursor_x, CONSOLE.cursor_y, CHAR_W, LINE_H);
    }

    blink_draw(true);
    BLINK_VISIBLE = true;
    BLINK_LAST = crate::arch::aarch64::timer::tick_count();

    crate::gfx::cursor::show();
}

unsafe fn scroll() {
    core::arch::asm!("msr daifset, #2");

    crate::gfx::cursor::hide();

    if BLINK_VISIBLE { blink_draw(false); }

    let ptr = 0x48200000u64 as *mut u32;
    let pixels = (FB_W * FB_H) as usize;
    let line_pixels = (FB_W * LINE_H) as usize;
    let tail_start = pixels - line_pixels;

    let ptr64 = ptr as *mut u64;
    let src_start = (line_pixels / 2) as usize;
    let n64 = (pixels - line_pixels) / 2;

    for i in 0..n64 {
        let val = core::ptr::read_volatile(ptr64.add(src_start + i));
        core::ptr::write_volatile(ptr64.add(i), val);
    }

    let c64 = (BG as u64) | ((BG as u64) << 32);
    let tail64 = ptr.add(tail_start) as *mut u64;
    for i in 0..(line_pixels / 2) {
        core::ptr::write_volatile(tail64.add(i), c64);
    }

    core::arch::asm!("dsb sy");

    CONSOLE.cursor_y -= LINE_H;
    mark_dirty(0, 0, FB_W, FB_H);

    blink_draw(true);
    BLINK_VISIBLE = true;
    BLINK_LAST = crate::arch::aarch64::timer::tick_count();

    crate::gfx::cursor::show();

    core::arch::asm!("msr daifclr, #2");
}