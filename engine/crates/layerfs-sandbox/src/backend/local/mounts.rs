//! Kernel mounts left under a local Sandbox's directory once its daemon is
//! gone. A container's mounts end with its namespace; a local daemon's stay
//! in this one ("transport endpoint is not connected") until detached. On
//! macOS the table is `mount`'s output and a mount is forced off with
//! `umount -f`.
use std::{
    io,
    path::{Path, PathBuf},
};

/// Mount points at or under `directory`, deepest first, from this
/// process's mount table.
#[cfg(target_os = "macos")]
pub(super) fn under(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let mut found: Vec<PathBuf> = (super::darwin::mount_points()?.into_iter())
        .filter(|point| point.starts_with(directory))
        .collect();
    found.sort_by_key(|point| std::cmp::Reverse(point.components().count()));
    found.dedup();
    Ok(found)
}
#[cfg(target_os = "linux")]
pub(super) fn under(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let table = std::fs::read_to_string("/proc/self/mountinfo")?;
    let mut found = Vec::new();
    for line in table.lines() {
        // field 5 is the mount point, with space, tab, newline and backslash octal-escaped
        let Some(point) = line.split(' ').nth(4) else {
            continue;
        };
        let point = PathBuf::from(unescape(point));
        if point.starts_with(directory) {
            found.push(point);
        }
    }
    found.sort_by_key(|point| std::cmp::Reverse(point.components().count()));
    found.dedup();
    Ok(found)
}

#[cfg(target_os = "linux")]
fn unescape(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let octal = bytes
            .get(i + 1..i + 4)
            // one byte is at most octal 377, so its first digit is 0 to 3
            .filter(|digits| {
                bytes[i] == b'\\'
                    && digits[0] <= b'3'
                    && digits.iter().all(|d| (b'0'..=b'7').contains(d))
            });
        match octal {
            Some(digits) => {
                out.push(digits.iter().fold(0u8, |n, d| n * 8 + (d - b'0')));
                i += 4;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Unmounts every mount under `directory` once with `umount -f` (a dead
/// NFS export's mount is forced off; macOS has no lazy detach), then checks
/// that none is left.
#[cfg(target_os = "macos")]
pub(super) fn detach(directory: &Path) -> io::Result<()> {
    for point in under(directory)? {
        super::darwin::force_unmount(&point)?;
    }
    if under(directory)?.is_empty() {
        Ok(())
    } else {
        Err(io::Error::other("a mount under the local Sandbox remains"))
    }
}
/// Detaches every mount under `directory` once (lazy unmount: a process
/// still inside keeps its view; new lookups no longer reach it), then checks
/// that none is left.
#[cfg(target_os = "linux")]
pub(super) fn detach(directory: &Path) -> io::Result<()> {
    use nix::mount::{umount2, MntFlags};
    for point in under(directory)? {
        match umount2(&point, MntFlags::MNT_DETACH) {
            Ok(()) | Err(nix::errno::Errno::EINVAL) | Err(nix::errno::Errno::ENOENT) => {}
            Err(errno) => return Err(errno.into()),
        }
    }
    if under(directory)?.is_empty() {
        Ok(())
    } else {
        Err(io::Error::other("a mount under the local Sandbox remains"))
    }
}
