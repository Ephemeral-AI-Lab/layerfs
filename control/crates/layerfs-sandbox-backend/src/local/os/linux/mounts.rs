//! Kernel mounts under a folder, from `/proc/self/mountinfo`: what a
//! removal asks before it deletes a Sandbox's folder, and the filesystem a
//! namespaces overlay sits on.
use std::{
    io,
    path::{Path, PathBuf},
};

/// Mount points at or under `directory`, deepest first, from this
/// process's mount table.
pub(crate) fn under(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let table = std::fs::read_to_string("/proc/self/mountinfo")?;
    // the table names real paths; a directory reached through a symlink
    // is matched by its real path too
    let real = directory.canonicalize().ok();
    let mut found = Vec::new();
    for line in table.lines() {
        // field 5 is the mount point, with space, tab, newline and backslash octal-escaped
        let Some(point) = line.split(' ').nth(4) else {
            continue;
        };
        let point = PathBuf::from(unescape(point));
        if point.starts_with(directory) || real.as_deref().is_some_and(|r| point.starts_with(r)) {
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

/// Every mount a `mountinfo` table's text names (this process's, or
/// another process's for its namespace): its id, its mount point and its
/// filesystem type, in table order. Reading a table touches no mount.
pub(crate) fn table(text: &str) -> Vec<(u64, PathBuf, String)> {
    let mut found = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split(' ').collect();
        let Some(separator) = fields.iter().position(|field| *field == "-") else {
            continue;
        };
        let id = fields.first().and_then(|id| id.parse().ok());
        if let (Some(id), Some(point), Some(kind)) = (id, fields.get(4), fields.get(separator + 1))
        {
            found.push((id, PathBuf::from(unescape(point)), (*kind).to_owned()));
        }
    }
    found
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
