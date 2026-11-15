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

/// The filesystem containing a path, as `statvfs` and `statfs` report it
/// (layerfs-project#164): sizes in bytes, the inodes this caller may still
/// make and the filesystem's type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Filesystem {
    /// Its size.
    pub total: u64,
    /// Bytes in use: its size less every free block, those only root may
    /// use included, so `used + free` can be less than `total`.
    pub used: u64,
    /// Bytes available to this caller ([`filesystem_available`]).
    pub free: u64,
    /// Inodes available to this caller; `None` where the filesystem counts
    /// none (btrfs).
    pub inodes_free: Option<u64>,
    /// Its type: the name macOS gives (`apfs`), or on Linux the name of its
    /// magic number for the types below, else the number (`0x...`).
    pub kind: String,
}

/// The filesystem containing `path`, read with two system calls; never
/// writes and walks nothing.
pub fn filesystem(path: &Path) -> std::io::Result<Filesystem> {
    let found = nix::sys::statvfs::statvfs(path).map_err(std::io::Error::from)?;
    let unit = u128::from(found.fragment_size());
    let bytes = |blocks: u128| {
        u64::try_from(blocks * unit)
            .map_err(|_| std::io::Error::other("filesystem byte count overflow"))
    };
    let (blocks, free) = (u128::from(found.blocks()), u128::from(found.blocks_free()));
    let inodes = u128::from(found.files());
    Ok(Filesystem {
        total: bytes(blocks)?,
        used: bytes(blocks.saturating_sub(free))?,
        free: bytes(u128::from(found.blocks_available()))?,
        inodes_free: (inodes > 0)
            .then(|| u64::try_from(u128::from(found.files_available())).ok())
            .flatten(),
        kind: kind(path)?,
    })
}

/// The type of the filesystem containing `path`.
#[cfg(target_os = "linux")]
fn kind(path: &Path) -> std::io::Result<String> {
    use nix::sys::statfs as fs;
    let magic = fs::statfs(path)
        .map_err(std::io::Error::from)?
        .filesystem_type();
    let named = [
        (fs::EXT4_SUPER_MAGIC, "ext4"),
        (fs::XFS_SUPER_MAGIC, "xfs"),
        (fs::BTRFS_SUPER_MAGIC, "btrfs"),
        (fs::OVERLAYFS_SUPER_MAGIC, "overlay"),
        (fs::TMPFS_MAGIC, "tmpfs"),
        (fs::NFS_SUPER_MAGIC, "nfs"),
        (fs::FUSE_SUPER_MAGIC, "fuse"),
    ];
    Ok(named.iter().find(|(known, _)| *known == magic).map_or_else(
        || format!("0x{:x}", magic.0),
        |(_, name)| (*name).to_owned(),
    ))
}

/// The type of the filesystem containing `path`.
#[cfg(not(target_os = "linux"))]
fn kind(path: &Path) -> std::io::Result<String> {
    let found = nix::sys::statfs::statfs(path).map_err(std::io::Error::from)?;
    Ok(found.filesystem_type_name().to_owned())
}
