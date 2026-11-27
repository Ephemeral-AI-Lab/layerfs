//! One-line JSON logs in the home's `log/` folder: [`log_line`] appends a
//! line and rotates the file at its size bound, and [`log_swept`] records
//! what a sweep removed.
use crate::json;
use crate::record::file::{now_ms, private_dir, to_hex};
use crate::reply::outcome::failed;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

/// `log/uncertain.jsonl`: the only record of uncertain outcomes, so it is
/// never rotated (layerfs-project#179 D8).
pub const UNCERTAIN: &str = "uncertain.jsonl";

/// Appends one JSON line to `log/<name>`: the file is opened, written and
/// closed, so no writer holds it across a rotation. Past 16 MiB it then
/// rotates once to `log/<stem>.1.jsonl`, as the call log does, under a
/// non-blocking `flock` on `log/rotate.lock`; a writer that does not get
/// the lock skips the rotation, and a rotation that fails changes no
/// outcome. [`UNCERTAIN`] never rotates.
pub fn log_line(home: &Path, name: &str, value: &SdkValue) -> Result<(), SdkRefusal> {
    let dir = crate::disk::paths::log_dir(home);
    private_dir(&dir)?;
    let path = dir.join(name);
    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .open(&path)
        .map_err(failed)?;
    let mut text = json::print(&to_hex(value.clone()));
    text.push('\n');
    file.write_all(text.as_bytes()).map_err(failed)?;
    let size = file.metadata().map_or(0, |meta| meta.len());
    drop(file);
    if size > crate::log::calls::ROTATE_AT && name != UNCERTAIN {
        crate::log::calls::rotate(&crate::disk::paths::log_rotate_lock(home), &path);
    }
    Ok(())
}

/// `log/sweep.jsonl`: what LayerFS removed of what interrupted operations
/// left behind, and each removal that failed (spec v20).
pub const SWEPT: &str = "sweep.jsonl";

/// One line of [`SWEPT`] under `root` (a home, or a worker's root): `{ at,
/// kind, path, outcome: removed | failed, bytes?, message? }`, `outcome`
/// `removed` with the bytes freed when they were counted, else `failed`
/// with why. A line that cannot be written changes nothing.
pub fn log_swept(root: &Path, kind: &str, path: &Path, outcome: &Result<Option<u64>, String>) {
    let mut fields = vec![
        ("at", SdkValue::Int(now_ms())),
        ("kind", SdkValue::text(kind)),
        ("path", SdkValue::text(path.display().to_string())),
    ];
    match outcome {
        Ok(bytes) => {
            fields.push(("outcome", SdkValue::text("removed")));
            if let Some(bytes) = bytes.and_then(|bytes| i64::try_from(bytes).ok()) {
                fields.push(("bytes", SdkValue::Int(bytes)));
            }
        }
        Err(why) => {
            fields.push(("outcome", SdkValue::text("failed")));
            fields.push(("message", SdkValue::text(why)));
        }
    }
    let line = fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
    let _ = log_line(root, SWEPT, &SdkValue::Map(line));
}
