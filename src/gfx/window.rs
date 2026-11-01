use crate::gfx::compositor::{back_canvas, mark_dirty, FB_W, FB_H};
use crate::gfx::font::get_bitmap;

pub const MAX_WIN: usize = 4;
pub const MAX_WIN_W: u32 = 320;
pub const MAX_WIN_H: u32 = 240;

pub static mut DRAG_WIN: Option<usize> = None;
pub static mut DRAG_OFF_X: u32 = 0;
pub static mut DRAG_OFF_Y: u32 = 0;


// Цвета (формат 0x00BBGGRR)
pub const COLOR_DESKTOP: u32 = 0x00333333; // #333333
pub const COLOR_WIN_BG:  u32 = 0x003C3C3C; 
pub const COLOR_BORDER:  u32 = 0x000095DF; // #0095df
pub const COLOR_TITLE:   u32 = 0x000095DF; // #0095df
pub const COLOR_TITLE_FG:u32 = 0x00FFFFFF; 
pub const COLOR_CLOSE:   u32 = 0x00222222; 

pub const COLOR_BORDER_UNFOCUSED: u32 = 0x00555555;
pub const COLOR_TITLE_UNFOCUSED:  u32 = 0x00444444;

pub struct Window {
    pub id: usize,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub title_buf: [u8; 32],
    pub title_len: u8,
    pub visible: bool,
    pub focused: bool,
    pub bitmap: [u32; (MAX_WIN_W * MAX_WIN_H) as usize],
    pub cursor_x: u32,
    pub cursor_y: u32,
}

impl Window {
    pub const fn new() -> Self {
        Self {
            id: 0, x: 0, y: 0, w: 0, h: 0,
            title_buf: [0; 32], title_len: 0,
            visible: false, focused: false,
            bitmap: [0; (MAX_WIN_W * MAX_WIN_H) as usize],
            cursor_x: 10,
            cursor_y: 30,
        }
    }

    pub fn title(&self) -> &str {
        core::str::from_utf8(&self.title_buf[..self.title_len as usize]).unwrap_or("")
    }

    pub fn set_title(&mut self, s: &str) {
        let n = core::cmp::min(s.len(), 31);
        self.title_buf[..n].copy_from_slice(&s.as_bytes()[..n]);
        self.title_len = n as u8;
    }

    // Рисует рамку и заголовок в bitmap окна
    pub fn draw_chrome(&mut self) {
        let bw = 2;
        let th = 20;
        let b_color = if self.focused { COLOR_BORDER } else { COLOR_BORDER_UNFOCUSED };
        let t_color = if self.focused { COLOR_TITLE } else { COLOR_TITLE_UNFOCUSED };

        // 1. Рамка
        for x in 0..self.w {
            self.bitmap[x as usize] = b_color;
            self.bitmap[((self.h - 1) * self.w + x) as usize] = b_color;
        }
        for y in 0..self.h {
            self.bitmap[(y * self.w) as usize] = b_color;
            self.bitmap[(y * self.w + self.w - 1) as usize] = b_color;
        }

        // 2. Titlebar
        for y in bw..(bw + th) {
            for x in bw..(self.w - bw) {
                self.bitmap[(y * self.w + x) as usize] = t_color;
            }
        }

        // 3. Текст заголовка
        let mut tx = bw + 4;
        let ty = bw + 2;
        let title_len = self.title_len as usize;
        for i in 0..title_len {
            let c = self.title_buf[i];
            let bitmap = get_bitmap(c);
            for row in 0..16u32 {
                let bits = bitmap[row as usize];
                for col in 0..8u32 {
                    if (bits >> (7 - col)) & 1 != 0 {
                        let px = tx + col;
                        let py = ty + row;
                        if px < self.w - bw && py < bw + th {
                            self.bitmap[(py * self.w + px) as usize] = COLOR_TITLE_FG;
                        }
                    }
                }
            }
            tx += 8;
        }

        // 4. Кнопка закрытия
        let cx = self.w - bw - 16;
        let cy = bw + 2;
        for y in cy..(cy + 16) {
            for x in cx..(cx + 16) {
                self.bitmap[(y * self.w + x) as usize] = COLOR_CLOSE;
            }
        }
        for i in 0..10 {
            self.bitmap[((cy + 3 + i) * self.w + cx + 3 + i) as usize] = COLOR_TITLE_FG;
            self.bitmap[((cy + 3 + i) * self.w + cx + 13 - i) as usize] = COLOR_TITLE_FG;
        }
    }

