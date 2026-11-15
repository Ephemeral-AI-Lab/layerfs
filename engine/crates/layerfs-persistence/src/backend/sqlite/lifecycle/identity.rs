//! §10 Store-file identity: the `(dev, ino)` fence a SQLite handle applies
//! before it reads or writes, and the install fence re-applies to the sealed
//! source.
use std::path::Path;

/// The `(device, inode)` pair naming one Store file at one moment (§10).
///
/// A transaction compares the pair recorded at open with the pair the Store
/// path names now; a replacement or a move is `StaleStore`. Together with the
/// held writer lane this is the SQLite fence of §10: an admitted writer cannot
/// be in flight at install, and a connection that bypassed admission is caught
/// at its next transaction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FileIdentity {
    /// The filesystem device holding the file.
    pub(crate) dev: u64,
    /// The file's inode number on that device.
    pub(crate) ino: u64,
}

impl FileIdentity {
    /// Captures the identity of the file `path` currently names.
    ///
    /// `Ok(None)` means this platform has no `(dev, ino)` pair; the fence then
    /// has nothing to compare and callers keep their other facts. `Err` is a
    /// path that could not be inspected at all.
    pub(crate) fn capture(path: &Path) -> Result<Option<Self>, std::io::Error> {
        let metadata = std::fs::metadata(path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            Ok(Some(Self {
                dev: metadata.dev(),
                ino: metadata.ino(),
            }))
        }
        #[cfg(not(unix))]
        {
            let _ = metadata;
            Ok(None)
        }
    }

    /// §10: whether `path` still names this exact file.
    pub(crate) fn names(&self, path: &Path) -> Result<bool, std::io::Error> {
        match Self::capture(path)? {
            Some(current) => Ok(current == *self),
            None => Ok(true),
        }
    }
}
