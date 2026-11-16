//! Sole-owner host seal; never available through the daemon's Store ports.
use crate::backend::sqlite::lane::writer_admission::Writer;
use crate::backend::sqlite::session::connection::Session;
use crate::backend::sqlite::session::query;
use crate::backend::sqlite::session::rows;
use crate::{backend::records::BackendError, SealedStore};
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
    pub(crate) fn seal(self) -> Result<SealedStore, PersistenceError> {
        let writer = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| PersistenceError::Uncertain)?;
            state.writer_hold.take()
        };
        let writer = match writer {
            Some(writer) => writer,
            None => Writer::acquire(&self.path)?,
        };
        let (facts, writer) = self.seal_holding(writer)?;
        writer.release().map_err(|_| PersistenceError::Uncertain)?;
        Ok(facts.into_store())
    }

    /// [`Self::seal`] under a caller-held writer lane (§7 step 1: the guard is
    /// passed into the seal, never acquired a second time). The lane is returned
    /// so a compaction can keep it through the rename. The checkpoint, checked
    /// close, sidecar check and the sealed `(size, mtime, change counter)`
    /// capture all run while the lane is held.
    pub(crate) fn seal_holding(
        mut self,
        writer: Writer,
    ) -> Result<(SealedFacts, Writer), PersistenceError> {
        if !self.writable {
            return Err(BackendError::ReadOnly.into());
        }
        // The checkpoint thread ends and closes its connection first.
        drop(self.background.take());
        let mut state = self
            .state
            .into_inner()
            .map_err(|_| PersistenceError::Uncertain)?;
        if state.quarantined || !state.connection.is_autocommit() {
            return Err(PersistenceError::Uncertain);
        }
        #[cfg(target_os = "macos")]
        let identity = super::seal_allocation::Identity::capture(&self.path)?;
        #[cfg(target_os = "macos")]
        super::file_control::disable_persistent_wal(&state.connection)?;
        let result = query::run(
            &state.connection,
            "PRAGMA wal_checkpoint(TRUNCATE)",
            vec![],
            &state.work,
        )?;
        let row = result.first().ok_or(PersistenceError::Malformed)?;
        if row.get::<i64>(0)? != 0 {
            return Err(PersistenceError::Busy);
        }
        let connection = state.connection.take().ok_or(PersistenceError::Malformed)?;
        if let Err((connection, error)) = connection.close() {
            state.connection.restore(connection);
            return Err(rows::error(error).into());
        }
        // SQLite has checked-closed before advisory ownership is released.
        // Earlier error exits drop State.connection before its retained owner.
        // The sidecar check below runs while the lane is still held: an opener
        // waiting on it takes the same lane before it may create `-wal`, so
        // releasing first would let one land between the close and this check
        // and turn a sealed Store into a refused one.
        for suffix in ["-wal", "-shm", "-journal"] {
            let mut sidecar = self.path.as_os_str().to_os_string();
            sidecar.push(suffix);
            match std::fs::symlink_metadata(sidecar) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Ok(_) => {
                    return Err(PersistenceError::Refused {
                        status: "Store sidecar remains after seal".into(),
                    })
                }
                Err(error) => {
                    return Err(PersistenceError::Refused {
                        status: error.to_string(),
                    })
                }
            }
        }
        #[cfg(target_os = "macos")]
        let released = super::seal_allocation::release(&self.path, identity)?;
        #[cfg(not(target_os = "macos"))]
        let released = 0u64;
        let metadata = std::fs::metadata(&self.path).map_err(|error| PersistenceError::Refused {
            status: error.to_string(),
        })?;
        let modified = metadata
            .modified()
            .map_err(|error| PersistenceError::Refused {
                status: error.to_string(),
            })?;
        let bytes = if released == 0 {
            metadata.len()
        } else {
            released
        };
        let change_counter = read_change_counter(&self.path)?;
        Ok((
            SealedFacts {
                path: self.path.clone(),
                profile: self.profile.persistence,
                sqlite_version: self.profile.sqlite_version.clone(),
                bytes,
                modified,
                change_counter,
            },
            writer,
        ))
    }
}
