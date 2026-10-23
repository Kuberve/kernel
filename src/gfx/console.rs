use crate::gfx::text;
use crate::drivers::virtio::gpu;

const FB_ADDR: u64 = 0x48000000;
const FB_WIDTH: u32 = 640;
const FB_HEIGHT: u32 = 480;
const BG_COLOR: u32 = 0x00101010;
const FG_COLOR: u32 = 0x00E0E0E0;

pub struct Console {
    pub cursor_x: u32,
    pub cursor_y: u32,
    pub line_height: u32,
    pub char_width: u32,
}

pub static mut CONSOLE: Console = Console {
    cursor_x: 10,
    cursor_y: 10,
    line_height: 16,
    char_width: 8,
};

pub fn console() -> &'static mut Console {
    unsafe { &mut CONSOLE }
}

// Dirty region
static mut DIRTY_X0: u32 = u32::MAX;
static mut DIRTY_Y0: u32 = u32::MAX;
static mut DIRTY_X1: u32 = 0;
static mut DIRTY_Y1: u32 = 0;
static mut HAS_DIRTY: bool = false;

unsafe fn mark_dirty(x: u32, y: u32) {
    if x < DIRTY_X0 { DIRTY_X0 = x; }
    if y < DIRTY_Y0 { DIRTY_Y0 = y; }
    let rx = x + 8;
    let ry = y + 16;
    if rx > DIRTY_X1 { DIRTY_X1 = rx; }
    if ry > DIRTY_Y1 { DIRTY_Y1 = ry; }
    HAS_DIRTY = true;
}

pub unsafe fn init() {
    let total = (FB_WIDTH * FB_HEIGHT) as usize;
    let ptr = FB_ADDR as *mut u32;
    for i in 0..total {
        core::ptr::write_volatile(ptr.add(i), BG_COLOR);
    }
    CONSOLE.cursor_x = 10;
    CONSOLE.cursor_y = 10;
}

pub unsafe fn putchar(c: u8) {
    let draw_x = CONSOLE.cursor_x;
    let draw_y = CONSOLE.cursor_y;

    if c == b'\n' {
        CONSOLE.cursor_x = 10;
        CONSOLE.cursor_y += CONSOLE.line_height;
    } else if c == b'\r' {
        CONSOLE.cursor_x = 10;
    } else {
        text::draw_char(draw_x, draw_y, c, FG_COLOR, Some(BG_COLOR));
        CONSOLE.cursor_x += CONSOLE.char_width;
        if CONSOLE.cursor_x + CONSOLE.char_width > FB_WIDTH {
            CONSOLE.cursor_x = 10;
            CONSOLE.cursor_y += CONSOLE.line_height;
        }
        // Mark dirty только там, где нарисовали
        mark_dirty(draw_x, draw_y);
    }
    if CONSOLE.cursor_y + CONSOLE.line_height > FB_HEIGHT {
        CONSOLE.cursor_y = 10;
    }
}

pub unsafe fn backspace() {
    if CONSOLE.cursor_x > 10 {
        CONSOLE.cursor_x -= CONSOLE.char_width;
        text::draw_char(CONSOLE.cursor_x, CONSOLE.cursor_y, b' ', FG_COLOR, Some(BG_COLOR));
        mark_dirty(CONSOLE.cursor_x, CONSOLE.cursor_y);
    }
}

pub unsafe fn flush(gpu: &mut crate::drivers::virtio::gpu::VirtioGpu) {
    crate::drivers::virtio::gpu::flush(gpu, FB_WIDTH, FB_HEIGHT);
}