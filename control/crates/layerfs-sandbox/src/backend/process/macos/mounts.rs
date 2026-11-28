//! Kernel mounts left under a process Sandbox's directory once its daemon is
//! gone. A container's mounts end with its namespace; this daemon's stay in
//! the one kernel ("transport endpoint is not connected") until detached. The
//! table is `mount`'s output and a mount is forced off with `umount -f`.
use std::{
    io,
    path::{Path, PathBuf},
};

/// Mount points at or under `directory`, deepest first, from this
/// process's mount table. The table names real paths (`/private/var/...`
/// for `/var/...`), so the directory's real path is matched too.
///
/// A Sandbox directory whose `workspaces` is a link (#846: its daemon mounts
/// in the home's `sandbox/<name>/workspaces`) has its mounts under the
/// link's target too, which are listed with it.
pub(crate) fn under(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let mut places = vec![directory.to_owned()];
    places.extend(directory.canonicalize().ok());
    if let Some(home) = linked_home(directory) {
        places.extend(home.canonicalize().ok());
        places.push(home);
    }
    let mut found: Vec<PathBuf> = (super::mount_points()?.into_iter())
        .filter(|point| places.iter().any(|place| point.starts_with(place)))
        .collect();
    found.sort_by_key(|point| std::cmp::Reverse(point.components().count()));
    found.dedup();
    Ok(found)
}

/// Unmounts every mount under `directory` once with `umount -f` (a dead
/// NFS export's mount is forced off; macOS has no lazy detach), then checks
/// that none is left.
///
/// A forced unmount leaves its folder, which the daemon would have removed:
/// in a linked mount home (#846) each empty folder is removed once
/// (`rmdir`, which never enters a mount), so a later mount of that name
/// finds its point free.
pub(crate) fn detach(directory: &Path) -> io::Result<()> {
    for point in under(directory)? {
        super::force_unmount(&point)?;
    }
    if !under(directory)?.is_empty() {
        return Err(io::Error::other("a mount under the local Sandbox remains"));
    }
    if let Some(home) = linked_home(directory) {
        if let Ok(entries) = std::fs::read_dir(&home) {
            for entry in entries.flatten() {
                drop(std::fs::remove_dir(entry.path()));
            }
        }
    }
    Ok(())
}

/// The target of `directory`'s `workspaces` when it is a link: the mount
/// home its daemon was given (#846). Read, never followed further.
fn linked_home(directory: &Path) -> Option<PathBuf> {
    std::fs::read_link(directory.join("workspaces"))
        .ok()
        .filter(|home| home.is_absolute())
}
