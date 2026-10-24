use alloc::vec::Vec;
use alloc::vec;
use crate::gfx::text;

const FB_ADDR: u64 = 0x48000000;
const FB_WIDTH: u32 = 640;
const FB_HEIGHT: u32 = 480;
const FB_STRIDE: u32 = FB_WIDTH * 4;

const TITLE_H: u32 = 20;
const BORDER: u32 = 2;
const MAX_WIN: usize = 4;

// Colors (BGRX)
const BORDER_FOCUS: u32 = 0x00FFFFFF;
const BORDER_UNFOCUS: u32 = 0x00606060;
const TITLE_FOCUS: u32 = 0x00A08000;
const TITLE_UNFOCUS: u32 = 0x00404040;
const TITLE_FG: u32 = 0x00FFFFFF;
const CONTENT_BG: u32 = 0x00101010;
const CLOSE_BG: u32 = 0x00C00000;
const CLOSE_FG: u32 = 0x00FFFFFF;

pub struct Window {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub title: &'static str,
    pub visible: bool,
    pub focused: bool,
    pub under: Vec<u32>,   // save-under: w*h (пиксели FB до рисования)
    pub content_lines: &'static [&'static str],
}

pub struct Wm {
    pub windows: [Option<Window>; MAX_WIN],
    pub drag_idx: Option<usize>,
    pub drag_off_x: u32,
    pub drag_off_y: u32,
}

pub static mut WM: Wm = Wm {
    windows: [None, None, None, None],
    drag_idx: None,
    drag_off_x: 0,
    drag_off_y: 0,
};

pub fn wm() -> &'static mut Wm {
    unsafe { &mut WM }
}

pub fn find_at(mx: u32, my: u32) -> Option<usize> {
    let wm = wm();
    // topmost first: iterate in reverse order (last added = on top)
    for i in (0..MAX_WIN).rev() {
        if let Some(win) = &wm.windows[i] {
            if win.visible
                && mx >= win.x && mx < win.x + win.w
                && my >= win.y && my < win.y + win.h {
                return Some(i);
            }
        }
    }
    None
}

/// Попадание в title bar (и не в X).
pub fn hit_title(win_idx: usize, mx: u32, my: u32) -> bool {
    let wm = wm();
    if let Some(win) = &wm.windows[win_idx] {
        let in_title_y = my >= win.y + BORDER && my < win.y + BORDER + TITLE_H;
        let in_x_range = mx >= win.x + BORDER && mx < win.x + win.w - BORDER;
        let cx = win.x + win.w - BORDER - 16;
        let in_x = mx >= cx && mx < cx + 16;
        return in_title_y && in_x_range && !in_x;
    }
    false
}

pub fn hit_close(win_idx: usize, mx: u32, my: u32) -> bool {
    let wm = wm();
    if let Some(win) = &wm.windows[win_idx] {
        let cx = win.x + win.w - BORDER - 16;
        let cy = win.y + BORDER + 2;
        return mx >= cx && mx < cx + 16 && my >= cy && my < cy + TITLE_H - 4;
    }
    false
}

fn put_pixel(x: u32, y: u32, c: u32) {
    if x >= FB_WIDTH || y >= FB_HEIGHT { return; }
    unsafe {
        let p = (FB_ADDR as *mut u32).add((y * FB_STRIDE / 4 + x) as usize);
        core::ptr::write_volatile(p, c);
    }
}

fn get_pixel(x: u32, y: u32) -> u32 {
    if x >= FB_WIDTH || y >= FB_HEIGHT { return 0; }
    unsafe {
        core::ptr::read_volatile((FB_ADDR as *const u32).add((y * FB_STRIDE / 4 + x) as usize))
    }
}

fn fill_rect(x: u32, y: u32, w: u32, h: u32, color: u32) {
    for dy in 0..h {
        for dx in 0..w {
            put_pixel(x + dx, y + dy, color);
        }
    }
}

fn save_under(win: &mut Window) {
    let total = (win.w * win.h) as usize;
    win.under.clear();
    win.under.resize(total, 0);
    for dy in 0..win.h {
        for dx in 0..win.w {
            win.under[(dy * win.w + dx) as usize] = get_pixel(win.x + dx, win.y + dy);
        }
    }
}

fn restore_under(win: &Window) {
    for dy in 0..win.h {
        for dx in 0..win.w {
            let c = win.under[(dy * win.w + dx) as usize];
            put_pixel(win.x + dx, win.y + dy, c);
        }
    }
}

