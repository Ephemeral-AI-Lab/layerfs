//! The SQLite engine behind the shared Metadata, Objects and history seams.
use super::session::transaction::Transaction;
use crate::backend::{
    records::{BackendError, Param, Record},
    HistoryStatements, Mechanics,
};
use layerfs_content::ObjectId;
use layerfs_storage::{
    location::{InodeValueRow, LocatedObject, SignatureRow},
    port::{PersistedPack, PersistenceError},
    StoragePolicy,
};

impl HistoryStatements for Transaction<'_> {
    fn history(&self, key: &str, params: Vec<Param>) -> Result<Vec<Record>, BackendError> {
        Transaction::history(self, key, params)
    }
}

impl Mechanics for Transaction<'_> {
    fn policy(&self) -> Result<StoragePolicy, PersistenceError> {
        super::metadata::policy::read(self)
    }
    fn allocation_cursor(&self) -> Result<(i64, i64), BackendError> {
        super::metadata::allocation::cursor(self)
    }
    fn advance_allocation(&self, pack: i64, ordinal: i64) -> Result<(), BackendError> {
        super::metadata::allocation::advance(self, pack, ordinal)
    }
    fn locate(
        &self,
        ids: &[ObjectId],
        out: &mut Vec<LocatedObject>,
    ) -> Result<(), PersistenceError> {
        super::metadata::locations::read(self, ids, out)
    }
    fn inode_values(
        &self,
        ordinals: &[u32],
        out: &mut Vec<InodeValueRow>,
    ) -> Result<(), PersistenceError> {
        super::metadata::values::read(self, ordinals, out)
    }
    fn signatures(&self, out: &mut Vec<SignatureRow>) -> Result<(), PersistenceError> {
        super::metadata::signatures::read(self, out)
    }
    fn read_packs(
        &self,
        ids: &[i64],
        out: &mut Vec<PersistedPack>,
    ) -> Result<(), PersistenceError> {
        super::objects::read::read(self, ids, out)
    }
}
