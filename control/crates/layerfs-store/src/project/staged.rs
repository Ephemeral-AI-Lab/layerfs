//! The worker's side of a folder copy for `project.init` (#536 H11a): the
//! records a caller sends ([`super::tree`]) written into a staging folder
//! `<root>/incoming/init-<id>`, which the worker's Init then reads as its
//! source, and removed on every outcome the worker knows. Paths come from
//! the root and the copy's id only; an entry's path stays inside its
//! staging folder and goes through no link.
//!
//! The caller sends several parts at once, each on its own connection and
//! thread here, so their data records apply in any order: a record makes
//! the folders on its way that are not there yet (a concurrent maker is
//! fine), a directory already made is kept, and a file piece is written at
//! its offset whatever the file's length. Pieces of one file never
//! overlap. `M` records come in parts of their own, sent only after every
//! part before them was answered ([`super::send`]). No lease is taken: the
//! staging folder is the copy's alone, and the Store is not opened.
//!
//! `project.export` writes the same records strictly ([`write_exported`]):
//! its one writer applies them in order, so a folder or file that is
//! already there is a collision (`Conflict`, a case-insensitive host's
//! `A` and `a` included), never merged or overwritten, and every folder on
//! a record's way must already be a real directory.
use super::frame::{StageFrame, StageStep};
use super::tree::{Reader, Record};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::{failed, refused, Outcome};
#[cfg(unix)]
use nix::sys::stat::{utimensat, UtimensatFlags};
#[cfg(unix)]
use nix::sys::time::TimeSpec;
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, FileExt, OpenOptionsExt, PermissionsExt};

// Windows face: the bytes grammar is UTF-8 (PathName enforces it);
// the conversions live in layerfs_home::os_bytes.
#[cfg(windows)]
use layerfs_home::os_bytes::from_bytes;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// Where copy `id` is staged under the worker's `root`.
pub fn folder(root: &Path, id: &str) -> PathBuf {
    root.join("incoming").join(format!("init-{id}"))
}

/// `project.stage`: `write` applies one part of the copy, `discard`
/// removes the staging folder.
pub fn run(root: &Path, frame: &StageFrame) -> Outcome {
    let dir = folder(root, &frame.id);
    match &frame.step {
        StageStep::Write(part) => {
            layerfs_home::home::private_dir(&dir)?;
            let records = apply(&dir, part)?;
            Ok(SdkValue::map([("records", SdkValue::Int(records as i64))]))
        }
        StageStep::Discard => {
            let existed = dir.exists();
            remove(&dir)?;
            Ok(SdkValue::map([("discarded", SdkValue::Bool(existed))]))
        }
    }
}

/// Writes every record of `part` under `dir`; answers how many.
fn apply(dir: &Path, part: &[u8]) -> Result<u64> {
    let mut reader = Reader::new(part);
    let mut count = 0;
    while let Some(record) = reader.next_record()? {
        one(dir, record, false)
            .map_err(|r| refused("Failed", format!("folder copy: {}", r.message)))?;
        count += 1;
    }
    Ok(count)
}

/// Writes one `record` of an export under `dir`, strictly: an entry that is
/// already there is `Conflict`, a full disk `Capacity`.
pub fn write_exported(dir: &Path, record: Record<'_>) -> Result<()> {
    one(dir, record, true)
}

