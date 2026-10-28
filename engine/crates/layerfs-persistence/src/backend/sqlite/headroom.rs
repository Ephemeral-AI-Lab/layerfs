//! Free-space guard before pack publication (#379).
//!
//! A Save writes its packs in many short transactions. On a filling disk the
//! one that runs out of room is refused, but the earlier ones stay, and with
//! no space left every later write is refused too. Refusing a pack batch
//! while a fixed margin is still free leaves room for small metadata writes
//! and for later cleanup. The check is best effort: other writers share the
//! disk, and a miss still ends in SQLite's own full-disk refusal.
use crate::backend::records::BackendError;
use std::path::Path;

/// Free bytes a pack batch must leave on the Store's filesystem.
pub const STORE_HEADROOM_BYTES: u64 = 64 * 1024 * 1024;

/// Refuses `Capacity`, before any effect, a batch of `bytes` pack bytes that
/// would leave less than [`STORE_HEADROOM_BYTES`] available. The bytes count
/// twice: the WAL holds them until a checkpoint copies them into the file.
/// A Store file whose filesystem cannot be observed is an integrity failure.
pub(crate) fn admit(path: &Path, bytes: u64) -> Result<(), BackendError> {
    let filesystem = nix::sys::statvfs::statvfs(path).map_err(|_| BackendError::Integrity)?;
    let available =
        u128::from(filesystem.blocks_available()) * u128::from(filesystem.fragment_size());
    let needed = u128::from(bytes) * 2 + u128::from(STORE_HEADROOM_BYTES);
    if available < needed {
        return Err(BackendError::Capacity);
    }
    Ok(())
}