    // Копирует bitmap окна в FB_BACK и помечает dirty
    pub unsafe fn blit_to_back(&self) {
        if !self.visible { return; }
        let fb = back_canvas();
        let cv = fb;
        
        for y in 0..self.h {
            let dst_y = self.y + y;
            if dst_y >= FB_H { break; }
            
            let src_row = (y * self.w) as usize;
            let dst_row = (dst_y * FB_W + self.x) as usize;
            
            let w = core::cmp::min(self.w, FB_W - self.x);
            for x in 0..w {
                let color = self.bitmap[src_row + x as usize];
                core::ptr::write_volatile(cv.ptr.add(dst_row + x as usize), color);
            }
        }
        
        mark_dirty(self.x, self.y, self.w, self.h);
    }

    // Заливка прямоугольника внутри окна (не выходит за границы окна)
    pub fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: u32) {
        let x_end = core::cmp::min(x + w, self.w);
        let y_end = core::cmp::min(y + h, self.h);
        for yy in y..y_end {
            for xx in x..x_end {
                self.bitmap[(yy * self.w + xx) as usize] = color;
            }
        }
    }

    // Вывод текста внутри окна (с поддержкой переноса строки \n)
    pub fn draw_text(&mut self, start_x: u32, start_y: u32, text: &str, color: u32) {
        let mut cx = start_x;
        let mut cy = start_y;
        for c in text.bytes() {
            if c == b'\n' {
                cx = start_x;
                cy += 16; // межстрочный интервал
                continue;
            }
            let bitmap = get_bitmap(c);
            for row in 0..16u32 {
                let bits = bitmap[row as usize];
                for col in 0..8u32 {
                    if (bits >> (7 - col)) & 1 != 0 {
                        let px = cx + col;
                        let py = cy + row;
                        if px < self.w && py < self.h {
                            self.bitmap[(py * self.w + px) as usize] = color;
                        }
                    }
                }
            }
            cx += 8;
        }
    }

    // Ввод символа в окно (как в терминале)
        pub fn put_char(&mut self, c: u8) {
        if c == b'\n' {
            self.cursor_x = 10;
            self.cursor_y += 16;
            if self.cursor_y + 16 < self.h {
                self.fill_rect(10, self.cursor_y, self.w - 20, 16, COLOR_WIN_BG);
            }
        } else if c == 8 { // backspace
            if self.cursor_x > 10 {
                self.cursor_x -= 8;
                self.fill_rect(self.cursor_x, self.cursor_y, 8, 16, COLOR_WIN_BG);
            }
        } else {
            // wrap
            if self.cursor_x + 8 > self.w - 10 {
                self.cursor_x = 10;
                self.cursor_y += 16;
            }

            // 1. затираем ВСЮ ячейку 8×16 BG
            self.fill_rect(self.cursor_x, self.cursor_y, 8, 16, COLOR_WIN_BG);

            // 2. рисуем FG-пиксели
            let bitmap = get_bitmap(c);
            for row in 0..16u32 {
                let bits = bitmap[row as usize];
                for col in 0..8u32 {
                    if (bits >> (7 - col)) & 1 != 0 {
                        let px = self.cursor_x + col;
                        let py = self.cursor_y + row;
                        if px < self.w && py < self.h {
                            self.bitmap[(py * self.w + px) as usize] = 0x00FFFFFF;
                        }
                    }
                }
            }
            self.cursor_x += 8;
        }

        // ЕДИНАЯ точка flush — работает для \n, \b и обычных символов
        unsafe { self.blit_to_back(); }
    }
}

pub struct Wm {
    pub windows: [Window; MAX_WIN],
    pub z_order: [usize; MAX_WIN],
    pub count: usize,
}

pub static mut WM: Wm = Wm {
    windows: [
        Window::new(), Window::new(), Window::new(), Window::new()
    ],
    z_order: [0; MAX_WIN],
    count: 0,
};