fn one(dir: &Path, record: Record<'_>, strict: bool) -> Result<()> {
    let plain = |e: String| refused("Failed", e);
    match record {
        Record::Directory { path } => {
            let at = place(dir, path, false, strict).map_err(plain)?;
            match strict {
                true => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::DirBuilderExt;
                        fs::DirBuilder::new()
                            
                            .create(&at)
                            .map_err(|e| io_refusal(&at, e))
                    }
                    // Windows: the folder's face carries the mode when a
                    // record reads it back.
                    #[cfg(windows)]
                    fs::DirBuilder::new()
                        .create(&at)
                        .map_err(|e| io_refusal(&at, e))
                }
                false => folder_at(&at).map_err(plain),
            }
        }
        Record::Write {
            path,
            offset,
            bytes,
        } => {
            let at = place(dir, path, false, strict).map_err(plain)?;
            match strict {
                true => write_in_order(&at, offset, bytes),
                false => {
                    #[cfg(unix)]
                    let file = {
                        use std::os::unix::fs::OpenOptionsExt;
                        OpenOptions::new()
                            .write(true)
                            .create(true)
                            .mode(0o600)
                            .custom_flags(libc::O_NOFOLLOW)
                            .open(&at)
                            .map_err(|e| io_refusal(&at, e))?
                    };
                    #[cfg(windows)]
                    let file = OpenOptions::new()
                        .write(true)
                        .create(true)
                        .open(&at)
                        .map_err(|e| io_refusal(&at, e))?;
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::FileExt;
                        file.write_all_at(bytes, offset)
                            .map_err(|e| io_refusal(&at, e))
                    }
                    #[cfg(windows)]
                    {
                        use std::os::windows::fs::FileExt;
                        file.seek_write(bytes, offset)
                            .map(|_| ())
                            .map_err(|e| io_refusal(&at, e))
                    }
                }
            }
        }
        Record::Link { path, target } => {
            let at = place(dir, path, false, strict).map_err(plain)?;
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(OsStr::from_bytes(target), &at)
                    .map_err(|e| io_refusal(&at, e))
            }
            #[cfg(windows)]
            {
                let target = from_bytes(target);
                std::os::windows::fs::symlink_file(target.as_os_str(), &at)
                    .map_err(|e| io_refusal(&at, e))
            }
        }
        Record::Meta {
            path,
            mode,
            seconds,
            nanoseconds,
        } => {
            let at = place(dir, path, true, strict).map_err(plain)?;
            let shown =
                |e: &dyn std::fmt::Display| refused("Failed", format!("{}: {e}", at.display()));
            let meta = fs::symlink_metadata(&at).map_err(|e| io_refusal(&at, e))?;
            #[cfg(unix)]
            if !meta.file_type().is_symlink() {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&at, fs::Permissions::from_mode(mode & 0o7777))
                    .map_err(|e| io_refusal(&at, e))?;
            }
            // Windows: the readonly attribute is the only mode bit; the
            // recorded face already chose 0644/0755 writable shapes, and
            // the mtime write is the port's SetFileTime remainder (#454).
            #[cfg(windows)]
            {
                let _ = (mode, seconds, nanoseconds);
                let _ = &meta;
                Ok::<(), SdkRefusal>(())
            }
            #[cfg(unix)]
            {
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
}

/// An export's file piece at `offset` of the file at `at`: the first piece
/// makes the file (one already there is `Conflict`), each later one follows
/// the file's length.
fn write_in_order(at: &Path, offset: u64, bytes: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
        if offset == 0 {
            options.mode(0o600);
        }
    }
    #[cfg(windows)]
    {
        use std::io::Write;
        let _ = &mut options;
    }
    if offset == 0 {
        options.create_new(true);
    }
    let mut file = options.open(at).map_err(|e| io_refusal(at, e))?;
    if file.metadata().map_err(|e| io_refusal(at, e))?.len() != offset {
        return Err(refused(
            "Failed",
            format!(
                "{}: a piece at {offset} does not follow the file's length",
                at.display()
            ),
        ));
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| io_refusal(at, e))?;
    file.write_all(bytes).map_err(|e| io_refusal(at, e))
}

/// An I/O failure at `at`: an entry already there is `Conflict`, a full
/// disk `Capacity`, anything else `Failed`.
fn io_refusal(at: &Path, error: std::io::Error) -> SdkRefusal {
    let code = if error.kind() == std::io::ErrorKind::AlreadyExists {
        "Conflict"
    } else if error.raw_os_error() == Some(libc::ENOSPC) {
        "Capacity"
    } else {
        "Failed"
    };
    refused(code, format!("{}: {error}", at.display()))
}

/// A directory at `at` (mode 0700), made unless it is there: another
/// part's record may make it first. Anything else there, a link to a
/// directory included, is refused.
fn folder_at(at: &Path) -> std::result::Result<(), String> {
    #[cfg(unix)]
    let made = {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(at)
    };
    #[cfg(windows)]
    let made = fs::DirBuilder::new().create(at);
    if let Err(e) = made {
        if e.kind() != std::io::ErrorKind::AlreadyExists {
            return Err(format!("{}: {e}", at.display()));
        }
    }
    let meta = fs::symlink_metadata(at).map_err(|e| format!("{}: {e}", at.display()))?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(format!("{} is not a directory", at.display()));
    }
    Ok(())
}

/// `path` under `dir`: plain names only (no `.`, `..` or empty one), each
/// folder on the way a real directory, not a link, made when it is not
/// there yet ([`folder_at`]); a `strict` (export) path's folders must be
/// there already. The empty path is `dir` itself, only where `root` allows
/// it.
fn place(
    dir: &Path,
    path: &[u8],
    root: bool,
    strict: bool,
) -> std::result::Result<PathBuf, String> {
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
        #[cfg(unix)]
        at.push(OsStr::from_bytes(name));
        #[cfg(windows)]
        at.push(from_bytes(name));
        if names.peek().is_some() {
            match strict {
                true => real_dir(&at)?,
                false => folder_at(&at)?,
            }
        }
    }
    Ok(at)
}

/// Whether `at` is already a real directory, not a link.
fn real_dir(at: &Path) -> std::result::Result<(), String> {
    let meta = fs::symlink_metadata(at).map_err(|e| format!("{}: {e}", at.display()))?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(format!("{} is not a directory", at.display()));
    }
    Ok(())
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
        #[cfg(unix)]
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
