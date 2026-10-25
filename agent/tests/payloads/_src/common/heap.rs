// Bump allocator for payloads that use `alloc`. Include with:
//   #[path = "../../common/heap.rs"] mod heap_impl;
//   #[global_allocator] static A: heap_impl::heap::Bump = heap_impl::heap::Bump::new();
/// Simple bump allocator on top of memory.grow (never frees except via reset()).
pub mod heap {
    use core::alloc::{GlobalAlloc, Layout};
    use core::cell::UnsafeCell;

    pub struct Bump {
        next: UnsafeCell<usize>,
        end: UnsafeCell<usize>,
    }
    unsafe impl Sync for Bump {}

    impl Bump {
        pub const fn new() -> Self { Bump { next: UnsafeCell::new(0), end: UnsafeCell::new(0) } }
    }

    unsafe impl GlobalAlloc for Bump {
        unsafe fn alloc(&self, l: Layout) -> *mut u8 {
            let next = &mut *self.next.get();
            let end = &mut *self.end.get();
            let start = (*next + l.align() - 1) & !(l.align() - 1);
            let new_next = start + l.size();
            if *next == 0 || new_next > *end {
                // Grow by enough pages for this allocation (+1 page slack).
                let pages = (l.size() + l.align()) / 65536 + 1;
                let old = core::arch::wasm32::memory_grow(0, pages);
                if old == usize::MAX {
                    return core::ptr::null_mut();
                }
                let base = old * 65536;
                if *next == 0 || base != *end {
                    *next = base;
                }
                *end = base + pages * 65536;
                return self.alloc(l);
            }
            *next = new_next;
            start as *mut u8
        }
        unsafe fn dealloc(&self, _p: *mut u8, _l: Layout) {}
    }
}
