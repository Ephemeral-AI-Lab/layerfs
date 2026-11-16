//! General helpers every entity uses (moved from `sandbox/registry.rs`,
//! layerfs-project#103): the lock of a process table, a required text
//! argument and fresh random bytes.
use crate::outcome::{failed, refused};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
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

/// A required text field `name` of a map (`value`, a record's or a frame's).
pub fn text<'a>(value: &'a SdkValue, name: &str) -> Result<&'a str, SdkRefusal> {
    value
        .get_text(name)
        .ok_or_else(|| refused("InvalidInput", format!("missing text argument `{name}`")))
}

/// A required text field `name` of a typed request or record: refused as
/// [`text`] refuses a missing argument.
pub fn required<'a>(value: Option<&'a str>, name: &str) -> Result<&'a str, SdkRefusal> {
    value.ok_or_else(|| refused("InvalidInput", format!("missing text argument `{name}`")))
}

/// 32 fresh random bytes from the transport's key generator.
pub fn random() -> Result<[u8; 32], SdkRefusal> {
    layerfs_bridge::duplex::generate_keypair()
        .map(|pair| pair.public)
        .map_err(failed)
}
