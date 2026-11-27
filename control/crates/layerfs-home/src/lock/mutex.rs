//! The poison-tolerant lock of a process table (layerfs-project#177).
use layerfs_protocol::sdk::SdkRefusal;
use std::sync::{Mutex, MutexGuard};

/// The lock of a table each holder changes in one step (one insert, remove
/// or lookup of a map or set), so the table is whole whenever a holder
/// panics: its guard is taken back from a panicked holder, and a panic in
/// one call never refuses the calls after it (layerfs-project#177). State a
/// panic can leave half-changed takes its own lock, not this one. It never
/// refuses; the result keeps its callers' shape.
pub fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, SdkRefusal> {
    Ok(mutex.lock().unwrap_or_else(|poison| poison.into_inner()))
}
