//! The export's own scratch files for AppleDouble companion bodies.
//!
//! A companion body beyond the resident budget lives in one file per
//! companion serial under the export's spill directory, read and written with
//! `pread`/`pwrite` by an operation that opens it and closes it again: the
//! export holds no descriptor per spilled body. A spill writes a temporary
//! `<serial>.<n>.tmp` that only the call adopting it renames to `<serial>`, so
//! two calls spilling the same companion never share a file. The directory
//! and its files are this user's alone; the modes are set here rather than
//! left to the caller's umask.
use crate::attributes::Status;
use layerfs_content::filesystem::attributes::PortableMetadata;
use std::{
    fs::{self, File, OpenOptions},
    io,
    os::unix::fs::{DirBuilderExt, FileExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

/// Mode of a companion created without one.
pub(crate) const MODE: u32 = 0o644;
/// Temporary names tried for one spilled body before the spill gives up.
const SPILL_TEMP_TRIES: u32 = 64;
/// Counter behind the distinct temporary name of each spilled body.
static SPILL_TEMP: AtomicU64 = AtomicU64::new(0);

/// A failed file operation as the client sees it.
pub(crate) fn io_status(error: &io::Error) -> Status {
    match (error.kind(), error.raw_os_error()) {
        (io::ErrorKind::StorageFull, _) | (_, Some(28 | 69 | 122)) => Status::NoSpc,
        (_, Some(27)) => Status::FBig,
        (_, Some(1)) => Status::Perm,
        (io::ErrorKind::PermissionDenied, _) | (_, Some(13)) => Status::Access,
        (io::ErrorKind::ReadOnlyFilesystem, _) | (_, Some(30)) => Status::RoFs,
        _ => Status::Io,
    }
}
/// The spill directory's own file of `serial`.
pub(crate) fn spill_path(dir: &Path, serial: u64) -> PathBuf {
    dir.join(serial.to_string())
}
/// Creates `dir` and a temporary file in it holding `bytes` extended to `len`
/// bytes, then closes it: the caller adopts it by renaming it to the
/// companion's own name.
pub(crate) fn create_spilled(
    dir: &Path,
    serial: u64,
    bytes: &[u8],
    len: u64,
) -> Result<PathBuf, Status> {
    let made = fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir);
    made.map_err(|e| io_status(&e))?;
    for _ in 0..SPILL_TEMP_TRIES {
        let name = SPILL_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = dir.join(format!("{serial}.{name}.tmp"));
        let file = match OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(io_status(&e)),
        };
        let written = (file.write_all_at(bytes, 0)).and_then(|()| file.set_len(len));
        drop(file);
        return match written {
            Ok(()) => Ok(path),
            Err(e) => {
                drop(fs::remove_file(&path));
                Err(io_status(&e))
            }
        };
    }
    Err(Status::Io)
}
/// Unlinks the spilled files a companion operation retired, outside the lock.
pub(crate) fn unlink_paths(paths: impl IntoIterator<Item = PathBuf>) {
    for path in paths {
        drop(fs::remove_file(path));
    }
}
/// The facts of a companion body created now.
pub(crate) fn now() -> PortableMetadata {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    PortableMetadata {
        mode: MODE,
        mtime_seconds: i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX),
        mtime_nanoseconds: elapsed.subsec_nanos(),
    }
}
/// Records a change of a companion body's time, keeping its mode.
pub(crate) fn touch(metadata: &mut PortableMetadata) {
    let mode = metadata.mode;
    *metadata = now();
    metadata.mode = mode;
}
/// One change to a companion body, applied to the resident bytes under the
/// lock or to the spilled file outside it.
pub(crate) enum Edit<'a> {
    Write { offset: usize, data: &'a [u8] },
    Size(usize),
}
impl Edit<'_> {
    /// The body length this edit needs; a write keeps what the body had beyond
    /// its end.
    pub(crate) fn target_len(&self) -> usize {
        match self {
            Edit::Write { offset, data } => offset + data.len(),
            Edit::Size(size) => *size,
        }
    }
    /// Applies the edit to resident bytes, which already hold `target_len`.
    pub(crate) fn memory(&self, bytes: &mut Vec<u8>) {
        match self {
            Edit::Write { offset, data } => {
                bytes.resize(bytes.len().max(offset + data.len()), 0);
                bytes[*offset..*offset + data.len()].copy_from_slice(data);
            }
            Edit::Size(size) => bytes.resize(*size, 0),
        }
    }
    /// Applies the edit to a spilled file, outside the companion lock.
    pub(crate) fn disk(&self, file: &File) -> Result<(), Status> {
        match self {
            Edit::Write { offset, data } => {
                (file.write_all_at(data, *offset as u64)).map_err(|e| io_status(&e))
            }
            Edit::Size(size) => (file.set_len(*size as u64)).map_err(|e| io_status(&e)),
        }
    }
}
/// Where [`crate::apple_double`] found a companion's body.
pub(crate) enum Room {
    /// Applied to the resident body, whose length, recency and time were
    /// recorded under the same lock: this is the companion's stat afterwards.
    Resident(layerfs_workspace::ViewStat),
    /// On disk: finish with its file outside the lock.
    Spilled,
    /// Resident, but growing it needs this many more budget bytes first.
    Grow(usize),
}
