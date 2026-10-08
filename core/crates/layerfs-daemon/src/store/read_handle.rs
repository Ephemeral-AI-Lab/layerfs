//! One admitted read-only session and its exact provider failure custody.
use super::{open::Counts, read_service::ReadPool, PortError};
use layerfs_content::ObjectId;
use layerfs_history::{BranchId, BranchSnapshot, HistoryCatalog, WorkspaceId};
use layerfs_storage::Storage;
use std::sync::{atomic::Ordering, Arc};

/// Providers opened together for one read-only Store session. Composition
/// validates their authority and policy before handing them to the Store.
pub struct StoreReader {
    pub(super) storage: Storage,
    pub(super) history: Arc<dyn HistoryCatalog>,
}
impl StoreReader {
    pub fn new(storage: Storage, history: Arc<dyn HistoryCatalog>) -> Self {
        Self { storage, history }
    }
}

/// An exclusively owned idle reader. No pool/provider mutex spans its work.
/// Drop returns it to fair admission, or retains it outside service after an
/// uncertain provider outcome. There is no reopen or failed-demand replay.
pub struct ReadLease {
    pub(super) pool: Arc<ReadPool>,
    pub(super) slot: usize,
    pub(super) index: usize,
    pub(super) reader: Option<Box<StoreReader>>,
    pub(super) failure: Option<Arc<PortError>>,
    pub(super) workspace: Option<WorkspaceId>,
    pub(super) queue_wait_ns: u64,
}
impl ReadLease {
    pub const fn index(&self) -> usize {
        self.index
    }
    pub const fn workspace(&self) -> Option<WorkspaceId> {
        self.workspace
    }
    /// Original admission-to-grant wait; provider service is measured separately.
    pub const fn queue_wait_ns(&self) -> u64 {
        self.queue_wait_ns
    }
    fn attempt<T>(
        &mut self,
        call: impl FnOnce(&StoreReader, &Counts) -> Result<T, PortError>,
    ) -> Result<T, Arc<PortError>> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        call(
            self.reader.as_ref().expect("owned read session"),
            &self.pool.counts,
        )
        .map_err(|error| {
            let original = Arc::new(error);
            self.failure = Some(original.clone());
            original
        })
    }
    /// One original grouped object demand, preserving order and cardinality.
    pub fn objects(&mut self, ids: &[ObjectId]) -> Result<Vec<Vec<u8>>, Arc<PortError>> {
        self.attempt(|handle, counts| {
            counts.objects.fetch_add(1, Ordering::Relaxed);
            counts
                .object_ids
                .fetch_add(ids.len() as u64, Ordering::Relaxed);
            handle
                .storage
                .reader()
                .and_then(|r| r.read_objects(ids))
                .map_err(PortError::Storage)
        })
    }
    pub fn file_lengths(&mut self, ids: &[ObjectId]) -> Result<Vec<u64>, Arc<PortError>> {
        self.attempt(|handle, counts| {
            counts.lengths.fetch_add(1, Ordering::Relaxed);
            counts
                .length_ids
                .fetch_add(ids.len() as u64, Ordering::Relaxed);
            handle
                .storage
                .reader()
                .and_then(|r| r.file_lengths(ids))
                .map_err(PortError::Storage)
        })
    }
    /// Mount snapshots share this read-only session, never the write provider.
    pub fn snapshot(&mut self, branch: BranchId) -> Result<Option<BranchSnapshot>, Arc<PortError>> {
        self.attempt(|handle, _| {
            handle
                .history
                .branch_snapshot(branch)
                .map_err(PortError::History)
        })
    }
}
impl Drop for ReadLease {
    fn drop(&mut self) {
        let Some(reader) = self.reader.take() else {
            return;
        };
        let retired = self
            .failure
            .as_ref()
            .filter(|error| match error.as_ref() {
                PortError::Storage(error) => error.is_unknown_outcome(),
                PortError::History(error) => error.unknown(),
                PortError::Poisoned => true,
                PortError::ConcurrentDemand => false,
                PortError::ReadAdmission(_) => false,
            })
            .cloned()
            .or_else(|| std::thread::panicking().then(|| Arc::new(PortError::Poisoned)));
        self.pool.release(self.slot, self.index, reader, retired);
    }
}
