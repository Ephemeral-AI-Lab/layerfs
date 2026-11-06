//! The worker's side of a folder copy for `project.init` (#536 H11a): the
//! records a caller sends ([`super::tree`]) written into a staging folder
//! `<root>/incoming/init-<id>`, which the worker's Init then reads as its
//! source, and removed on every outcome the worker knows. Paths come from
//! the root and the copy's id only; an entry's path stays inside its
//! staging folder and goes through no link.
use super::tree::{Reader, Record};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::{failed, refused, Outcome};
use nix::sys::stat::{utimensat, UtimensatFlags};
use nix::sys::time::TimeSpec;
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The copy's id: 16 lowercase hex digits the caller chose.
pub fn id(args: &SdkValue) -> Result<&str> {
    let id = args.get_text("staged").unwrap_or("");
    let hex = |c: u8| c.is_ascii_digit() || (b'a'..=b'f').contains(&c);
    if id.len() != 16 || !id.bytes().all(hex) {
        return Err(refused(
            "InvalidInput",
            "`staged` must be a folder copy's id: 16 lowercase hex digits",
        ));
    }
    Ok(id)
}

/// Where copy `id` is staged under the worker's `root`.
pub fn folder(root: &Path, id: &str) -> PathBuf {
    root.join("incoming").join(format!("init-{id}"))
}

/// `project.stage` `{ staged, step, part }`: `write` applies one part of
/// the copy, `discard` removes the staging folder.
pub fn run(root: &Path, args: &SdkValue) -> Outcome {
    let dir = folder(root, id(args)?);
    match args.get_text("step").unwrap_or("") {
        "write" => {
            let Some(SdkValue::Bytes(part)) = args.get("part") else {
                return Err(refused("InvalidInput", "`part` must be bytes"));
            };
            layerfs_home::home::private_dir(&dir)?;
            let records = apply(&dir, part)?;
            Ok(SdkValue::map([("records", SdkValue::Int(records as i64))]))
        }
        "discard" => {
            let existed = dir.exists();
            remove(&dir)?;
            Ok(SdkValue::map([("discarded", SdkValue::Bool(existed))]))
        }
        step => Err(refused(
            "InvalidInput",
            format!("no folder copy step `{step}`"),
        )),
    }
}

/// Writes every record of `part` under `dir`; answers how many.
fn apply(dir: &Path, part: &[u8]) -> Result<u64> {
    let mut reader = Reader::new(part);
    let mut count = 0;
    while let Some(record) = reader.next_record()? {
        one(dir, record).map_err(|error| refused("Failed", format!("folder copy: {error}")))?;
        count += 1;
    }
    Ok(count)
}

fn one(dir: &Path, record: Record<'_>) -> std::result::Result<(), String> {
    match record {
        Record::Directory { path } => {
            let at = place(dir, path, false)?;
            fs::DirBuilder::new()
                .mode(0o700)
                .create(&at)
                .map_err(|e| format!("{}: {e}", at.display()))
        }
        Record::Write {
            path,
            offset,
            bytes,
        } => {
            let at = place(dir, path, false)?;
            let shown = |e: std::io::Error| format!("{}: {e}", at.display());
            let mut options = OpenOptions::new();
            options.write(true).custom_flags(libc::O_NOFOLLOW);
            if offset == 0 {
                options.create_new(true).mode(0o600);
            }
            let mut file = options.open(&at).map_err(shown)?;
            if file.metadata().map_err(shown)?.len() != offset {
                return Err(format!(
                    "{}: a piece at {offset} does not follow the file's length",
                    at.display()
                ));
            }
            file.seek(SeekFrom::Start(offset)).map_err(shown)?;
            file.write_all(bytes).map_err(shown)
        }
        Record::Link { path, target } => {
            let at = place(dir, path, false)?;
            std::os::unix::fs::symlink(OsStr::from_bytes(target), &at)
                .map_err(|e| format!("{}: {e}", at.display()))
        }
        Record::Meta {
            path,
            mode,
            seconds,
            nanoseconds,
        } => {
            let at = place(dir, path, true)?;
            let shown = |e: &dyn std::fmt::Display| format!("{}: {e}", at.display());
            let meta = fs::symlink_metadata(&at).map_err(|e| shown(&e))?;
            if !meta.file_type().is_symlink() {
                fs::set_permissions(&at, fs::Permissions::from_mode(mode & 0o7777))
                    .map_err(|e| shown(&e))?;
            }
            let mtime = TimeSpec::new(seconds, i64::from(nanoseconds) as _);
            utimensat(
                nix::fcntl::AT_FDCWD,
                &at,
                &TimeSpec::UTIME_OMIT,
                &mtime,
                UtimensatFlags::NoFollowSymlink,
            )
            .map_err(|e| shown(&e))
        }
    }
}

/// `path` under `dir`: plain names only (no `.`, `..` or empty one), each
/// folder on the way a real directory, not a link. The empty path is `dir`
/// itself, only where `root` allows it.
fn place(dir: &Path, path: &[u8], root: bool) -> std::result::Result<PathBuf, String> {
    if path.is_empty() {
        return match root {
            true => Ok(dir.to_owned()),
            false => Err("an entry with no name".to_owned()),
        };
    }
    let mut at = dir.to_owned();
    let mut names = path.split(|&b| b == b'/').peekable();
    while let Some(name) = names.next() {
        if name.is_empty() || name == b"." || name == b".." || name.contains(&0) {
            return Err(format!(
                "an entry path {} is not plain",
                String::from_utf8_lossy(path)
            ));
        }
        at.push(OsStr::from_bytes(name));
        if names.peek().is_some() {
            let meta = fs::symlink_metadata(&at).map_err(|e| format!("{}: {e}", at.display()))?;
            if !meta.is_dir() || meta.file_type().is_symlink() {
                return Err(format!("{} is not a directory", at.display()));
            }
        }
    }
    Ok(at)
}

/// Removes staging folder `dir` and all it holds, first letting this
/// process into each directory a copied mode closed. Absent is done.
pub fn remove(dir: &Path) -> Result<()> {
    let mut pending = vec![dir.to_owned()];
    while let Some(at) = pending.pop() {
        let meta = match fs::symlink_metadata(&at) {
            Ok(meta) => meta,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(refused("Failed", format!("{}: {e}", at.display()))),
        };
        if !meta.is_dir() || meta.file_type().is_symlink() {
            continue;
        }
        if meta.permissions().mode() & 0o700 != 0o700 {
            fs::set_permissions(&at, fs::Permissions::from_mode(0o700))
                .map_err(|e| refused("Failed", format!("{}: {e}", at.display())))?;
        }
        for entry in
            fs::read_dir(&at).map_err(|e| refused("Failed", format!("{}: {e}", at.display())))?
        {
            let entry = entry.map_err(failed)?;
            if entry.file_type().map_err(failed)?.is_dir() {
                pending.push(entry.path());
            }
        }
    }
    match fs::remove_dir_all(dir) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(refused("Failed", format!("{}: {e}", dir.display())))
        }
        _ => Ok(()),
    }
}
