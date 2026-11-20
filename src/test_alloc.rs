//! Tests only: the memory a piece of code holds at most, counted by thread. The memory of the
//! whole process is shared with the tests that run beside, so it cannot tell.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    /// Bytes this thread has asked for and not given back (less, or below 0, when it frees what
    /// another thread made), and the most of them since `peak` began.
    static NOW: Cell<isize> = const { Cell::new(0) };
    static MOST: Cell<isize> = const { Cell::new(0) };
}

fn add(bytes: isize) {
    // `try_with`: nothing is counted while the thread's own values are being taken down.
    let _ = NOW.try_with(|now| {
        let v = now.get().wrapping_add(bytes);
        now.set(v);
        let _ = MOST.try_with(|most| most.set(most.get().max(v)));
    });
}

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            add(layout.size() as isize);
        }
        p
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            add(layout.size() as isize);
        }
        p
    }

    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) };
        add(-(layout.size() as isize));
    }

    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, layout, size) };
        if !q.is_null() {
            add(size as isize - layout.size() as isize);
        }
        q
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

/// What `f` returns, and the most memory it held at once on this thread, in bytes.
pub fn peak<T>(f: impl FnOnce() -> T) -> (T, usize) {
    let start = NOW.with(Cell::get);
    MOST.with(|most| most.set(start));
    let out = f();
    (out, MOST.with(Cell::get).saturating_sub(start).max(0) as usize)
}
