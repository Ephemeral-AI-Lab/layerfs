//! Fixed saturating loss counters with explicit overflow state.
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
pub(crate) struct Counter {
    value: AtomicU64,
    overflow: AtomicBool,
}
impl Counter {
    pub(crate) const fn new() -> Self {
        Self {
            value: AtomicU64::new(0),
            overflow: AtomicBool::new(false),
        }
    }
    pub(crate) fn add(&self, n: u64) {
        let old = self
            .value
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                Some(v.saturating_add(n))
            })
            .unwrap_or(u64::MAX);
        if old.checked_add(n).is_none() {
            self.overflow.store(true, Ordering::Relaxed);
        }
    }
    pub(crate) fn value(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }
    pub(crate) fn overflowed(&self) -> bool {
        self.overflow.load(Ordering::Relaxed)
    }
}
