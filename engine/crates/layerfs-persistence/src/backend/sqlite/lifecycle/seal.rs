//! Sole-owner host seal; never available through the daemon's Store ports.
use crate::backend::sqlite::lane::writer_admission::Writer;
use crate::backend::sqlite::lifecycle::identity::FileIdentity;
use crate::backend::sqlite::session::connection::{Session, State};
use crate::backend::sqlite::session::query;
use crate::backend::sqlite::session::rows;
use crate::{backend::error::BackendError, SealedStore};
use layerfs_storage::port::PersistenceError;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// The identity of one closed Store file at the moment its last connection
/// closed, captured under the writer lane (§7 step 1).
#[derive(Clone, Debug)]
pub(crate) struct SealedFacts {
    /// The closed database file; no sidecar was present when it was captured.
    pub(crate) path: PathBuf,
    /// Provisioning profile the file was closed under.
    pub(crate) profile: crate::SqlitePersistenceProfile,
    /// SQLite version that created/sealed the file.
    pub(crate) sqlite_version: String,
    /// Main-file bytes after the WAL was checkpointed and the connection closed.
    pub(crate) bytes: u64,
    /// Main-file modification time at the same moment.
    pub(crate) modified: SystemTime,
    /// The database header change counter (big-endian bytes 24..28).
    pub(crate) change_counter: u32,
    /// The file's §10 `(dev, ino)` while the lane was still held.
    pub(crate) identity: Option<FileIdentity>,
}

impl SealedFacts {
    /// The public sealed-Store handoff this identity describes.
    pub(crate) fn into_store(self) -> SealedStore {
        SealedStore {
            path: self.path,
            profile: self.profile,
            sqlite_version: self.sqlite_version,
            bytes: self.bytes,
        }
    }
}

/// A failed seal and the writer lane it was given, when the seal still owned
/// it: a compaction's cleanup after a definite failure is a write to the live
/// Store and runs under that lane (#776 S2). `None` only when the seal found
/// no lane to give back.
pub(crate) struct SealFailure {
    pub(crate) error: PersistenceError,
    pub(crate) writer: Option<Writer>,
}

impl From<SealFailure> for PersistenceError {
    fn from(failure: SealFailure) -> Self {
        failure.error
    }
}

/// Ends a failed seal's connection under the lane it owns, as `State::drop`
/// would close it, and hands the lane back instead of releasing it.
fn hand_back(state: &mut State, error: PersistenceError) -> SealFailure {
    drop(state.connection.take());
    SealFailure {
        error,
        writer: state.writer_hold.take(),
    }
}

fn read_change_counter(path: &Path) -> Result<u32, PersistenceError> {
    let refused = |error: std::io::Error| PersistenceError::Refused {
        status: error.to_string(),
    };
    let mut file = std::fs::File::open(path).map_err(refused)?;
    let mut bytes = [0u8; 4];
    file.seek(SeekFrom::Start(24)).map_err(refused)?;
    file.read_exact(&mut bytes).map_err(refused)?;
    Ok(u32::from_be_bytes(bytes))
}

/// What a seal does with macOS allocation past the file's logical end.
enum Extents {
    /// The file is handed off as it is: give the unused allocation back.
    Release,
    /// The file is about to be replaced: leave its allocation alone.
    Keep,
}

impl Session {
    /// Consumes the sole session, checkpoints once, closes and verifies one file.
    /// macOS then releases unused allocation beyond the original file's logical
    /// end, without copying its contents or changing its identity or length.
    ///
    /// A retained provider/Storage/Reader/Save refuses with `Busy` before any
    /// checkpoint. Another process can obstruct checkpoint or keep sidecars;
    /// neither case triggers a retry or deletion. Provisioning must prevent
    /// new openers throughout sealing and handoff. This is not live Store
    /// reclamation. Uncertain transfer/temporary-close outcomes retain the owned
    /// `.layerfs-allocation-*` sibling; no automatic retry or deletion follows.
    /// A file that was ever one side of an APFS clone keeps its allocation: the
    /// filesystem refuses the release before moving anything, and the seal
    /// still succeeds (see `seal_allocation`).
    pub(crate) fn seal(self) -> Result<SealedStore, PersistenceError> {
        let writer = {
            let mut state = self.state.lock().map_err(|_| PersistenceError::Uncertain)?;
            state.writer_hold.take()
        };
        let writer = match writer {
            Some(writer) => writer,
            None => Writer::acquire(&self.path)?,
        };
        // A failure drops the lane it hands back, which releases it.
        let (facts, writer) = self.seal_under(writer, Extents::Release)?;
        writer.release().map_err(|_| PersistenceError::Uncertain)?;
        Ok(facts.into_store())
    }

