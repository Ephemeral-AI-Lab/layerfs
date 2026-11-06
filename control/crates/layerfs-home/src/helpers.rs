//! General helpers every entity uses (moved from `sandbox/registry.rs`,
//! layerfs-project#103): a refused poisoned lock, a required text argument
//! and fresh random bytes.
use crate::outcome::{failed, refused};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::sync::{Mutex, MutexGuard};

/// A lock, refused when a holder panicked.
pub fn lock<T>(mutex: &Mutex<T>) -> Result<MutexGuard<'_, T>, SdkRefusal> {
    mutex
        .lock()
        .map_err(|_| refused("Failed", "binding state poisoned"))
}

/// A required text argument.
pub fn text<'a>(args: &'a SdkValue, name: &str) -> Result<&'a str, SdkRefusal> {
    args.get_text(name)
        .ok_or_else(|| refused("InvalidInput", format!("missing text argument `{name}`")))
}

/// 32 fresh random bytes from the transport's key generator.
pub fn random() -> Result<[u8; 32], SdkRefusal> {
    layerfs_bridge::duplex::generate_keypair()
        .map(|pair| pair.public)
        .map_err(failed)
}
