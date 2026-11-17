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
    fn mechanics(&self) -> &dyn Mechanics {
        self
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
    fn pack_prefix(
        &self,
        id: i64,
        limit: usize,
    ) -> Result<Option<(usize, Vec<u8>)>, PersistenceError> {
        super::objects::read::prefix(self, id, limit)
    }
    fn pack_prefixes(
        &self,
        ids: &[i64],
        limit: usize,
        out: &mut Vec<(usize, Vec<u8>)>,
    ) -> Result<(), PersistenceError> {
        super::objects::read::prefixes(self, ids, limit, out)
    }
    fn pack_ranges(
        &self,
        requests: &[(i64, Vec<layerfs_storage::port::PackRange>)],
        out: &mut Vec<Vec<Vec<u8>>>,
    ) -> Result<(), PersistenceError> {
        if self.layout().uses_units() {
            // Group rows are separate BLOBs: the scoped read maps offsets to
            // them, one pack at a time.
            return crate::backend::seam::ranges_by_acquisition(self, requests, out);
        }
        super::objects::read::ranges(self, requests, out)
    }
    fn read_scoped_pack(
        &self,
        id: i64,
        plan: &mut dyn layerfs_storage::port::PackReadPlan,
    ) -> Result<layerfs_storage::port::AcquiredPackRead, PersistenceError> {
        super::objects::selection::read_scoped(self, id, plan)
    }
}
