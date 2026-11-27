//! The PostgreSQL engine behind the shared Metadata, Objects and history seams.
use super::{
    history, metadata, objects,
    session::{rows, transaction::Transaction},
};
use crate::backend::error::BackendError;
use crate::backend::seam::{Param, Record};
use crate::backend::{Column, HistoryStatements, Mechanics};
use layerfs_content::ObjectId;
use layerfs_storage::{
    location::{InodeValueRow, LocatedObject, ObjectLocation, SignatureRow},
    port::{PersistedPack, PersistenceError},
    StoragePolicy,
};
use postgres::types::ToSql;

impl HistoryStatements for Transaction<'_> {
    fn mechanics(&self) -> &dyn Mechanics {
        self
    }
    fn history(&self, key: &str, params: Vec<Param>) -> Result<Vec<Record>, BackendError> {
        match history::statements::qualified(key) {
            Some(sql) => self.query(&sql.replace("{s}", self.schema), params),
            None => self.query(history::statements::statement(key)?, params),
        }
    }
    fn history_arrays(
        &self,
        key: &str,
        columns: Vec<Column>,
    ) -> Option<Result<Vec<Record>, BackendError>> {
        // An array statement may name the schema (`{s}`) to run alone.
        let sql = history::statements::arrays(key)?.replace("{s}", self.schema);
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
            self.typed(&sql, &refs, bytes)
                .and_then(|found| found.iter().map(rows::record).collect()),
        )
    }
}

impl Mechanics for Transaction<'_> {
    fn policy(&self) -> Result<StoragePolicy, PersistenceError> {
        metadata::policy::policy(self)
    }
    fn allocation_cursor(&self) -> Result<(i64, i64), BackendError> {
        metadata::allocation::cursor(self)
    }
    fn advance_allocation(&self, pack: i64, ordinal: i64) -> Result<(), BackendError> {
        metadata::allocation::advance(self, pack, ordinal)
    }
    fn allocate(&self, packs: i64, ordinals: i64) -> Result<(i64, i64), BackendError> {
        metadata::allocation::allocate(self, packs, ordinals)
    }
    fn locate(
        &self,
        ids: &[ObjectId],
        out: &mut Vec<LocatedObject>,
    ) -> Result<(), PersistenceError> {
        metadata::locations::locate(self, ids, out)
    }
    fn locations_after(
        &self,
        after: Option<&ObjectId>,
        limit: usize,
        out: &mut Vec<ObjectLocation>,
    ) -> Result<(), PersistenceError> {
        metadata::locations::locations_after(self, after, limit, out)
    }
    fn location(&self, id: &ObjectId) -> Result<Option<ObjectLocation>, PersistenceError> {
        if let Some(held) = metadata::locations::held_location(self, id) {
            return Ok(Some(held));
        }
        let mut found = Vec::with_capacity(1);
        metadata::locations::locate(self, std::slice::from_ref(id), &mut found)?;
        Ok(found
            .first()
            .map(|located| located.location)
            .filter(|location| location.object_id == *id))
    }
    fn expect_locations(&self, ids: &[ObjectId]) -> Result<(), PersistenceError> {
        metadata::locations::expect_locations(self, ids)
    }
    fn batches_locations(&self) -> bool {
        // An autocommit read keeps nothing between its statements.
        self.unfenced()
    }
    fn locators_by_signed_prefix(&self) -> bool {
        true
    }
    fn inode_values(
        &self,
        ordinals: &[u32],
        out: &mut Vec<InodeValueRow>,
    ) -> Result<(), PersistenceError> {
        metadata::values::inode_values(self, ordinals, out)
    }
    fn signatures(&self, out: &mut Vec<SignatureRow>) -> Result<(), PersistenceError> {
        metadata::signatures::signatures(self, out)
    }
    fn read_packs(
        &self,
        ids: &[i64],
        out: &mut Vec<PersistedPack>,
    ) -> Result<(), PersistenceError> {
        objects::read::read(self, ids, out)
    }
    fn pack_prefix(
        &self,
        id: i64,
        limit: usize,
    ) -> Result<Option<(usize, Vec<u8>)>, PersistenceError> {
        objects::read::prefix(self, id, limit)
    }
    fn pack_prefixes(
        &self,
        ids: &[i64],
        limit: usize,
        out: &mut Vec<(usize, Vec<u8>)>,
    ) -> Result<(), PersistenceError> {
        objects::read::prefixes(self, ids, limit, out)
    }
    fn pack_ranges(
        &self,
        requests: &[(i64, Vec<layerfs_storage::port::PackRange>)],
        out: &mut Vec<Vec<Vec<u8>>>,
    ) -> Result<(), PersistenceError> {
        objects::read::ranges(self, requests, out)
    }
    fn read_scoped_pack(
        &self,
        id: i64,
        plan: &mut dyn layerfs_storage::port::PackReadPlan,
    ) -> Result<layerfs_storage::port::AcquiredPackRead, PersistenceError> {
        objects::read::scoped(self, id, plan)
    }
    fn expect_scoped_packs(&self, ids: &[i64]) -> Result<(), PersistenceError> {
        objects::read::expect(self, ids)
    }
    fn inode_values_ahead(
        &self,
        ordinals: &[u32],
        ahead: u32,
        out: &mut Vec<InodeValueRow>,
    ) -> Result<(), PersistenceError> {
        metadata::values::inode_values_ahead(self, ordinals, ahead, out)
    }
}
