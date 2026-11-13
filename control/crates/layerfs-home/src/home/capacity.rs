//! Observational filesystem capacity; no allocation or disk policy.
use std::path::Path;

/// Bytes available to this caller on the filesystem containing `path`.
/// Uses filesystem allocation units, with checked arithmetic; never writes.
pub fn filesystem_available(path: &Path) -> std::io::Result<u64> {
    let filesystem = nix::sys::statvfs::statvfs(path).map_err(std::io::Error::from)?;
    // `fsblkcnt_t` is `u32` on macOS but `u64` on Linux, so `u64::from`
    // is a useless conversion there; widen through `u128` (the same shape
    // every other statvfs consumer uses) and narrow back, checked.
    let available =
        u128::from(filesystem.blocks_available()) * u128::from(filesystem.fragment_size());
    u64::try_from(available)
        .map_err(|_| std::io::Error::other("filesystem available-byte count overflow"))
}
