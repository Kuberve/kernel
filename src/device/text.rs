use crate::device::font8x16::get_bitmap;

const FB_ADDR: u64 = 0x48000000;
const FB_WIDTH: u32 = 640;
const FB_HEIGHT: u32 = 480;
const FB_STRIDE: u32 = FB_WIDTH * 4;

fn put_pixel(x: u32, y: u32, color: u32) {
    if x >= FB_WIDTH || y >= FB_HEIGHT { return; }
    let offset = (y * FB_STRIDE / 4 + x) as usize;
    unsafe {
        let ptr = (FB_ADDR as *mut u32).add(offset);
        core::ptr::write_volatile(ptr, color);
    }
}

pub fn draw_char(x: u32, y: u32, c: u8, fg: u32, bg: Option<u32>) {
    let bitmap = get_bitmap(c);
    for row in 0..16u32 {
        let bits = bitmap[row as usize];
        for col in 0..8u32 {
            let on = (bits >> (7 - col)) & 1 != 0;
            if on {
                put_pixel(x + col, y + row, fg);
            } else if let Some(bg_color) = bg {
                put_pixel(x + col, y + row, bg_color);
            }
        }
    }
}

pub fn draw_string(x: u32, y: u32, s: &str, fg: u32, bg: Option<u32>) {
    let mut cx = x;
    for byte in s.bytes() {
        if byte == b'\n' {
            // TODO: handle newline? for now skip
            continue;
        }
        draw_char(cx, y, byte, fg, bg);
        cx += 8;
    }
}

pub fn draw_string_wrap(x: u32, mut y: u32, s: &str, fg: u32, bg: Option<u32>) -> u32 {
    let mut cx = x;
    let max_x = FB_WIDTH - 8;
    for byte in s.bytes() {
        if byte == b'\n' {
            cx = x;
            y += 16;
            continue;
        }
        if cx > max_x {
            cx = x;
            y += 16;
        }
        draw_char(cx, y, byte, fg, bg);
        cx += 8;
    }
    y
}