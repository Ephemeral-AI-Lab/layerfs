//! Observational filesystem capacity; no allocation or disk policy.
use std::path::Path;

/// Bytes available to this caller on the filesystem containing `path`.
/// Uses filesystem allocation units, with checked arithmetic; never writes.
#[cfg(unix)]
pub fn filesystem_available(path: &Path) -> std::io::Result<u64> {
    let filesystem = nix::sys::statvfs::statvfs(path).map_err(std::io::Error::from)?;
    u64::from(filesystem.blocks_available())
        .checked_mul(filesystem.fragment_size() as u64)
        .ok_or_else(|| std::io::Error::other("filesystem available-byte count overflow"))
}

/// Windows: the volume free-space answer waits for the volume-guard port
/// (#454); MAX never refuses for room.
#[cfg(windows)]
pub fn filesystem_available(_path: &Path) -> std::io::Result<u64> {
    Ok(u64::MAX)
}