pub fn wm() -> &'static mut Wm {
    unsafe { &mut WM }
}

pub unsafe fn init_desktop() {
    let mut cv = back_canvas();
    cv.fill_rect(0, 0, FB_W, FB_H, COLOR_DESKTOP);
    mark_dirty(0, 0, FB_W, FB_H);
}

pub unsafe fn create_window(x: u32, y: u32, w: u32, h: u32, title: &str) -> usize {
    let wm = wm();
    if wm.count >= MAX_WIN { return usize::MAX; }
    
    let id = wm.count;
    let win = &mut wm.windows[id];
    
    win.id = id;
    win.x = x;
    win.y = y;
    win.w = w;
    win.h = h;
    win.set_title(title);
    win.visible = true;
    
    win.draw_chrome();
    win.blit_to_back();
    
    wm.z_order[id] = id;
    wm.count += 1;
    id
}

pub unsafe fn start_drag(id: usize, mx: u32, my: u32) {
    let wm = wm();
    let win = &wm.windows[id];
    DRAG_WIN = Some(id);
    DRAG_OFF_X = mx.saturating_sub(win.x);
    DRAG_OFF_Y = my.saturating_sub(win.y);
}

pub unsafe fn is_dragging() -> bool {
    DRAG_WIN.is_some()
}

pub unsafe fn end_drag() {
    DRAG_WIN = None;
}

pub unsafe fn do_drag(mx: u32, my: u32) {
    if let Some(id) = DRAG_WIN {
        let wm = wm();
        let win = &mut wm.windows[id];
        
        // Вычисляем новые координаты
        let nx = mx.saturating_sub(DRAG_OFF_X);
        let ny = my.saturating_sub(DRAG_OFF_Y);
        
        // Ограничиваем окно пределами экрана
        win.x = nx.min(FB_W.saturating_sub(win.w));
        win.y = ny.min(FB_H.saturating_sub(win.h));
        
        // Перерисовываем всё (пока что full damage, оптимизируем в будущем)
        redraw_all();
    }
}

pub unsafe fn redraw_all() {
    // 1. Прячем курсор, чтобы он не оставил "пустой квадрат"
    crate::gfx::cursor::hide();
    
    init_desktop();
    let wm = wm();
    for i in 0..wm.count {
        let win_idx = wm.z_order[i];
        // ТОЛЬКО блитим. draw_chrome здесь НЕ НУЖЕН (он тяжелый)
        wm.windows[win_idx].blit_to_back();
    }
    
    // 2. Возвращаем курсор на место
    crate::gfx::cursor::show();
}

pub unsafe fn find_at(mx: u32, my: u32) -> Option<usize> {
    let wm = wm();
    // Идем по z_order с конца (сверху вниз)
    for i in (0..wm.count).rev() {
        let win_idx = wm.z_order[i];
        let win = &wm.windows[win_idx];
        if win.visible && mx >= win.x && mx < win.x + win.w && my >= win.y && my < win.y + win.h {
            return Some(win_idx);
        }
    }
    None
}

pub unsafe fn focus(id: usize) {
    let wm = wm();
    if id >= wm.count { return; }
    
    // 1. Снять фокус со всех
    for i in 0..wm.count {
        wm.windows[i].focused = false;
    }
    
    // 2. Обновить z_order
    let mut pos = 0;
    for i in 0..wm.count {
        if wm.z_order[i] == id { pos = i; break; }
    }
    for i in pos..(wm.count - 1) {
        wm.z_order[i] = wm.z_order[i+1];
    }
    wm.z_order[wm.count - 1] = id;
    
    // 3. Поставить фокус топу
    let top = wm.z_order[wm.count - 1];
    wm.windows[top].focused = true;
    
    // 4. Т.к. фокус изменился, цвета рамок поменялись — 
    // нужно перерисовать chrome для ВСЕХ окон.
    for i in 0..wm.count {
        wm.windows[i].draw_chrome();
    }
    
    // 5. Блитим всё на экран (внутри hide/show)
    redraw_all();
}

pub unsafe fn get_focused() -> Option<usize> {
    let wm = wm();
    for i in 0..wm.count {
        if wm.windows[i].focused {
            return Some(i);
        }
    }
    None
}