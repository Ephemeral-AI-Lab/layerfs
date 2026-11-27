//! The per-attempt token behind `set_config('layerfs.attempt', …)` and the
//! token-checked statements of one transaction.
use std::sync::atomic::{AtomicU64, Ordering};

/// A token no other transaction of this process holds: the process, a
/// counter and the clock, hashed. It only tells this transaction from an
/// implicit one; it is not a secret.
pub(crate) fn nonce() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"layerfs/fused-commit/v1\0");
    hasher.update(&std::process::id().to_le_bytes());
    hasher.update(&NEXT.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    hasher.update(&now.to_le_bytes());
    hasher.finalize().to_hex()[..32].to_owned()
}
