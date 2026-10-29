//! The Store engine seam (#506): SQLite always, PostgreSQL behind the
//! `postgresql` feature. Shared Metadata, Objects and history semantics reach
//! an engine only through [`Mechanics`] and [`HistoryStatements`].
pub(crate) mod admission;
#[cfg(feature = "postgresql")]
pub(crate) mod postgresql;
pub(crate) mod records;
pub(crate) mod sqlite;
pub(crate) use sqlite::metadata::locations as metadata_locations;
pub(crate) use sqlite::objects::publish;
pub(crate) use sqlite::session::connection::Session;
pub(crate) use sqlite::session::transaction::Transaction;

use layerfs_content::ObjectId;
use layerfs_storage::{
    location::{InodeValueRow, LocatedObject, SignatureRow},
    port::{PersistedPack, PersistenceError},
    StoragePolicy,
};
use records::{BackendError, Param, Record};
use std::sync::Arc;

/// One opened engine session behind a history provider.
#[derive(Clone)]
pub(crate) enum Engine {
    Sqlite(Arc<Session>),
    #[cfg(feature = "postgresql")]
    Postgres(Arc<postgresql::session::connection::Session>),
}

/// Named history statements in one open engine transaction.
pub(crate) trait HistoryStatements {
    fn history(&self, key: &str, params: Vec<Param>) -> Result<Vec<Record>, BackendError>;
}

/// Engine statements under the shared Metadata and Objects bounds; every
/// call runs inside one caller-owned transaction.
pub(crate) trait Mechanics {
    /// The stored policy row, checked against the engine's schema version.
    fn policy(&self) -> Result<StoragePolicy, PersistenceError>;
    /// Next unreserved pack id and inode-value ordinal.
    fn allocation_cursor(&self) -> Result<(i64, i64), BackendError>;
    fn advance_allocation(&self, pack: i64, ordinal: i64) -> Result<(), BackendError>;
    fn locate(
        &self,
        ids: &[ObjectId],
        out: &mut Vec<LocatedObject>,
    ) -> Result<(), PersistenceError>;
    fn inode_values(
        &self,
        ordinals: &[u32],
        out: &mut Vec<InodeValueRow>,
    ) -> Result<(), PersistenceError>;
    fn signatures(&self, out: &mut Vec<SignatureRow>) -> Result<(), PersistenceError>;
    fn read_packs(&self, ids: &[i64], out: &mut Vec<PersistedPack>)
        -> Result<(), PersistenceError>;
}
