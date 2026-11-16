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
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let filesystem = nix::sys::statvfs::statvfs(path).map_err(|_| BackendError::Integrity)?;
        let available =
            u128::from(filesystem.blocks_available()) * u128::from(filesystem.fragment_size());
        let needed = u128::from(bytes) * 2 + u128::from(STORE_HEADROOM_BYTES);
        if available < needed {
            return Err(BackendError::Capacity);
        }
    }
    // Windows: the free-space gate needs the volume-guard port (#454); until
    // then admission is granted and a filling disk ends in SQLite's own
    // full-disk refusal, the same best-effort end state the module documents.
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = (path, bytes);
    }
    Ok(())
}
