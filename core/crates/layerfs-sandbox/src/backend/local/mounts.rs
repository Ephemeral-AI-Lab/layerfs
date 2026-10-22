//! Kernel mounts left under a local Sandbox's directory once its daemon is
//! gone. A container's mounts end with its namespace; a local daemon's stay
//! in this one ("transport endpoint is not connected") until detached.
use std::{
    io,
    path::{Path, PathBuf},
};

/// Mount points at or under `directory`, deepest first, from this
/// process's mount table.
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

/// Detaches every mount under `directory` once (lazy unmount: a process
/// still inside keeps its view; new lookups no longer reach it), then checks
/// that none is left.
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
