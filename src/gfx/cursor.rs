use crate::gfx::compositor::{back_canvas, mark_dirty, FB_W, FB_H};

const SIZE: u32 = 7;
const HALF: u32 = SIZE / 2;
const COLOR: u32 = 0x00FFFFFF;

pub static mut MOUSE_X: u32 = 320;
pub static mut MOUSE_Y: u32 = 240;
pub static mut VISIBLE: bool = false;

static mut SAVED_BG: [[u32; SIZE as usize]; SIZE as usize] = [[0; SIZE as usize]; SIZE as usize];

unsafe fn save_bg() {
    let mx = MOUSE_X; let my = MOUSE_Y;
    for dy in 0..SIZE {
        for dx in 0..SIZE {
            let x = mx.saturating_sub(HALF) + dx;
            let y = my.saturating_sub(HALF) + dy;
            SAVED_BG[dy as usize][dx as usize] = if x < FB_W && y < FB_H {
                core::ptr::read_volatile((0x48200000u64 as *const u32).add((y * FB_W + x) as usize))
            } else { 0 };
        }
    }
}

unsafe fn restore_bg() {
    let mx = MOUSE_X; let my = MOUSE_Y;
    let dst = 0x48200000u64 as *mut u32;
    for dy in 0..SIZE {
        for dx in 0..SIZE {
            let x = mx.saturating_sub(HALF) + dx;
            let y = my.saturating_sub(HALF) + dy;
            if x < FB_W && y < FB_H {
                core::ptr::write_volatile(dst.add((y * FB_W + x) as usize), SAVED_BG[dy as usize][dx as usize]);
            }
        }
    }
}

unsafe fn draw() {
    let mut cv = back_canvas();
    let mx = MOUSE_X; let my = MOUSE_Y;
    for d in 0..SIZE {
        let x = mx.saturating_sub(HALF) + d;
        if x < FB_W && my < FB_H { cv.put_pixel(x, my, COLOR); }
    }
    for d in 0..SIZE {
        let y = my.saturating_sub(HALF) + d;
        if mx < FB_W && y < FB_H { cv.put_pixel(mx, y, COLOR); }
    }
}

pub unsafe fn hide() {
    if VISIBLE {
        restore_bg();
        mark_dirty(MOUSE_X.saturating_sub(HALF), MOUSE_Y.saturating_sub(HALF), SIZE, SIZE);
        VISIBLE = false;
    }
}

pub unsafe fn show() {
    if !VISIBLE {
        save_bg();
        draw();
        mark_dirty(MOUSE_X.saturating_sub(HALF), MOUSE_Y.saturating_sub(HALF), SIZE, SIZE);
        VISIBLE = true;
    }
}

pub unsafe fn move_to(nx: u32, ny: u32) {
    let old_x = MOUSE_X; let old_y = MOUSE_Y;
    if VISIBLE { restore_bg(); }
    MOUSE_X = nx.min(FB_W - 1);
    MOUSE_Y = ny.min(FB_H - 1);
    if VISIBLE { save_bg(); draw(); }
    let min_x = old_x.min(MOUSE_X).saturating_sub(HALF);
    let min_y = old_y.min(MOUSE_Y).saturating_sub(HALF);
    let max_x = old_x.max(MOUSE_X) + HALF + 1;
    let max_y = old_y.max(MOUSE_Y) + HALF + 1;
    mark_dirty(min_x, min_y, max_x - min_x, max_y - min_y);
}