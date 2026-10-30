//! The PostgreSQL engine behind the shared Metadata, Objects and history seams.
use super::{
    history, metadata, objects,
    session::{rows, transaction::Transaction},
};
use crate::backend::{
    records::{BackendError, Param, Record},
    Column, HistoryStatements, Mechanics,
};
use layerfs_content::ObjectId;
use layerfs_storage::{
    location::{InodeValueRow, LocatedObject, SignatureRow},
    port::{PersistedPack, PersistenceError},
    StoragePolicy,
};
use postgres::types::ToSql;

impl HistoryStatements for Transaction<'_> {
    fn history(&self, key: &str, params: Vec<Param>) -> Result<Vec<Record>, BackendError> {
        self.query(history::statements::statement(key)?, params)
    }
    fn history_arrays(
        &self,
        key: &str,
        columns: Vec<Column>,
    ) -> Option<Result<Vec<Record>, BackendError>> {
        let sql = history::statements::arrays(key)?;
        let mut bytes = 0u64;
        let bound: Vec<Box<dyn ToSql + Sync>> = columns
            .into_iter()
            .map(|column| -> Box<dyn ToSql + Sync> {
                match column {
                    Column::I64(values) => {
                        bytes += 8 * values.len() as u64;
                        Box::new(values)
                    }
                    Column::Bytes(values) => {
                        bytes += values.iter().map(|v| v.len() as u64).sum::<u64>();
                        Box::new(values)
                    }
                }
            })
            .collect();
        let refs: Vec<&(dyn ToSql + Sync)> = bound.iter().map(|b| b.as_ref()).collect();
        Some(
            self.typed(sql, &refs, bytes)
                .and_then(|found| found.iter().map(rows::record).collect()),
        )
    }
}

impl Mechanics for Transaction<'_> {
    fn policy(&self) -> Result<StoragePolicy, PersistenceError> {
        metadata::reads::policy(self)
    }
    fn allocation_cursor(&self) -> Result<(i64, i64), BackendError> {
        metadata::reads::cursor(self)
    }
    fn advance_allocation(&self, pack: i64, ordinal: i64) -> Result<(), BackendError> {
        metadata::writes::advance(self, pack, ordinal)
    }
    fn locate(
        &self,
        ids: &[ObjectId],
        out: &mut Vec<LocatedObject>,
    ) -> Result<(), PersistenceError> {
        metadata::reads::locate(self, ids, out)
    }
    fn inode_values(
        &self,
        ordinals: &[u32],
        out: &mut Vec<InodeValueRow>,
    ) -> Result<(), PersistenceError> {
        metadata::reads::inode_values(self, ordinals, out)
    }
    fn signatures(&self, out: &mut Vec<SignatureRow>) -> Result<(), PersistenceError> {
        metadata::reads::signatures(self, out)
    }
    fn read_packs(
        &self,
        ids: &[i64],
        out: &mut Vec<PersistedPack>,
    ) -> Result<(), PersistenceError> {
        objects::read::read(self, ids, out)
    }
}