unsafe fn draw_window(win: &Window) {
    let bc = if win.focused { BORDER_FOCUS } else { BORDER_UNFOCUS };
    let tc = if win.focused { TITLE_FOCUS } else { TITLE_UNFOCUS };

    // Content bg
    fill_rect(win.x, win.y, win.w, win.h, CONTENT_BG);
    // Border
    fill_rect(win.x, win.y, win.w, BORDER, bc);
    fill_rect(win.x, win.y + win.h - BORDER, win.w, BORDER, bc);
    fill_rect(win.x, win.y, BORDER, win.h, bc);
    fill_rect(win.x + win.w - BORDER, win.y, BORDER, win.h, bc);
    // Title bar
    fill_rect(win.x + BORDER, win.y + BORDER, win.w - 2*BORDER, TITLE_H, tc);
    // Title text
    text::draw_string(win.x + BORDER + 4, win.y + BORDER + 2, win.title, TITLE_FG, None);
    // Close [X]
    let cx = win.x + win.w - BORDER - 16;
    let cy = win.y + BORDER + 2;
    fill_rect(cx, cy, 16, TITLE_H - 4, CLOSE_BG);
    text::draw_string(cx + 4, cy + 2, "X", CLOSE_FG, None);

    // Content lines
    let mut ly = win.y + BORDER + TITLE_H + 4;
    for line in win.content_lines {
        text::draw_string(win.x + BORDER + 4, ly, line, 0x00A0A0A0, None);
        ly += 14;
    }
}

// === Public API ===

pub fn init() {}

pub unsafe fn add_window(
    x: u32, y: u32, w: u32, h: u32,
    title: &'static str,
    content_lines: &'static [&'static str],
) -> Option<usize> {
    let was_vis = crate::gfx::cursor::VISIBLE;
    crate::gfx::cursor::hide();

    let wm = wm();
    let mut idx = None;
    for i in 0..MAX_WIN {
        if wm.windows[i].is_none() {
            let mut win = Window {
                x, y, w, h, title,
                visible: false, focused: false,
                under: Vec::new(),
                content_lines,
            };
            save_under(&mut win);
            win.visible = true;
            draw_window(&win);
            wm.windows[i] = Some(win);
            idx = Some(i);
            break;
        }
    }

    if was_vis { crate::gfx::cursor::show(); }
    idx
}

pub unsafe fn focus(win_idx: usize) {
    let was_vis = crate::gfx::cursor::VISIBLE;
    crate::gfx::cursor::hide();

    let wm = wm();
    for i in 0..MAX_WIN {
        if let Some(win) = &mut wm.windows[i] {
            let new_f = i == win_idx;
            if win.focused != new_f {
                win.focused = new_f;
                if win.visible {
                    draw_window(win);
                }
            }
        }
    }

    if was_vis { crate::gfx::cursor::show(); }
}

pub unsafe fn start_drag(win_idx: usize, mx: u32, my: u32) {
    let wm = wm();
    if let Some(win) = &wm.windows[win_idx] {
        wm.drag_off_x = mx - win.x;
        wm.drag_off_y = my - win.y;
    }
    wm.drag_idx = Some(win_idx);
}

pub unsafe fn drag_to(mx: u32, my: u32) -> (u32, u32, u32, u32) {
    let was_vis = crate::gfx::cursor::VISIBLE;
    crate::gfx::cursor::hide();

    let wm = wm();
    let mut union = (0u32, 0u32, 0u32, 0u32);

    if let Some(idx) = wm.drag_idx {
        if let Some(win) = &mut wm.windows[idx] {
            let old_x = win.x;
            let old_y = win.y;

            // Restore old position
            restore_under(win);

            // Update coords
            let nx = mx.saturating_sub(wm.drag_off_x);
            let ny = my.saturating_sub(wm.drag_off_y);
            win.x = nx.min(FB_WIDTH.saturating_sub(win.w));
            win.y = ny.min(FB_HEIGHT.saturating_sub(win.h));

            // Save under new position + draw
            save_under(win);
            draw_window(win);

            // Union rect для flush
            let min_x = old_x.min(win.x);
            let min_y = old_y.min(win.y);
            let max_x = (old_x + win.w).max(win.x + win.w);
            let max_y = (old_y + win.h).max(win.y + win.h);
            union = (min_x, min_y, max_x - min_x, max_y - min_y);

            // Cursor at mouse
            crate::gfx::cursor::MOUSE_X = mx.min(FB_WIDTH - 1);
            crate::gfx::cursor::MOUSE_Y = my.min(FB_HEIGHT - 1);
        }
    }

    if was_vis { crate::gfx::cursor::show(); }
    union
}

pub unsafe fn end_drag() {
    wm().drag_idx = None;
}

pub unsafe fn close_window(win_idx: usize) {
    let was_vis = crate::gfx::cursor::VISIBLE;
    crate::gfx::cursor::hide();

    let wm = wm();
    if let Some(win) = &mut wm.windows[win_idx] {
        if win.visible {
            restore_under(win);
        }
        wm.windows[win_idx] = None;
    }

    if was_vis { crate::gfx::cursor::show(); }
}

pub unsafe fn redraw_all() {
    let was_vis = crate::gfx::cursor::VISIBLE;
    crate::gfx::cursor::hide();

    let wm = wm();
    for i in 0..MAX_WIN {
        if let Some(win) = &mut wm.windows[i] {
            if win.visible {
                save_under(win);
                draw_window(win);
            }
        }
    }

    if was_vis { crate::gfx::cursor::show(); }
}