    /// A compaction's seal under its caller-held writer lane (§7 step 1: the
    /// guard is passed into the seal, never acquired a second time). The lane
    /// is returned so the compaction can keep it through the rename. The
    /// checkpoint, checked close, sidecar check and the sealed `(size, mtime,
    /// change counter)` capture all run while the lane is held.
    ///
    /// Unlike [`Self::seal`] it releases no allocation: the install's rename
    /// replaces this file, which frees its allocation with it, and the fresh
    /// file is sized and cut on its own. On macOS it still refuses a Store
    /// path that is a symbolic link or has a second hard link, before the
    /// checkpoint, as `seal` does.
    ///
    /// A failure hands the lane back with its error, so the compaction's
    /// cleanup runs under it before letting it go.
    pub(crate) fn seal_holding(self, writer: Writer) -> Result<(SealedFacts, Writer), SealFailure> {
        self.seal_under(writer, Extents::Keep)
    }

    /// The seal both entry points share; `extents` is their one difference.
    #[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
    fn seal_under(
        mut self,
        writer: Writer,
        extents: Extents,
    ) -> Result<(SealedFacts, Writer), SealFailure> {
        if !self.writable {
            // A read-only session's close takes no lane.
            return Err(SealFailure {
                error: BackendError::ReadOnly.into(),
                writer: Some(writer),
            });
        }
        // The checkpoint thread ends and closes its connection first.
        drop(self.background.take());
        let (mut state, poisoned) = match self.state.into_inner() {
            Ok(state) => (state, false),
            Err(error) => (error.into_inner(), true),
        };
        // The caller's lane becomes this connection's owner while it is open.
        // An error exit below closes the connection under it (`hand_back`) and
        // returns it with the error: left with the caller while `state` still
        // held an open connection, the lane would be waited for by the very
        // call that holds it, and the seal would never return.
        state.writer_hold = Some(writer);
        if poisoned || state.quarantined || !state.connection.is_autocommit() {
            return Err(hand_back(&mut state, PersistenceError::Uncertain));
        }
        #[cfg(target_os = "macos")]
        let identity = match super::seal_allocation::Identity::capture(&self.path) {
            Ok(identity) => identity,
            Err(error) => return Err(hand_back(&mut state, error)),
        };
        #[cfg(target_os = "macos")]
        if let Err(error) = super::file_control::disable_persistent_wal(&state.connection) {
            return Err(hand_back(&mut state, error.into()));
        }
        let checkpointed = query::run(
            &state.connection,
            "PRAGMA wal_checkpoint(TRUNCATE)",
            vec![],
            &state.work,
        )
        .map_err(PersistenceError::from)
        .and_then(|result| {
            let row = result.first().ok_or(PersistenceError::Malformed)?;
            match row.get::<i64>(0)? {
                0 => Ok(()),
                _ => Err(PersistenceError::Busy),
            }
        });
        if let Err(error) = checkpointed {
            return Err(hand_back(&mut state, error));
        }
        let Some(connection) = state.connection.take() else {
            return Err(hand_back(&mut state, PersistenceError::Malformed));
        };
        if let Err((connection, error)) = connection.close() {
            state.connection.restore(connection);
            return Err(hand_back(&mut state, rows::error(error).into()));
        }
        // The connection is closed; the lane goes back to the caller.
        let Some(writer) = state.writer_hold.take() else {
            return Err(SealFailure {
                error: PersistenceError::Uncertain,
                writer: None,
            });
        };
        let refused = |error: PersistenceError, writer: Writer| SealFailure {
            error,
            writer: Some(writer),
        };
        // SQLite has checked-closed before advisory ownership is released.
        // Earlier error exits close State.connection before handing back its
        // retained owner. The sidecar check below runs while the lane is still
        // held: writable openers, and read-only openers that find no `-shm`
        // (`open_admitted_read_only`), wait on this lane before their first
        // read creates the sidecars, so releasing first would let them land
        // between the close and this check and turn a sealed Store into a
        // refused one. A read-only opener that saw `-shm` just before this
        // close removed it can still create empty sidecars without the lane;
        // the compaction's re-check before its rename catches those.
        for suffix in ["-wal", "-shm", "-journal"] {
            let mut sidecar = self.path.as_os_str().to_os_string();
            sidecar.push(suffix);
            match std::fs::symlink_metadata(sidecar) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Ok(_) => {
                    let status = "Store sidecar remains after seal".into();
                    return Err(refused(PersistenceError::Refused { status }, writer));
                }
                Err(error) => {
                    let status = error.to_string();
                    return Err(refused(PersistenceError::Refused { status }, writer));
                }
            }
        }
        #[cfg(target_os = "macos")]
        if matches!(extents, Extents::Release) {
            if let Err(error) = super::seal_allocation::release(&self.path, identity) {
                return Err(refused(error, writer));
            }
        }
        let facts = (|| {
            let refused = |error: std::io::Error| PersistenceError::Refused {
                status: error.to_string(),
            };
            let metadata = std::fs::metadata(&self.path).map_err(refused)?;
            let modified = metadata.modified().map_err(refused)?;
            let change_counter = read_change_counter(&self.path)?;
            let identity = FileIdentity::capture(&self.path).map_err(refused)?;
            Ok(SealedFacts {
                path: self.path.clone(),
                profile: self.profile.persistence,
                sqlite_version: self.profile.sqlite_version.clone(),
                bytes: metadata.len(),
                modified,
                change_counter,
                identity,
            })
        })();
        match facts {
            Ok(facts) => Ok((facts, writer)),
            Err(error) => Err(refused(error, writer)),
        }
    }
}
