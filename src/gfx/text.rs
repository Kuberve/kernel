use crate::gfx::font::get_bitmap;
use crate::gfx::compositor::Canvas;

pub unsafe fn draw_char(cv: &mut Canvas, x: u32, y: u32, c: u8, fg: u32, bg: Option<u32>) {
    let bitmap = get_bitmap(c);
    for row in 0..16u32 {
        let bits = bitmap[row as usize];
        for col in 0..8u32 {
            let on = (bits >> (7 - col)) & 1 != 0;
            if on {
                cv.put_pixel(x + col, y + row, fg);
            } else if let Some(bg_color) = bg {
                cv.put_pixel(x + col, y + row, bg_color);
            }
        }
    }
}

pub unsafe fn draw_string(cv: &mut Canvas, x: u32, y: u32, s: &str, fg: u32, bg: Option<u32>) {
    let mut cx = x;
    for byte in s.bytes() {
        if byte == b'\n' { continue; }
        draw_char(cv, cx, y, byte, fg, bg);
        cx += 8;
    }
}

pub unsafe fn draw_string_wrap(cv: &mut Canvas, x: u32, mut y: u32, s: &str, fg: u32, bg: Option<u32>) -> u32 {
    let mut cx = x;
    let max_x = cv.w - 8;
    for byte in s.bytes() {
        if byte == b'\n' { cx = x; y += 16; continue; }
        if cx > max_x { cx = x; y += 16; }
        draw_char(cv, cx, y, byte, fg, bg);
        cx += 8;
    }
    y
}