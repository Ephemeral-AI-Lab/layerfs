//! A SQLite Store read in place from a remote file (#510): the read-only,
//! immutable `layerfs-remote` VFS over a bounded block cache.
//!
//! The caller supplies the file as a [`RemoteSource`] (the remote storage
//! driver pinned to one version of the file) and opens [`Handles`] on it
//! with [`Handles::open_remote`]. Every SQLite read of the file goes
//! through the file's [`BlockCache`]; nothing is written, locked, synced
//! or deleted, and no WAL, `-shm` or journal is looked for. A failed
//! request fails the SQLite read that needed it (`SQLITE_IOERR_READ`); the
//! failure is kept on the file, so the call that saw the read fail can
//! report its kind ([`RemoteFile::failure_since`]). Nothing is retried.
//!
//! [`Handles`]: crate::Handles
//! [`Handles::open_remote`]: crate::Handles::open_remote
pub mod cache;
mod vfs;

pub use cache::{BlockCache, CacheStats, BLOCK, DEFAULT_CACHE, MAX_REQUEST};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
pub(crate) use vfs::{ensure as ensure_vfs, NAME as VFS_NAME};

/// Why a remote read failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemoteFailureKind {
    /// The remote could not be reached; nothing was sent.
    Unavailable,
    /// The remote refused the credentials (401, 403).
    Denied,
    /// The file is no longer the version the view opened (412).
    Changed,
    /// A request was sent and its outcome is unknown (connection lost).
    Unknown,
    /// Any other failure.
    Failed,
}

/// A failed remote read, with its kind.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteFailure {
    /// What kind of failure.
    pub kind: RemoteFailureKind,
    /// The remote's own account of it.
    pub message: String,
}

impl RemoteFailure {
    /// A failure of kind `Failed`.
    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            kind: RemoteFailureKind::Failed,
            message: message.into(),
        }
    }
}

/// One version of a remote file, read by byte range.
pub trait RemoteSource: Send + Sync {
    /// The file's size.
    fn bytes(&self) -> u64;
    /// Fills `out` with the file's bytes from `offset` (within the file),
    /// in as few requests as the remote's layout allows, each sent once.
    fn fetch(&self, offset: u64, out: &mut [u8]) -> Result<(), RemoteFailure>;
}

/// A remote Store file as the VFS opens it: its source, its block cache
/// and the last failure its reads met. Registered with the VFS under its
/// own name while it lives; every connection opened on it shares it.
pub struct RemoteFile {
    name: String,
    source: Arc<dyn RemoteSource>,
    cache: BlockCache,
    /// Counts failures; `failure` is the latest.
    failures: AtomicU64,
    failure: Mutex<Option<RemoteFailure>>,
}

impl RemoteFile {
    /// `source` with a block cache of `cache_bytes`, registered with the VFS.
    pub fn new(source: Arc<dyn RemoteSource>, cache_bytes: u64) -> Arc<Self> {
        let name = vfs::unique_name();
        let file = Arc::new(Self {
            name,
            source,
            cache: BlockCache::new(cache_bytes),
            failures: AtomicU64::new(0),
            failure: Mutex::new(None),
        });
        vfs::register(&file);
        file
    }

    /// The name the VFS opens this file by.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The file's size.
    pub fn bytes(&self) -> u64 {
        self.source.bytes()
    }

    /// What the block cache did.
    pub fn stats(&self) -> CacheStats {
        self.cache.stats()
    }

    /// How many reads have failed so far; pass it to [`Self::failure_since`].
    pub fn failures(&self) -> u64 {
        self.failures.load(Ordering::Acquire)
    }

    /// The latest failure, when one happened after `failures` were counted.
    pub fn failure_since(&self, failures: u64) -> Option<RemoteFailure> {
        if self.failures() == failures {
            return None;
        }
        self.failure
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }

    /// Reads `out.len()` bytes at `offset` through the cache, keeping a
    /// failure for the caller. Once the file has changed, a read the cache
    /// cannot answer fails `Changed` without a request.
    fn read(&self, offset: u64, out: &mut [u8]) -> Result<(), ()> {
        let changed = self.failure_since(0).map(|failure| failure.kind);
        let result = if changed == Some(RemoteFailureKind::Changed) {
            self.cache.read(&Refuse(self.source.bytes()), offset, out)
        } else {
            self.cache.read(self.source.as_ref(), offset, out)
        };
        result.map_err(|failure| {
            *self
                .failure
                .lock()
                .unwrap_or_else(|poison| poison.into_inner()) = Some(failure);
            self.failures.fetch_add(1, Ordering::AcqRel);
        })
    }
}

impl Drop for RemoteFile {
    fn drop(&mut self) {
        vfs::unregister(&self.name);
    }
}

/// The source of a file of this size that has changed: every fetch fails
/// `Changed`, so only cached blocks are read.
struct Refuse(u64);

impl RemoteSource for Refuse {
    fn bytes(&self) -> u64 {
        self.0
    }
    fn fetch(&self, _: u64, _: &mut [u8]) -> Result<(), RemoteFailure> {
        Err(RemoteFailure {
            kind: RemoteFailureKind::Changed,
            message: "the remote file changed since this view opened; open it again".to_owned(),
        })
    }
}
