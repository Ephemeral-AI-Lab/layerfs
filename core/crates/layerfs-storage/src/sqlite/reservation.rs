//! Bounded physical headroom before SQLite grows a new pack row.
//!
//! The reservation changes allocated blocks, never logical file length or
//! database bytes. It is inside the save's write transaction and paid by the
//! caller's timed phase. A failed reservation aborts the save; no fallback
//! silently substitutes a different physical profile.

use rusqlite::Connection;

use crate::error::{StorageError, StorageResult};

const MIB: u64 = 1 << 20;
const HEADROOM: u64 = 2 * MIB;
const MAX_REQUEST: u64 = crate::policy::SINGLETON_PACK_LIMIT as u64 + 3 * MIB;

/// Reserves only the extent needed for the next bounded pack plus 2 MiB of
/// following SQLite pages. The upper bound is independent of Store size.
pub(crate) fn before_pack_insert(
    connection: &Connection,
    pack_capacity: usize,
) -> StorageResult<()> {
    if pack_capacity > crate::policy::SINGLETON_PACK_LIMIT {
        return Err(StorageError::Integrity("pack reservation capacity"));
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = connection;
        Err(StorageError::UnsupportedPolicy {
            field: "physical reservation platform",
        })
    }
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        reserve_unix(connection, pack_capacity as u64)
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn reserve_unix(connection: &Connection, pack_capacity: u64) -> StorageResult<()> {
    use std::fs::OpenOptions;
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::Path;

    let filename = connection
        .path()
        .filter(|path| !path.is_empty())
        .ok_or(StorageError::Integrity("Store file path for reservation"))?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(Path::new(filename))
        .map_err(StorageError::Io)?;
    let metadata = file.metadata().map_err(StorageError::Io)?;
    let apparent = metadata.len();
    let wanted = apparent
        .checked_add(pack_capacity)
        .and_then(|value| value.checked_add(HEADROOM))
        .and_then(|value| value.checked_add(MIB - 1))
        .map(|value| value / MIB * MIB)
        .ok_or(StorageError::Integrity("Store reservation overflow"))?;
    super::native_reservation::reserve_to(
        &file,
        &metadata,
        wanted,
        MAX_REQUEST,
        "one Store physical reservation",
    )
}
