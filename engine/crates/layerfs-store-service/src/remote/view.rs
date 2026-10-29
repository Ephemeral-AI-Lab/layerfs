//! A live, read-only view of a Store file on a remote (#510): the Store is
//! read in place, through ranged requests pinned to the version it had at
//! open, and never downloaded.
//!
//! [`open_view`] stats the file once (its size, version tag and push
//! metadata), reads its binding from the push metadata, and opens a
//! read-only [`StoreService`] over the `layerfs-remote` VFS with a block
//! cache of the caller's size. Every read call the service answers works as
//! on a local Store; it constructs nothing, so every write is refused. A
//! failed request fails the read that needed it, with one of the kinds of
//! [`RemoteFailureKind`]; a file replaced since the open is `Changed`.
use crate::bootstrap::open_remote_store;
use crate::remote::heads::HEADS;
use crate::remote::push::BINDING;
use crate::StoreService;
use layerfs_persistence::remote::{RemoteFailure, RemoteFailureKind, RemoteFile, RemoteSource};
use layerfs_remote_storage::pieces::SHA256;
use layerfs_remote_storage::{Driver, PinnedFile, RemoteError};
use std::sync::Arc;

/// The Store's shared immutable-object cache in a view, as a daemon's.
const OBJECT_CACHE_BYTES: usize = 64 * 1024;

/// An open view: its service, the remote file it reads, and what the open's
/// stat said.
pub struct OpenedView {
    pub service: StoreService,
    pub file: Arc<RemoteFile>,
    pub pinned: Arc<PinnedFile>,
    /// The binding the push recorded.
    pub binding: Vec<u8>,
}

impl OpenedView {
    /// The pinned version's size.
    pub fn bytes(&self) -> u64 {
        self.pinned.bytes()
    }
    /// The pinned version's tag.
    pub fn etag(&self) -> Option<&str> {
        self.pinned.stat().etag.as_deref()
    }
    /// The pushed file's sha256 (hex), as the push recorded it.
    pub fn sha256(&self) -> Option<&str> {
        self.pinned.stat().metadata.get(SHA256).map(String::as_str)
    }
    /// The pushed file's history heads digest (hex), as the push recorded it.
    pub fn heads(&self) -> Option<&str> {
        self.pinned.stat().metadata.get(HEADS).map(String::as_str)
    }
    /// When the file was written, in milliseconds since the Unix epoch.
    pub fn modified(&self) -> Option<u64> {
        self.pinned.stat().modified
    }
    /// What the view did: `(requests, bytes)` sent and received (the open's
    /// stat included), and the block cache's counters.
    pub fn stats(&self) -> ViewStats {
        ViewStats {
            requests: self.pinned.requests(),
            bytes: self.pinned.fetched(),
            cache: self.file.stats(),
        }
    }
}

/// A view's counters (#510 §4.2).
#[derive(Clone, Copy, Debug)]
pub struct ViewStats {
    /// Requests sent to the remote, the open's stat included.
    pub requests: u64,
    /// Bytes received.
    pub bytes: u64,
    pub cache: layerfs_persistence::remote::CacheStats,
}

/// Why a view did not open.
#[derive(Debug)]
pub enum ViewFailure {
    /// The remote request of the open failed.
    Remote(RemoteError),
    /// A read of the file failed while the Store opened.
    Read(RemoteFailure),
    /// The file is not a Store LayerFS can view.
    Invalid(String),
}

impl std::fmt::Display for ViewFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Remote(error) => error.fmt(f),
            Self::Read(failure) => f.write_str(&failure.message),
            Self::Invalid(why) => f.write_str(why),
        }
    }
}

/// The kind of a failed remote request: 401 and 403 are `Denied`, 412
/// `Changed`, a sent request with an unknown outcome `Unknown`.
pub fn failure_kind(error: &RemoteError) -> RemoteFailureKind {
    match error {
        RemoteError::Unavailable(_) => RemoteFailureKind::Unavailable,
        RemoteError::Refused {
            status: 401 | 403, ..
        } => RemoteFailureKind::Denied,
        RemoteError::Changed => RemoteFailureKind::Changed,
        RemoteError::Uncertain(_) | RemoteError::PartsLeft { .. } => RemoteFailureKind::Unknown,
        _ => RemoteFailureKind::Failed,
    }
}

/// The pinned file as the reader's source.
struct Source(Arc<PinnedFile>);

impl RemoteSource for Source {
    fn bytes(&self) -> u64 {
        self.0.bytes()
    }
    fn fetch(&self, offset: u64, out: &mut [u8]) -> Result<(), RemoteFailure> {
        self.0.read(offset, out).map_err(|error| RemoteFailure {
            kind: failure_kind(&error),
            message: match error {
                RemoteError::Changed => format!(
                    "the remote file {} changed since this view opened; open it again",
                    self.0.name()
                ),
                other => other.to_string(),
            },
        })
    }
}

/// The binding a push recorded in the file's metadata.
fn binding(file: &PinnedFile) -> Result<Vec<u8>, ViewFailure> {
    let text = file.stat().metadata.get(BINDING).ok_or_else(|| {
        ViewFailure::Invalid(format!(
            "the remote file {} records no {BINDING}; push it again to view it",
            file.name()
        ))
    })?;
    let digit = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
    let bytes = text.as_bytes();
    (bytes.len() % 2 == 0 && !bytes.is_empty())
        .then(|| {
            bytes
                .chunks(2)
                .map(|pair| Some((digit(pair[0])? << 4) | digit(pair[1])?))
                .collect::<Option<Vec<u8>>>()
        })
        .flatten()
        .ok_or_else(|| ViewFailure::Invalid(format!("the remote file's {BINDING} is malformed")))
}

/// Opens a read-only view of the Store file `name` on `driver`, with a
/// block cache of `cache_bytes` and the view's own `cursor_key`.
pub fn open_view(
    driver: Box<dyn Driver>,
    name: &str,
    cache_bytes: u64,
    cursor_key: [u8; 32],
) -> Result<OpenedView, ViewFailure> {
    let pinned = Arc::new(PinnedFile::open(driver, name).map_err(ViewFailure::Remote)?);
    let binding = binding(&pinned)?;
    let file = RemoteFile::new(Arc::new(Source(pinned.clone())), cache_bytes);
    let opened =
        open_remote_store(&file, &binding, cursor_key, OBJECT_CACHE_BYTES).map_err(|error| {
            match file.failure_since(0) {
                Some(failure) => ViewFailure::Read(failure),
                None => {
                    ViewFailure::Invalid(format!("the remote file {name} is not a Store: {error}"))
                }
            }
        })?;
    Ok(OpenedView {
        service: StoreService::read_only(opened),
        file,
        pinned,
        binding,
    })
}
