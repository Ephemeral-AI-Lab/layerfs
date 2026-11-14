//! Sole-owner host seal; never available through the daemon's Store ports.
use crate::backend::sqlite::session::connection::Session;
use crate::backend::sqlite::session::query;
use crate::backend::sqlite::session::rows;
use crate::{backend::records::BackendError, SealedStore};
use layerfs_storage::port::PersistenceError;

impl Session {
    pub(crate) fn seal(mut self) -> Result<SealedStore, PersistenceError> {
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
        // The same advisory lane as a write transaction: a seal is the sole
        // owner's offline handoff, so no other opener may write while the
        // checkpoint, close and sidecar removal below run.
        if state.writer_hold.is_none() {
            state.writer_hold =
                Some(crate::backend::sqlite::lane::writer_admission::Writer::acquire(&self.path)?);
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
        if let Some(writer) = state.writer_hold.take() {
            writer.release().map_err(|_| PersistenceError::Uncertain)?;
        }
        #[cfg(target_os = "macos")]
        let bytes = super::seal_allocation::release(&self.path, identity)?;
        #[cfg(not(target_os = "macos"))]
        let bytes = std::fs::metadata(&self.path)
            .map_err(|error| PersistenceError::Refused {
                status: error.to_string(),
            })?
            .len();
        Ok(SealedStore {
            path: self.path,
            profile: self.profile.persistence,
            sqlite_version: self.profile.sqlite_version,
            bytes,
        })
    }
}
