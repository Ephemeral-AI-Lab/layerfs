//! Host byte encoding of paths and names, and the metadata identity Init
//! reads. Unix uses the raw bytes and the stat identity; Windows has no raw
//! byte spelling, so names are carried as their UTF-8 bytes (a name outside
//! UTF-8 becomes lossy — a recorded limitation of this route), and the
//! identity is the entry's real volume serial number and file index, its
//! length, timestamps and a mode synthesized from the read-only attribute.
//! Hard-link membership is not detected on Windows (the format supports it;
//! each link acquires as its own file).
use std::ffi::{OsStr, OsString};
use std::fs::Metadata;
use std::io;
use std::path::{Path, PathBuf};

#[cfg(windows)]
#[path = "host_wtf8_inner.rs"]
mod name_bytes;

/// One host name's bytes.
pub(super) fn os_to_bytes(value: &OsStr) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        value.as_bytes().to_vec()
    }
    #[cfg(windows)]
    {
        name_bytes::encode(value)
    }
}

/// The host name of `value`'s bytes.
pub(super) fn bytes_to_os(value: &[u8]) -> OsString {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(value.to_vec())
    }
    #[cfg(windows)]
    {
        name_bytes::decode(value)
    }
}

/// The host path of `value`'s bytes.
pub(super) fn bytes_to_path(value: &[u8]) -> PathBuf {
    PathBuf::from(bytes_to_os(value))
}

/// The host identity fields Init observes.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct HostStat {
    pub dev: u64,
    pub ino: u64,
    pub len: u64,
    pub mode: u32,
    pub mtime: i64,
    pub mtime_nsec: i64,
    pub ctime: i64,
    pub ctime_nsec: i64,
}

impl HostStat {
    /// Reads `path`'s identity. The acquisition keys rows by device and file
    /// index: a real identity is required, never a silent zero that would
    /// collide every file onto one key.
    pub(super) fn of(path: &Path, metadata: &Metadata) -> io::Result<Self> {
        #[cfg(unix)]
        {
            let _ = path;
            use std::os::unix::fs::MetadataExt;
            Ok(Self {
                dev: metadata.dev(),
                ino: metadata.ino(),
                len: metadata.len(),
                mode: metadata.mode(),
                mtime: metadata.mtime(),
                mtime_nsec: metadata.mtime_nsec(),
                ctime: metadata.ctime(),
                ctime_nsec: metadata.ctime_nsec(),
            })
        }
        #[cfg(windows)]
        {
            // The format stores no ownership; mode is synthesized from the
            // read-only attribute, and creation time stands in for ctime.
            let base = if metadata.is_dir() { 0o555 } else { 0o444 };
            let mode = if metadata.permissions().readonly() {
                base
            } else {
                base | 0o200
            };
            let (mtime, mtime_nsec) = match metadata.modified() {
                Ok(time) => match time.duration_since(std::time::UNIX_EPOCH) {
                    Ok(span) => (span.as_secs() as i64, i64::from(span.subsec_nanos())),
                    Err(before) => (
                        -(before.duration().as_secs() as i64),
                        -(i64::from(before.duration().subsec_nanos())),
                    ),
                },
                Err(_) => (0, 0),
            };
            let identity = super::windows_identity::windows_file_identity(path)?;
            Ok(Self {
                dev: identity.device,
                ino: identity.index,
                len: metadata.len(),
                mode,
                mtime,
                mtime_nsec,
                ctime: identity.created,
                ctime_nsec: identity.created_nsec,
            })
        }
    }
}
