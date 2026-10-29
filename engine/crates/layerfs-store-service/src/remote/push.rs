//! Push: one consistent snapshot of a live Store, staged beside it, its size
//! checked against the remote's limits, then one upload of the whole file
//! (as pieces when it is larger than the remote holds as one object) with
//! its sha256, under the Store's name.
use crate::bootstrap::OpenedStore;
use crate::remote::settings::store_object;
use layerfs_persistence::STORE_HEADROOM_BYTES;
use layerfs_remote_storage::pieces::{self, MAX_MANIFEST};
use layerfs_remote_storage::{Driver, Metadata, RemoteError};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Metadata key of the SQLite version that wrote the pushed Store.
pub const SQLITE_VERSION: &str = "layerfs-sqlite";
/// A bound on one manifest line, so a manifest never exceeds what a pull reads.
const MANIFEST_LINE: u64 = 96;

/// What a push uploaded.
#[derive(Debug)]
pub struct Pushed {
    /// The remote file, `NAME/store.sqlite`.
    pub object: String,
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
    /// Bad input; nothing was written or sent.
    Invalid(String),
    /// The Store's filesystem lacks room for the staged snapshot; nothing
    /// was written or sent.
    Space {
        directory: PathBuf,
        needed: u64,
        available: u64,
    },
    /// The snapshot failed; nothing was sent.
    Snapshot(String),
    /// The upload failed. `RemoteError::Uncertain` means its outcome is
    /// unknown; nothing was resent.
    Remote(RemoteError),
}

impl std::fmt::Display for PushFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(cause) => write!(f, "Store push: {cause}"),
            Self::Space {
                directory,
                needed,
                available,
            } => write!(
                f,
                "Store push: the snapshot is staged beside the Store and needs {needed} \
                 bytes free in {}, which has {available}; free space there and push again",
                directory.display()
            ),
            Self::Snapshot(cause) => write!(f, "Store push: snapshot: {cause}"),
            Self::Remote(error) => write!(f, "Store push: {error}"),
        }
    }
}

fn length(path: &Path) -> io::Result<u64> {
    match fs::metadata(path) {
        Ok(found) => Ok(found.len()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error),
    }
}

/// Pushes the live Store `opened`, whose file is `store`, to `driver` as the
/// Store named `name`. The snapshot is staged at `store` + `.pushing`, on
/// the Store's own filesystem, after checking room for one Store-sized copy
/// plus the Store's write headroom;
/// it is this call's own copy and is removed when the call ends.
pub fn push(
    opened: &OpenedStore,
    store: &Path,
    driver: &dyn Driver,
    name: &str,
) -> Result<Pushed, PushFailure> {
    let object = store_object(name).map_err(PushFailure::Invalid)?;
    let directory = store
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| PushFailure::Invalid("the Store path has no directory".to_owned()))?;
    let mut staged = store.as_os_str().to_owned();
    staged.push(".pushing");
    let staged = PathBuf::from(staged);
    let limits = driver.limits();
    limits.validate().map_err(PushFailure::Remote)?;
    // A snapshot is at most the main file plus what its WAL adds; the live
    // Store keeps its own write headroom beside it.
    let local = |error: io::Error| PushFailure::Invalid(format!("{}: {error}", store.display()));
    let mut wal = store.as_os_str().to_owned();
    wal.push("-wal");
    let needed = length(store).map_err(local)?
        + length(Path::new(&wal)).map_err(local)?
        + STORE_HEADROOM_BYTES;
    let filesystem = nix::sys::statvfs::statvfs(directory)
        .map_err(|error| PushFailure::Invalid(format!("{}: {error}", directory.display())))?;
    let available = u128::from(filesystem.blocks_available())
        * u128::from(filesystem.fragment_size());
    let available = u64::try_from(available).unwrap_or(u64::MAX);
    if available < needed {
        return Err(PushFailure::Space {
            directory: directory.to_owned(),
            needed,
            available,
        });
    }
    let result = (|| {
        let sealed = opened
            .snapshot(&staged)
            .map_err(|error| PushFailure::Snapshot(error.to_string()))?;
        let count = limits.pieces(sealed.bytes);
        if count.saturating_mul(MANIFEST_LINE) > MAX_MANIFEST {
            return Err(PushFailure::Remote(RemoteError::TooLarge {
                bytes: sealed.bytes,
                max: MAX_MANIFEST / MANIFEST_LINE * limits.piece_size,
            }));
        }
        let metadata = Metadata::from([(SQLITE_VERSION.to_owned(), sealed.sqlite_version)]);
        let moved =
            pieces::upload(driver, &object, &staged, &metadata).map_err(PushFailure::Remote)?;
        if moved.bytes != sealed.bytes {
            return Err(PushFailure::Snapshot(
                "the snapshot changed while it was uploaded".to_owned(),
            ));
        }
        Ok(Pushed {
            object: object.clone(),
            bytes: moved.bytes,
            sha256: moved.sha256,
            pieces: moved.pieces,
            cleanup: moved.cleanup,
        })
    })();
    // This call's own copy, not data of record. A snapshot that failed may
    // leave an incomplete file there; it is this call's too.
    let _ = fs::remove_file(&staged);
    result
}
