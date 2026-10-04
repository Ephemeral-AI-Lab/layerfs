//! C1's authenticated provider over a two-port Storage handle.

use super::objects::ReadState;
use crate::{error::StorageResult, storage::Storage};
use layerfs_content::{AuthenticatedObjects, ContentResult, ObjectId};
use std::cell::RefCell;

/// One operation's decode arena and bounded group/pooled-value caches.
pub struct Reader<'a> {
    storage: &'a Storage,
    state: RefCell<ReadState>,
}
impl<'a> Reader<'a> {
    pub(crate) fn new(storage: &'a Storage) -> StorageResult<Self> {
        Ok(Self {
            storage,
            state: RefCell::new(ReadState::new()?),
        })
    }
    /// Actual pooled reconstruction work across this operation’s read demands.
    pub fn pooled_read_counters(&self) -> crate::encoding::pool::PoolReadCounters {
        self.state.borrow().pooled_read_counters()
    }
    /// Reconstructs and authenticates a bounded demand, in demand order.
    pub fn read_objects(&self, ids: &[ObjectId]) -> StorageResult<Vec<Vec<u8>>> {
        self.state
            .borrow_mut()
            .read(&self.storage.source, &self.storage.capacities(), ids)
    }
}
impl AuthenticatedObjects for Reader<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.read_objects(ids).map_err(crate::error::provider_error)
    }
}
