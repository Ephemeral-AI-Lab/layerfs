//! Push: one consistent snapshot of the live Store to a temporary file, its
//! size checked against the remote's limits, then one upload of the whole
//! file (as pieces when it is larger than the remote holds as one object)
//! with its sha256.
use crate::bootstrap::OpenedStore;
use crate::remote::settings::STORE_OBJECT;
use layerfs_remote_storage::pieces::{self, MAX_MANIFEST};
use layerfs_remote_storage::{Driver, Metadata, RemoteError};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Metadata key of the SQLite version that wrote the pushed Store.
pub const SQLITE_VERSION: &str = "layerfs-sqlite";
/// A bound on one manifest line, so a manifest never exceeds what a pull reads.
const MANIFEST_LINE: u64 = 96;

/// What a push uploaded.
#[derive(Debug)]
pub struct Pushed {
    pub bytes: u64,
    pub sha256: [u8; 32],
    /// 0 when the Store went up as one object.
    pub pieces: u64,
    /// Stale pieces of an earlier, larger push that could not be removed;
    /// the pushed Store itself is complete.
    pub cleanup: Option<RemoteError>,
}

/// Why a push did not complete.
#[derive(Debug)]
pub enum PushFailure {
    /// The snapshot failed; nothing was sent.
    Snapshot(String),
    /// The upload failed. `RemoteError::Uncertain` means its outcome is
    /// unknown; nothing was resent.
    Remote(RemoteError),
}

impl std::fmt::Display for PushFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Snapshot(cause) => write!(f, "Store push: snapshot: {cause}"),
            Self::Remote(error) => write!(f, "Store push: {error}"),
        }
    }
}

/// Pushes the live Store `opened` to `driver`, snapshotting it into the
/// existing directory `scratch` first. The snapshot is this call's own
/// temporary copy and is removed when the call ends.
pub fn push(
    opened: &OpenedStore,
    driver: &dyn Driver,
    scratch: &Path,
) -> Result<Pushed, PushFailure> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let temporary = scratch.join(format!(
        "layerfs-push-{}-{nonce}.sqlite",
        std::process::id()
    ));
    let result = (|| {
        let limits = driver.limits();
        limits.validate().map_err(PushFailure::Remote)?;
        let sealed = opened
            .snapshot(&temporary)
            .map_err(|error| PushFailure::Snapshot(error.to_string()))?;
        let count = limits.pieces(sealed.bytes);
        if count.saturating_mul(MANIFEST_LINE) > MAX_MANIFEST {
            return Err(PushFailure::Remote(RemoteError::TooLarge {
                bytes: sealed.bytes,
                max: MAX_MANIFEST / MANIFEST_LINE * limits.piece_size,
            }));
        }
        let metadata = Metadata::from([(SQLITE_VERSION.to_owned(), sealed.sqlite_version)]);
        let moved = pieces::upload(driver, STORE_OBJECT, &temporary, &metadata)
            .map_err(PushFailure::Remote)?;
        if moved.bytes != sealed.bytes {
            return Err(PushFailure::Snapshot(
                "the snapshot changed while it was uploaded".to_owned(),
            ));
        }
        Ok(Pushed {
            bytes: moved.bytes,
            sha256: moved.sha256,
            pieces: moved.pieces,
            cleanup: moved.cleanup,
        })
    })();
    // This call's own copy, not data of record (absent if the snapshot failed early).
    let _ = fs::remove_file(&temporary);
    result
}
