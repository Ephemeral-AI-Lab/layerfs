//! Opened provider ownership and fixed read capacity.
use layerfs_history::HistoryCatalog;
use layerfs_storage::{
    port::PackPersistence, ReservationBlocks, Storage, StorageError, StoragePolicy, StorageResult,
};
use layerfs_workspace::{CanonicalCache, ClientWork};
use std::sync::{
    atomic::{AtomicU64, AtomicUsize, Ordering},
    Arc, Mutex,
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
    readers: Vec<Mutex<Storage>>,
    next: AtomicUsize,
    policy: StoragePolicy,
    reservations: ReservationBlocks,
    pub(super) cache: Arc<CanonicalCache>,
    pub(super) counts: Counts,
}
impl Store {
    /// Takes already opened providers. Read capacity is fixed for this daemon.
    pub fn new(
        writer: Arc<dyn PackPersistence>,
        history: Arc<dyn HistoryCatalog>,
        readers: Vec<Storage>,
        cache_bytes: usize,
        reservations: ReservationBlocks,
    ) -> StorageResult<Self> {
        let reservations = reservations.validate()?;
        let policy = writer.policy()?.validated()?;
        if readers.is_empty() || readers.iter().any(|read| read.policy() != policy) {
            return Err(StorageError::Integrity("Store read set or policy"));
        }
        Ok(Self {
            writer,
            history,
            readers: readers.into_iter().map(Mutex::new).collect(),
            next: AtomicUsize::new(0),
            policy,
            reservations,
            cache: Arc::new(CanonicalCache::new(cache_bytes)),
            counts: Counts::default(),
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
        self.readers.len()
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
    pub(super) fn read<T>(
        &self,
        read: impl FnOnce(&Storage) -> StorageResult<T>,
    ) -> StorageResult<T> {
        let index = self.next.fetch_add(1, Ordering::Relaxed) % self.readers.len();
        let handle = self.readers[index]
            .lock()
            .map_err(|_| StorageError::Integrity("Store read owner poisoned"))?;
        read(&handle)
    }
}
