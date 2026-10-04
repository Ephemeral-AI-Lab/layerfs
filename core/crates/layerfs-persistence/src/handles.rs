//! Application-owned physical and history handles over one database session.
use crate::{Checkpoint, ConnectionProfile, HistoryProvider, SqlWork, StorageProvider};
use layerfs_storage::port::PersistenceError;
use std::sync::Arc;
/// Store composition with no service bootstrap or implicit authority creation.
pub struct Handles {
    /// Bounded pack publication/read/allocation port.
    pub storage: Arc<StorageProvider>,
    /// History authority validated at explicit open.
    pub history: HistoryProvider,
}
impl Handles {
    /// Read-back settings from the actual shared connection.
    pub fn profile(&self) -> &ConnectionProfile {
        &self.storage.session.profile
    }
    /// Actual SQL, VM, binding, transaction and BLOB counts.
    pub fn diagnostics(&self) -> Result<SqlWork, PersistenceError> {
        self.storage.session.diagnostics().map_err(Into::into)
    }
    /// Explicit checkpoint and unused allocation release; caller times both.
    pub fn checkpoint(&self) -> Result<Checkpoint, PersistenceError> {
        self.storage.session.checkpoint().map_err(Into::into)
    }
}
