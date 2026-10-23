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
        mark_dirty(draw_x, draw_y);
    }

    // Scroll если нужно
    while CONSOLE.cursor_y + CONSOLE.line_height > FB_HEIGHT {
        scroll();
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

unsafe fn scroll() {
    // Скрыть курсор (восстановить SAVED_BG под ним — крестик не попадёт в copy)
    crate::gfx::cursor::hide();

    let ptr = FB_ADDR as *mut u32;
    let line_h = CONSOLE.line_height as usize;
    let pixels_per_row = FB_WIDTH as usize;
    let rows = FB_HEIGHT as usize;

    if line_h >= rows { return; }

    let src = ptr.add(line_h * pixels_per_row);
    let n = (rows - line_h) * pixels_per_row;
    core::ptr::copy(src, ptr, n);

    let clear_start = ptr.add((rows - line_h) * pixels_per_row);
    let clear_n = line_h * pixels_per_row;
    for i in 0..clear_n {
        core::ptr::write_volatile(clear_start.add(i), BG_COLOR);
    }

    CONSOLE.cursor_y -= CONSOLE.line_height;

    // Показать курсор (сохранить новый текст под ним, нарисовать крестик)
    crate::gfx::cursor::show();

    // Полный flush (весь экран сдвинулся)
    DIRTY_X0 = 0;
    DIRTY_Y0 = 0;
    DIRTY_X1 = FB_WIDTH;
    DIRTY_Y1 = FB_HEIGHT;
    HAS_DIRTY = true;
}