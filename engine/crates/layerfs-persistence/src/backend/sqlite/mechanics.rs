//! The SQLite engine behind the shared Metadata, Objects and history seams.
use super::session::transaction::Transaction;
use crate::backend::{
    error::{BackendError, Param, Record},
    HistoryStatements, Mechanics,
};
use layerfs_content::ObjectId;
use layerfs_storage::{
    location::{InodeValueRow, LocatedObject, ObjectLocation, SignatureRow},
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
    fn locations_after(
        &self,
        after: Option<&ObjectId>,
        limit: usize,
        out: &mut Vec<ObjectLocation>,
    ) -> Result<(), PersistenceError> {
        super::metadata::locations::after(self, after, limit, out)
    }
    fn location(&self, id: &ObjectId) -> Result<Option<ObjectLocation>, PersistenceError> {
        super::metadata::locations::one(self, id)
    }
    fn location_after(
        &self,
        after: Option<&ObjectId>,
        id: &ObjectId,
    ) -> Result<Option<crate::backend::Ranked>, PersistenceError> {
        super::metadata::locations::ranked(self, after, id).map(Some)
    }
    fn closure_in_batches(&self) -> bool {
        // Temporary storage is memory on this connection (`temp_store`), so
        // one recursive statement over the history would hold all of it.
        true
    }
    fn scratch_directory(&self) -> std::path::PathBuf {
        // Beside the Store: the file system that holds the Store and its
        // rebuild has the room, and a temporary directory may be memory.
        self.store_directory()
    }
    fn inode_values(
        &self,
        ordinals: &[u32],
        out: &mut Vec<InodeValueRow>,
    ) -> Result<(), PersistenceError> {
        super::metadata::values::read(self, ordinals, out)
    }
    fn inode_values_ahead(
        &self,
        ordinals: &[u32],
        _ahead: u32,
        out: &mut Vec<InodeValueRow>,
    ) -> Result<(), PersistenceError> {
        // Nothing is read past the asked blocks: only their other values,
        // which the read decoded anyway, come along.
        super::metadata::values::read_blocks(self, ordinals, out)
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
    fn pack_range_fill(&self) -> usize {
        // Group rows are separate BLOBs: a range is mapped to the rows it
        // covers, and nothing between two ranges is free.
        if self.layout().uses_units() {
            0
        } else {
            super::objects::read::RANGE_FILL_BYTES
        }
    }
    fn read_scoped_pack(
        &self,
        id: i64,
        plan: &mut dyn layerfs_storage::port::PackReadPlan,
    ) -> Result<layerfs_storage::port::AcquiredPackRead, PersistenceError> {
        super::objects::selection::read_scoped(self, id, plan)
    }
}
