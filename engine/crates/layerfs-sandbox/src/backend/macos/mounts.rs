//! Kernel mounts left under a native Sandbox's directory once its daemon is
//! gone. A container's mounts end with its namespace; this daemon's stay in
//! the one kernel ("transport endpoint is not connected") until detached. The
//! table is `mount`'s output and a mount is forced off with `umount -f`.
use std::{
    io,
    path::{Path, PathBuf},
};

/// Mount points at or under `directory`, deepest first, from this
/// process's mount table.
pub(crate) fn under(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let mut found: Vec<PathBuf> = (super::mount_points()?.into_iter())
        .filter(|point| point.starts_with(directory))
        .collect();
    found.sort_by_key(|point| std::cmp::Reverse(point.components().count()));
    found.dedup();
    Ok(found)
}

/// Unmounts every mount under `directory` once with `umount -f` (a dead
/// NFS export's mount is forced off; macOS has no lazy detach), then checks
/// that none is left.
pub(crate) fn detach(directory: &Path) -> io::Result<()> {
    for point in under(directory)? {
        super::force_unmount(&point)?;
    }
    if under(directory)?.is_empty() {
        Ok(())
    } else {
        Err(io::Error::other("a mount under the local Sandbox remains"))
    }
}
