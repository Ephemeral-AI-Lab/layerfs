//! Opened provider ownership and fixed read capacity.
use super::{
    read_service::ReadPool, PortError, ReadAdmissionError, ReadLimits, ReadServiceWork, ReadTicket,
    StoreReader,
};
use layerfs_history::{HistoryCatalog, WorkspaceId};
use layerfs_storage::{
    port::PackPersistence, ReservationBlocks, Storage, StorageError, StoragePolicy, StorageResult,
};
use layerfs_workspace::{CanonicalCache, ClientWork};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

/// Exact adapter demand counts. They do not substitute for provider work receipts.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StoreWork {
    pub object_batches: u64,
    pub object_ids: u64,
    pub length_batches: u64,
    pub length_ids: u64,
    pub serial_reservations: u64,
}
#[derive(Default)]
pub(super) struct Counts {
    pub objects: AtomicU64,
    pub object_ids: AtomicU64,
    pub lengths: AtomicU64,
    pub length_ids: AtomicU64,
    pub serials: AtomicU64,
}
/// One global Store binding, with a fixed read set and shared immutable cache.
/// Opened providers must belong to the same validated authority. Application
/// composition owns that selection; this adapter knows only the public ports.
pub struct Store {
    pub(super) writer: Arc<dyn PackPersistence>,
    pub(super) history: Arc<dyn HistoryCatalog>,
    pub(super) readers: Arc<ReadPool>,
    policy: StoragePolicy,
    reservations: ReservationBlocks,
    pub(super) cache: Arc<CanonicalCache>,
    pub(super) counts: Arc<Counts>,
}
impl Store {
    /// Takes already opened providers. Read capacity is fixed for this daemon.
    pub fn new(
        writer: Arc<dyn PackPersistence>,
        history: Arc<dyn HistoryCatalog>,
        readers: Vec<StoreReader>,
        cache_bytes: usize,
        reservations: ReservationBlocks,
        read_limits: ReadLimits,
    ) -> StorageResult<Self> {
        let reservations = reservations.validate()?;
        let policy = writer.policy()?.validated()?;
        if readers.is_empty() || readers.iter().any(|read| read.storage.policy() != policy) {
            return Err(StorageError::Integrity("Store read set or policy"));
        }
        let counts = Arc::new(Counts::default());
        let readers = ReadPool::new(readers, read_limits, counts.clone())
            .map_err(|_| StorageError::Integrity("Store read admission limits"))?;
        Ok(Self {
            writer,
            history,
            readers,
            policy,
            reservations,
            cache: Arc::new(CanonicalCache::new(cache_bytes)),
            counts,
        })
    }
    /// Independent mutable producer state over the same opened write provider.
    /// No shared Save or whole-Commit ownership is acquired here.
    pub fn producer(&self) -> StorageResult<Storage> {
        Storage::with_reservations(self.writer.clone(), self.reservations)
    }
    pub const fn policy(&self) -> StoragePolicy {
        self.policy
    }
    pub fn history(&self) -> &dyn HistoryCatalog {
        self.history.as_ref()
    }
    pub fn read_handles(&self) -> usize {
        self.readers.work().readers
    }
    pub fn cache_work(&self) -> layerfs_content::ContentResult<ClientWork> {
        self.cache.diagnostics()
    }
    pub fn work(&self) -> StoreWork {
        StoreWork {
            object_batches: self.counts.objects.load(Ordering::Relaxed),
            object_ids: self.counts.object_ids.load(Ordering::Relaxed),
            length_batches: self.counts.lengths.load(Ordering::Relaxed),
            length_ids: self.counts.length_ids.load(Ordering::Relaxed),
            serial_reservations: self.counts.serials.load(Ordering::Relaxed),
        }
    }
    /// Reserve fair before-effect admission. None is the explicitly unscoped
    /// control/constructor lane; native callers use their actual Workspace id.
    pub fn read_ticket(
        &self,
        workspace: Option<WorkspaceId>,
    ) -> Result<ReadTicket, ReadAdmissionError> {
        self.readers.request(workspace)
    }
    pub fn read_work(&self) -> ReadServiceWork {
        self.readers.work()
    }
    pub fn workspace_reads(&self, workspace: WorkspaceId) -> Result<usize, ReadAdmissionError> {
        self.readers.outstanding(workspace)
    }
    pub fn reader_failures(&self) -> Vec<(usize, Arc<PortError>)> {
        self.readers.failures()
    }
    /// Whole-Store shutdown only. Workspace unmount must not stop other readers.
    pub fn stop_reads(&self) {
        self.readers.stop();
    }
}
