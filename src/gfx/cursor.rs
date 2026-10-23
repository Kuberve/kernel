const FB_ADDR: u64 = 0x48000000;
const FB_WIDTH: u32 = 640;
const FB_HEIGHT: u32 = 480;
const FB_STRIDE: u32 = FB_WIDTH * 4;
const CURSOR_COLOR: u32 = 0x00FFFFFF;
const CURSOR_SIZE: u32 = 7;
const HALF: u32 = CURSOR_SIZE / 2;  // = 3

pub static mut MOUSE_X: u32 = 320;
pub static mut MOUSE_Y: u32 = 240;
pub static mut VISIBLE: bool = false;

// Save-under buffer: что было под курсором
static mut SAVED_BG: [[u32; CURSOR_SIZE as usize]; CURSOR_SIZE as usize] =
    [[0; CURSOR_SIZE as usize]; CURSOR_SIZE as usize];

unsafe fn get_pixel(x: u32, y: u32) -> u32 {
    if x >= FB_WIDTH || y >= FB_HEIGHT { return 0; }
    let offset = (y * FB_STRIDE / 4 + x) as usize;
    core::ptr::read_volatile((FB_ADDR as *const u32).add(offset))
}

unsafe fn put_pixel(x: u32, y: u32, color: u32) {
    if x >= FB_WIDTH || y >= FB_HEIGHT { return; }
    let offset = (y * FB_STRIDE / 4 + x) as usize;
    core::ptr::write_volatile((FB_ADDR as *mut u32).add(offset), color);
}

unsafe fn save_under() {
    let mx = MOUSE_X;
    let my = MOUSE_Y;
    for dy in 0..CURSOR_SIZE {
        for dx in 0..CURSOR_SIZE {
            let x = mx.saturating_sub(HALF) + dx;
            let y = my.saturating_sub(HALF) + dy;
            SAVED_BG[dy as usize][dx as usize] = get_pixel(x, y);
        }
    }
}

unsafe fn restore_under() {
    let mx = MOUSE_X;
    let my = MOUSE_Y;
    for dy in 0..CURSOR_SIZE {
        for dx in 0..CURSOR_SIZE {
            let x = mx.saturating_sub(HALF) + dx;
            let y = my.saturating_sub(HALF) + dy;
            put_pixel(x, y, SAVED_BG[dy as usize][dx as usize]);
        }
    }
}

unsafe fn draw_at() {
    let mx = MOUSE_X;
    let my = MOUSE_Y;
    // Горизонталь
    for dx in 0..CURSOR_SIZE {
        let x = mx.saturating_sub(HALF) + dx;
        put_pixel(x, my, CURSOR_COLOR);
    }
    // Вертикаль
    for dy in 0..CURSOR_SIZE {
        let y = my.saturating_sub(HALF) + dy;
        put_pixel(mx, y, CURSOR_COLOR);
    }
}

/// Скрыть курсор: восстановить пиксели под ним.
pub unsafe fn hide() {
    if VISIBLE {
        restore_under();
        VISIBLE = false;
    }
}

/// Показать курсор: сохранить пиксели под ним и нарисовать.
pub unsafe fn show() {
    if !VISIBLE {
        save_under();
        draw_at();
        VISIBLE = true;
    }
}

/// Переместить курсор: hide → update coords → show.
pub unsafe fn move_to(mx: u32, my: u32) {
    hide();
    MOUSE_X = mx;
    MOUSE_Y = my;
    show();
}

/// Границы для flush (x, y, w, h).
pub fn dirty_rect() -> (u32, u32, u32, u32) {
    unsafe {
        let x = MOUSE_X.saturating_sub(HALF);
        let y = MOUSE_Y.saturating_sub(HALF);
        (x, y, CURSOR_SIZE, CURSOR_SIZE)
    }
}