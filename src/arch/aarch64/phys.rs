pub const PAGE_SIZE: usize = 4096;
const BITMAP_WORDS: usize = 8192;

// Bitmap: 1 = used, 0 = free. 64 * 8192 = 524288 страниц = 2GB. С запасом.
static mut BITMAP: [u64; BITMAP_WORDS] = [0u64; BITMAP_WORDS];
static mut PAGE_START: usize = 0;
static mut PAGE_COUNT: usize = 0;

pub unsafe fn init(ram_start: usize, ram_end: usize) {
    let start = (ram_start + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
    let end = ram_end & !(PAGE_SIZE - 1);
    let count = (end - start) / PAGE_SIZE;
    if count > 8192 * 64 {
        // обрезаем, если RAM больше bitmap
        // (для QEMU 128MB — не случится)
    }

    PAGE_START = start;
    PAGE_COUNT = count;

    // Все помечаем used
    for i in 0..8192 {
        BITMAP[i] = !0u64;
    }
    // Наш диапазон — free
    for i in 0..count {
        let idx = i / 64;
        let bit = i % 64;
        BITMAP[idx] &= !(1u64 << bit);
    }
}

pub unsafe fn alloc_page() -> Option<usize> {
    for i in 0..PAGE_COUNT {
        let idx = i / 64;
        let bit = i % 64;
        if BITMAP[idx] & (1u64 << bit) == 0 {
            BITMAP[idx] |= 1u64 << bit;
            return Some(PAGE_START + i * PAGE_SIZE);
        }
    }
    None
}

pub unsafe fn free_page(addr: usize) {
    if addr < PAGE_START { return; }
    let i = (addr - PAGE_START) / PAGE_SIZE;
    if i >= PAGE_COUNT { return; }
    let idx = i / 64;
    let bit = i % 64;
    BITMAP[idx] &= !(1u64 << bit);
}