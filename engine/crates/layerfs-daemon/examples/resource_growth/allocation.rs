//! External Rust requested-byte accounting; C allocations/RSS are separate.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};
static LIVE: AtomicU64 = AtomicU64::new(0);
static ALLOCATED: AtomicU64 = AtomicU64::new(0);
static FREED: AtomicU64 = AtomicU64::new(0);
pub struct Observed;
fn added(n: usize) {
    LIVE.fetch_add(n as u64, Ordering::SeqCst);
    ALLOCATED.fetch_add(n as u64, Ordering::SeqCst);
}
fn removed(n: usize) {
    LIVE.fetch_sub(n as u64, Ordering::SeqCst);
    FREED.fetch_add(n as u64, Ordering::SeqCst);
}
// SAFETY: the exact caller layout/pointer is forwarded to System. No pointer is
// dereferenced, retained or changed. Failed allocation leaves counts unchanged.
unsafe impl GlobalAlloc for Observed {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            added(layout.size());
        }
        p
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            added(layout.size());
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe {
            System.dealloc(p, layout);
        }
        removed(layout.size());
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let result = unsafe { System.realloc(p, layout, size) };
        if !result.is_null() {
            if size >= layout.size() {
                added(size - layout.size());
            } else {
                removed(layout.size() - size);
            }
        }
        result
    }
}
pub fn snapshot() -> (u64, u64, u64) {
    // These are separately atomic observations, not a transaction or phase peak.
    (
        LIVE.load(Ordering::SeqCst),
        ALLOCATED.load(Ordering::SeqCst),
        FREED.load(Ordering::SeqCst),
    )
}
