//! Sole-owner host seal; never available through the daemon's Store ports.
use super::{connection::Session, query, rows};
use crate::{backend::records::BackendError, SealedStore};
use layerfs_storage::port::PersistenceError;

impl Session {
    pub(crate) fn seal(self) -> Result<SealedStore, PersistenceError> {
        if !self.writable {
            return Err(BackendError::ReadOnly.into());
        }
        let state = self
            .state
            .into_inner()
            .map_err(|_| PersistenceError::Uncertain)?;
        if state.quarantined || !state.connection.is_autocommit() {
            return Err(PersistenceError::Uncertain);
        }
        #[cfg(target_os = "macos")]
        super::file_control::disable_persistent_wal(&state.connection)?;
        if self.profile.private_init {
            let result = query::run(
                &state.connection,
                "PRAGMA journal_mode=wal",
                vec![],
                &state.work,
            )?;
            let mode = result
                .first()
                .ok_or(PersistenceError::Malformed)?
                .get::<String>(0)?;
            if mode != "wal" {
                return Err(PersistenceError::Malformed);
            }
        }
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
        state
            .connection
            .close()
            .map_err(|(_, error)| rows::error(error))?;
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
