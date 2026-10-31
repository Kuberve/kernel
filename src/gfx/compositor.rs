//! Double buffer + damage tracking.
//! FB_BACK в RAM по фиксированному PA 0x48200000 (identity во всех AS через l2[65]).

pub const FB_W: u32 = 640;
pub const FB_H: u32 = 480;
pub const FB_VA: u64 = 0x48000000;
pub const FB_BACK_VA: u64 = 0x48200000;

pub struct Canvas {
    pub ptr: *mut u32,
    pub stride: u32,
    pub w: u32,
    pub h: u32,
}

impl Canvas {
    pub unsafe fn put_pixel(&mut self, x: u32, y: u32, color: u32) {
        if x >= self.w || y >= self.h { return; }
        core::ptr::write_volatile(self.ptr.add((y * self.stride + x) as usize), color);
    }

    pub unsafe fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: u32) {
        if w == 0 || h == 0 { return; }
        let x_end = core::cmp::min(x.saturating_add(w), self.w);
        let y_end = core::cmp::min(y.saturating_add(h), self.h);
        if x >= self.w || y >= self.h || x_end <= x || y_end <= y { return; }
        for yy in y..y_end {
            let row = (yy * self.stride) as usize;
            let ptr = self.ptr;
            for xx in x..x_end {
                core::ptr::write_volatile(ptr.add(row + xx as usize), color);
            }
        }
    }
}

pub fn back_canvas() -> Canvas {
    Canvas { ptr: FB_BACK_VA as *mut u32, stride: FB_W, w: FB_W, h: FB_H }
}

pub fn front_canvas() -> Canvas {
    Canvas { ptr: FB_VA as *mut u32, stride: FB_W, w: FB_W, h: FB_H }
}

pub struct Rect {
    pub x0: u32, pub y0: u32, pub x1: u32, pub y1: u32,
}

impl Rect {
    pub const EMPTY: Rect = Rect { x0: u32::MAX, y0: u32::MAX, x1: 0, y1: 0 };
    pub fn is_empty(&self) -> bool { self.x1 <= self.x0 || self.y1 <= self.y0 }
}

static mut DAMAGE: Rect = Rect::EMPTY;

pub unsafe fn mark_dirty(x: u32, y: u32, w: u32, h: u32) {
    if w == 0 || h == 0 { return; }
    let x1 = x.saturating_add(w);
    let y1 = y.saturating_add(h);
    if DAMAGE.is_empty() {
        DAMAGE = Rect { x0: x, y0: y, x1, y1 };
    } else {
        if x < DAMAGE.x0 { DAMAGE.x0 = x; }
        if y < DAMAGE.y0 { DAMAGE.y0 = y; }
        if x1 > DAMAGE.x1 { DAMAGE.x1 = x1; }
        if y1 > DAMAGE.y1 { DAMAGE.y1 = y1; }
    }
}

/// Копирует damage-регион FB_BACK → FB, flush в virtio-gpu.
pub unsafe fn present_damage() {
    if DAMAGE.is_empty() { return; }
    let x = DAMAGE.x0;
    let y = DAMAGE.y0;
    let w = DAMAGE.x1 - x;
    let h = DAMAGE.y1 - y;
    DAMAGE = Rect::EMPTY;

    // Disable IRQ на копирование (1.2 MB ~ 5-10 мс) — не хотим прерывания
    // посередине, иначе FB остаётся полу-обновлённым.
    core::arch::asm!("msr daifset, #2");

    let src = FB_BACK_VA as *const u32;
    let dst = FB_VA as *mut u32;

    if x == 0 && w == FB_W {
        let offset = (y * FB_W) as usize;
        let n = (FB_W * h) as usize;
        if offset % 2 == 0 && n % 2 == 0 {
            let src64 = src.add(offset) as *const u64;
            let dst64 = dst.add(offset) as *mut u64;
            core::ptr::copy_nonoverlapping(src64, dst64, n / 2);
        } else {
            core::ptr::copy_nonoverlapping(src.add(offset), dst.add(offset), n);
        }
    } else {
        for yy in y..(y + h) {
            let row = (yy * FB_W) as usize;
            let n = w as usize;
            core::ptr::copy_nonoverlapping(
                src.add(row + x as usize),
                dst.add(row + x as usize),
                n,
            );
        }
    }

    core::arch::asm!("dsb sy");
    core::arch::asm!("msr daifclr, #2");

    if let Some(gpu) = crate::drivers::virtio::gpu::get_global_mut() {
        crate::drivers::virtio::gpu::flush_region(gpu, x, y, w, h);
    }
}

pub unsafe fn present_full() {
    mark_dirty(0, 0, FB_W, FB_H);
    present_damage();
}