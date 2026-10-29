//! Push: one consistent snapshot of a live Store, staged beside it, its size
//! checked against the remote's limits, then one upload of the whole file
//! (as pieces when it is larger than the remote holds as one object) with
//! its sha256, under the Store's name.
//!
//! The snapshot's history heads digest goes with it as `layerfs-heads`
//! (#506 E2). Progress (#506 E4) reports `check`, `snapshot` (bytes copied),
//! then the upload's `hash` and `upload` to the thread's transfer progress.
use crate::bootstrap::OpenedStore;
use crate::remote::heads::{heads, HEADS};
use crate::remote::settings::store_object;
use layerfs_persistence::STORE_HEADROOM_BYTES;
use layerfs_remote_storage::pieces::{self, MAX_MANIFEST};
use layerfs_remote_storage::{progress, Driver, Metadata, RemoteError};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Metadata key of the SQLite version that wrote the pushed Store.
pub const SQLITE_VERSION: &str = "layerfs-sqlite";
/// Metadata key of the Store's binding key (hex), which a clone opens with.
/// The binding is not a secret; the cursor key never leaves its holder.
pub const BINDING: &str = "layerfs-binding";

/// What the remote must hold for a push to go ahead: the copy this home
/// last pushed or pulled. Checked before the snapshot and again just before
/// the upload; not atomic (no compare-and-swap on the remote), so a push by
/// another machine in between is not detected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Expected {
    /// No check: overwrite whatever is there.
    Any,
    /// The remote holds no copy of this Store.
    Absent,
    /// The remote copy has this sha256.
    Sha256([u8; 32]),
}
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
    /// The snapshot's history heads digest; `None` when it could not be
    /// read, and then no `layerfs-heads` was sent.
    pub heads: Option<[u8; 32]>,
    /// Stale pieces of an earlier, larger push that could not be removed;
    /// the pushed Store itself is complete.
    pub cleanup: Option<RemoteError>,
}

/// Why a push did not complete.
#[derive(Debug)]
pub enum PushFailure {
    /// Bad input; nothing was written or sent.
    Invalid(String),
    /// The remote copy is not the expected one: it moved since this home's
    /// last push or pull. `found` is its sha256, `None` when absent or
    /// unrecorded. Nothing was uploaded.
    RemoteMoved {
        expected: Expected,
        found: Option<[u8; 32]>,
    },
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
            Self::RemoteMoved { found, .. } => write!(
                f,
                "Store push: the remote copy moved since this home's last push or pull (it \
                 now holds {}); pull it first, or push without the check to overwrite it",
                found.map_or_else(|| "nothing".to_owned(), |sha| pieces::hex(&sha))
            ),
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

/// Refuses when the remote copy of `object` is not `expected`.
fn check_expected(
    driver: &dyn Driver,
    object: &str,
    expected: Expected,
) -> Result<(), PushFailure> {
    if expected == Expected::Any {
        return Ok(());
    }
    let stat = driver.stat(object).map_err(PushFailure::Remote)?;
    let found = stat.as_ref().map(|stat| {
        stat.metadata
            .get(pieces::SHA256)
            .and_then(|text| pieces::parse_hex32(text))
    });
    let matches = match expected {
        Expected::Any => true,
        Expected::Absent => found.is_none(),
        Expected::Sha256(sha) => found == Some(Some(sha)),
    };
    if matches {
        Ok(())
    } else {
        Err(PushFailure::RemoteMoved {
            expected,
            found: found.flatten(),
        })
    }
}

/// Pushes the live Store `opened`, whose file is `store` and whose binding
/// key is `binding`, to `driver` as the Store named `name`, if the remote
/// holds the `expected` copy. The snapshot is staged at `store` + `.pushing`, on
/// the Store's own filesystem, after checking room for one Store-sized copy
/// plus the Store's write headroom;
/// it is this call's own copy and is removed when the call ends.
pub fn push(
    opened: &OpenedStore,
    store: &Path,
    driver: &dyn Driver,
    name: &str,
    binding: &[u8],
    expected: Expected,
) -> Result<Pushed, PushFailure> {
    let object = store_object(name).map_err(PushFailure::Invalid)?;
    let binding = pieces::hex(binding);
    if binding.is_empty() || binding.len() > 256 {
        return Err(PushFailure::Invalid(
            "the Store binding must be 1 to 128 bytes".to_owned(),
        ));
    }
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
    let live = length(store).map_err(local)? + length(Path::new(&wal)).map_err(local)?;
    let needed = live + STORE_HEADROOM_BYTES;
    let filesystem = nix::sys::statvfs::statvfs(directory)
        .map_err(|error| PushFailure::Invalid(format!("{}: {error}", directory.display())))?;
    let available =
        u128::from(filesystem.blocks_available()) * u128::from(filesystem.fragment_size());
    let available = u64::try_from(available).unwrap_or(u64::MAX);
    if available < needed {
        return Err(PushFailure::Space {
            directory: directory.to_owned(),
            needed,
            available,
        });
    }
    progress::begin("check", None);
    check_expected(driver, &object, expected)?;
    let result = (|| {
        progress::begin("snapshot", Some(live));
        let mut copied: Option<u64> = None;
        let sealed = opened
            .snapshot_observed(&staged, &mut |done, total| {
                if copied.is_none() {
                    // the copy's own size replaces the live estimate
                    progress::begin("snapshot", Some(total));
                }
                progress::add(done - copied.unwrap_or(0).min(done));
                copied = Some(done);
            })
            .map_err(|error| PushFailure::Snapshot(error.to_string()))?;
        let digest = heads(&staged).ok();
        let count = limits.pieces(sealed.bytes);
        if count.saturating_mul(MANIFEST_LINE) > MAX_MANIFEST {
            return Err(PushFailure::Remote(RemoteError::TooLarge {
                bytes: sealed.bytes,
                max: MAX_MANIFEST / MANIFEST_LINE * limits.piece_size,
            }));
        }
        let mut metadata = Metadata::from([
            (SQLITE_VERSION.to_owned(), sealed.sqlite_version),
            (BINDING.to_owned(), binding.clone()),
        ]);
        if let Some(digest) = &digest {
            metadata.insert(HEADS.to_owned(), pieces::hex(digest));
        }
        check_expected(driver, &object, expected)?;
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
            heads: digest,
            cleanup: moved.cleanup,
        })
    })();
    // This call's own copy, not data of record. A snapshot that failed may
    // leave an incomplete file there, and reading its heads sidecars that
    // its close removes; all are this call's.
    let _ = fs::remove_file(&staged);
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = staged.as_os_str().to_owned();
        sidecar.push(suffix);
        let _ = fs::remove_file(PathBuf::from(sidecar));
    }
    result
}
