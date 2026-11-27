//! Log rotation, one rule for every log LayerFS writes
//! (layerfs-project#180): past [`ROTATE_AT`] a log moves once to
//! `<name>.1.<extension>`, replacing an older one, so a log and its
//! rotated file together stay near twice that size.
//!
//! Two ways, by who holds the file. [`rotate`] renames it: for a log no
//! process holds open, either because every writer opens, appends one line
//! and closes (`log/*.jsonl`, `trace/calls.jsonl`), or because its one
//! writer has not started yet. [`rotate_in_place`] copies it and then
//! empties it: for a log other processes may still hold open for appending
//! (a machine's `forward.log`, shared by its forwards), whose descriptors
//! a rename would leave writing into the rotated file. Neither waits,
//! locks or retries: a caller that shares a log with other rotators takes
//! its own lock first.
use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

/// The size past which a log rotates: 16 MiB.
pub const ROTATE_AT: u64 = 16 << 20;

/// Where `path` rotates to: `worker.log` to `worker.1.log`,
/// `leases.jsonl` to `leases.1.jsonl`, a name with no extension to
/// `<name>.1`.
pub fn rotated(path: &Path) -> PathBuf {
    let mut name = path.file_stem().unwrap_or_default().to_os_string();
    name.push(".1");
    if let Some(extension) = path.extension() {
        name.push(".");
        name.push(extension);
    }
    path.with_file_name(name)
}

/// Whether `path` is a file larger than `limit`; an absent one is not.
fn past(path: &Path, limit: u64) -> io::Result<bool> {
    match fs::metadata(path) {
        Ok(found) => Ok(found.len() > limit),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

/// Renames `path` to [`rotated`] when it is larger than `limit`, replacing
/// an older rotated file; answers whether it did. Only for a log no
/// process holds open.
pub fn rotate(path: &Path, limit: u64) -> io::Result<bool> {
    if !past(path, limit)? {
        return Ok(false);
    }
    fs::rename(path, rotated(path))?;
    Ok(true)
}

/// Copies `path` to [`rotated`] when it is larger than `limit`, replacing
/// an older rotated file, then empties `path` in place; answers whether it
/// did. The file is never renamed, so a process that holds it open for
/// appending keeps writing into it, at its new end. A line such a process
/// writes between the copy and the emptying is lost.
pub fn rotate_in_place(path: &Path, limit: u64) -> io::Result<bool> {
    if !past(path, limit)? {
        return Ok(false);
    }
    fs::copy(path, rotated(path))?;
    OpenOptions::new().write(true).open(path)?.set_len(0)?;
    Ok(true)
}
