//! The engine seam (#506): the opened engine behind a history provider and
//! the two traits through which shared semantics reach an engine.
use super::records::{BackendError, Param, Record};
use layerfs_content::ObjectId;
use layerfs_storage::{
    location::{InodeValueRow, LocatedObject, SignatureRow},
    port::{PersistedPack, PersistenceError},
    StoragePolicy,
};
use std::sync::Arc;

/// One opened engine session behind a history provider.
#[derive(Clone)]
pub(crate) enum Engine {
    Sqlite(Arc<super::Session>),
    #[cfg(feature = "postgresql")]
    Postgres(Arc<super::postgresql::session::connection::Session>),
}

/// Named history statements in one open engine transaction.
pub(crate) trait HistoryStatements {
    fn history(&self, key: &str, params: Vec<Param>) -> Result<Vec<Record>, BackendError>;
    /// One statement over parallel column arrays, where the engine binds
    /// arrays (PostgreSQL: one round trip for many rows). `None`: the
    /// engine has no such statement and the caller runs its per-row one.
    fn history_arrays(
        &self,
        _key: &str,
        _columns: Vec<Column>,
    ) -> Option<Result<Vec<Record>, BackendError>> {
        None
    }
}

/// One column of an array statement.
pub(crate) enum Column {
    I64(Vec<i64>),
    Bytes(Vec<Vec<u8>>),
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
