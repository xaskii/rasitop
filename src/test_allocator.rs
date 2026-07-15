use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

struct CountingAllocator;

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        record_allocation(!pointer.is_null());
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        record_allocation(!pointer.is_null());
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let pointer = unsafe { System.realloc(pointer, layout, new_size) };
        record_allocation(!pointer.is_null());
        pointer
    }
}

fn record_allocation(succeeded: bool) {
    if succeeded {
        COUNTING.with(|counting| {
            if counting.get() {
                ALLOCATIONS.with(|allocations| allocations.set(allocations.get() + 1));
            }
        });
    }
}

pub fn count_allocations<T>(operation: impl FnOnce() -> T) -> (T, usize) {
    struct CountingGuard;

    impl Drop for CountingGuard {
        fn drop(&mut self) {
            COUNTING.with(|counting| counting.set(false));
        }
    }

    ALLOCATIONS.with(|allocations| allocations.set(0));
    COUNTING.with(|counting| {
        assert!(
            !counting.replace(true),
            "allocation counter is already active"
        );
    });
    let guard = CountingGuard;
    let value = operation();
    drop(guard);
    let allocations = ALLOCATIONS.with(Cell::get);
    (value, allocations)
}
