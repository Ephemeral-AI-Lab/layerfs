//! Kernel mounts left under a native Sandbox's directory once its daemon is
//! gone. A container's mounts end with its namespace; this daemon's stay in
//! the one kernel until detached. The table is `/proc/self/mountinfo` and a
//! mount is detached lazily with `umount2(MNT_DETACH)`.
use std::{
    io,
    path::{Path, PathBuf},
};

/// Mount points at or under `directory`, deepest first, from this
/// process's mount table.
pub(crate) fn under(directory: &Path) -> io::Result<Vec<PathBuf>> {
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

/// The filesystem type holding `path`: the deepest mount point in this
/// process's mount table that is one of its ancestors, and the type recorded
/// after the separator. A namespaces Sandbox's root is an overlay whose
/// writable layer must live on a filesystem that can be an overlay upper.
pub(crate) fn filesystem(path: &Path) -> io::Result<String> {
    let table = std::fs::read_to_string("/proc/self/mountinfo")?;
    let mut deepest: Option<(usize, String)> = None;
    for line in table.lines() {
        let fields: Vec<&str> = line.split(' ').collect();
        let Some(separator) = fields.iter().position(|field| *field == "-") else {
            continue;
        };
        let (Some(point), Some(kind)) = (fields.get(4), fields.get(separator + 1)) else {
            continue;
        };
        let point = PathBuf::from(unescape(point));
        if !path.starts_with(&point) {
            continue;
        }
        let depth = point.components().count();
        if deepest
            .as_ref()
            .is_none_or(|(deepest, _)| depth >= *deepest)
        {
            deepest = Some((depth, (*kind).to_owned()));
        }
    }
    deepest.map(|(_, kind)| kind).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} is not under any mount of this process", path.display()),
        )
    })
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
pub(crate) fn detach(directory: &Path) -> io::Result<()> {
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
