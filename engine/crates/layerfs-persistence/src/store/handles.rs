//! Application-owned physical and history handles over one database session.
use crate::{AcquisitionProvider, ConnectionProfile, HistoryProvider, SqlWork, StorageProvider};
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
    /// This host Init maintenance is unavailable through daemon Store ports.
    /// Final seal accounts for the WAL separately within caller timing.
    pub fn reclaim_space(&self, pages: u32) -> Result<crate::SpaceReclamation, PersistenceError> {
        self.storage
            .session
            .reclaim_space(pages)
            .map_err(Into::into)
    }
    /// Closes this session's admission: every call queued for, or later
    /// asking for, the connection fails `Closed` (Storage: `Refused`
    /// "Closed"; history: `OwnershipUnavailable`) without attempting SQL.
    /// The call holding the connection, and its lane, run to completion.
    pub fn close(&self) {
        self.storage.session.admission.close();
    }
    /// Drops this session under the writer lane, so its connection's
    /// close-time checkpoint never runs beside another opener's first
    /// attach. A failed lane acquisition closes bare.
    pub fn shutdown(self) {
        let lane = crate::store_lane(&self.storage.session.path);
        drop(self);
        drop(lane);
    }
    /// Read-back settings from the actual shared connection.
    pub fn profile(&self) -> &ConnectionProfile {
        &self.storage.session.profile
    }
    /// Actual SQL, VM, binding, transaction and BLOB counts.
    pub fn diagnostics(&self) -> Result<SqlWork, PersistenceError> {
        self.storage.session.diagnostics().map_err(Into::into)
    }
}
