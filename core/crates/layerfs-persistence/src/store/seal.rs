//! Consuming host handoff of one closed Store file.
use crate::{Handles, SqlitePersistenceProfile};
use layerfs_storage::port::PersistenceError;
use std::{path::PathBuf, sync::Arc};

/// A checked closed Store ready for one explicit installation.
#[derive(Clone, Debug)]
pub struct SealedStore {
    /// Closed database file; no sidecar was present when seal completed.
    pub path: PathBuf,
    /// Provisioning profile to use when opening the installed Store.
    pub profile: SqlitePersistenceProfile,
    /// SQLite version that created/sealed the host file.
    pub sqlite_version: String,
    /// Main-file bytes after the WAL was checkpointed and the connection closed.
    pub bytes: u64,
}

impl Handles {
    /// Consumes the sole session, checkpoints once, closes and verifies one file.
    ///
    /// A retained provider/Storage/Reader/Save refuses with `Busy` before any
    /// checkpoint. Another process can obstruct checkpoint or keep sidecars;
    /// neither case triggers a retry or deletion. Provisioning must prevent
    /// new openers while the sealed file is handed off.
    pub fn seal(self) -> Result<SealedStore, PersistenceError> {
        let Self {
            storage,
            history,
            acquisition,
        } = self;
        drop(history);
        drop(acquisition);
        let storage = Arc::try_unwrap(storage).map_err(|_| PersistenceError::Busy)?;
        let session = Arc::try_unwrap(storage.session).map_err(|_| PersistenceError::Busy)?;
        session.seal()
    }
}
