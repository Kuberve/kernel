use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicUsize, Ordering};
use core::ptr;

extern "C" {
    static __heap_start: u8;
    static __heap_end: u8;
}

pub struct BumpAlloc;

static NEXT: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for BumpAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let start = &__heap_start as *const _ as usize;
        let end = &__heap_end as *const _ as usize;

        let mut next = NEXT.load(Ordering::Relaxed);
        if next == 0 { next = start; }

        let align = layout.align().max(8);
        let aligned = (next + align - 1) & !(align - 1);
        let new_next = aligned.checked_add(layout.size()).unwrap_or(usize::MAX);

        if new_next > end {
            return ptr::null_mut();
        }

        NEXT.store(new_next, Ordering::Relaxed);
        aligned as *mut u8
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // no-op для milestone
    }
}

#[global_allocator]
pub static GLOBAL: BumpAlloc = BumpAlloc;

#[alloc_error_handler]
fn alloc_error(layout: Layout) -> ! {
    panic!("alloc error: size={} align={}", layout.size(), layout.align());
}