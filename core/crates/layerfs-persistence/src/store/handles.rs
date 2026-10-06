//! Application-owned physical and history handles over one database session.
use crate::{
    AcquisitionProvider, Checkpoint, ConnectionProfile, HistoryProvider, SqlWork, StorageProvider,
};
use layerfs_storage::port::PersistenceError;
use std::sync::Arc;
/// Store composition with no service bootstrap or implicit authority creation.
pub struct Handles {
    /// Bounded pack publication/read/allocation port.
    pub storage: Arc<StorageProvider>,
    /// History authority validated at explicit open.
    pub history: HistoryProvider,
    /// Initial-acquisition working state over the same session. Every unit is
    /// refused unless the Store was created with the acquisition tables.
    pub acquisition: AcquisitionProvider,
}
impl Handles {
    /// Reclaims at most `pages` free SQLite pages in one short atomic job.
    ///
    /// New Stores with acquisition tables support this operation. Existing
    /// Stores with auto-vacuum disabled are refused without migration. The
    /// budget must be in `1..=RECLAMATION_PAGE_LIMIT`. A caller may continue
    /// acknowledged jobs while debt remains, releasing the writer between
    /// jobs; a refused or uncertain attempt is never replayed automatically.
    /// Durable jobs retain WAL/FULL/fullfsync. A final checkpoint pays the WAL
    /// and filesystem allocation release separately, within caller timing.
    pub fn reclaim_space(&self, pages: u32) -> Result<crate::SpaceReclamation, PersistenceError> {
        self.storage
            .session
            .reclaim_space(pages)
            .map_err(Into::into)
    }
    /// Read-back settings from the actual shared connection.
    pub fn profile(&self) -> &ConnectionProfile {
        &self.storage.session.profile
    }
    /// Actual SQL, VM, binding, transaction and BLOB counts.
    pub fn diagnostics(&self) -> Result<SqlWork, PersistenceError> {
        self.storage.session.diagnostics().map_err(Into::into)
    }
    /// Explains the owning bounded locator query on this actual initialized DB.
    /// Uses the same distinct-ID paging and SQL as normal saved-object lookup.
    /// This read-only diagnostic neither reads payload nor changes Store policy.
    pub fn explain_locate(
        &self,
        ids: &[layerfs_content::ObjectId],
    ) -> Result<Vec<Vec<String>>, PersistenceError> {
        self.storage.session.run(false, |tx| {
            crate::backend::metadata_locations::explain(tx, ids)
        })
    }
    /// Explains every shipped acquisition statement on this actual initialized
    /// DB, by statement name. Read-only; refused when the Store was created
    /// without the acquisition tables.
    pub fn explain_acquisition(
        &self,
    ) -> Result<Vec<(&'static str, Vec<String>)>, PersistenceError> {
        if self.profile().acquisition != crate::SqliteAcquisitionSchema::Tables {
            return Err(PersistenceError::BackendUnavailable);
        }
        self.storage.session.run(false, |tx| {
            crate::backend::sqlite::acquisition::statements::explain(tx)
                .map_err(PersistenceError::from)
        })
    }
    /// Completes the selected profile and releases unused allocation, within caller timing.
    /// Durable checkpoints WAL; Disposable has no WAL and retains allocation release.
    pub fn checkpoint(&self) -> Result<Checkpoint, PersistenceError> {
        self.storage.session.checkpoint().map_err(Into::into)
    }
}
