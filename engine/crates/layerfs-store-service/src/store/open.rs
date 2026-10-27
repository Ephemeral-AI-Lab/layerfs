//! Opened provider ownership and one reader per mounted Workspace.
use super::{read_service::ReadSet, PortError, ReadServiceWork, ReaderFactory, StoreReader};
use layerfs_content::ObjectId;
use layerfs_history::{HistoryCatalog, WorkspaceId};
use layerfs_storage::{encoding::delta::select::DeltaCounters, save::PoolCounters, WriteOutcome};
use layerfs_storage::{
    port::PackPersistence, ReservationBlocks, Storage, StorageError, StoragePolicy, StorageResult,
};
use layerfs_workspace::{CanonicalCache, CanonicalClient, ClientWork};
use std::sync::{
    atomic::{AtomicU64, Ordering},
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
/// Fixed per-call handles made and reused (#352 F-025 fix 4).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReuseWork {
    /// Workspace readers opened, and SDK private ones reused from the pool.
    pub readers_opened: u64,
    pub readers_reused: u64,
    /// Producer Storage handles made (each reads the policy and hints).
    pub producers: u64,
}
#[derive(Default)]
pub(super) struct Counts {
    pub objects: AtomicU64,
    pub object_ids: AtomicU64,
    pub lengths: AtomicU64,
    pub length_ids: AtomicU64,
    pub serials: AtomicU64,
    pub readers_opened: AtomicU64,
    pub readers_reused: AtomicU64,
    pub producers: AtomicU64,
}
/// Delta selection outcomes of every Save this daemon finished, payload and
/// pooled metadata: stored depths and why candidates were refused.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SaveCounters {
    pub delta: DeltaCounters,
    pub pool: PoolCounters,
    pub merge: MergeCounters,
}
/// How every merge that reached path naming named its changed inodes (#352):
/// from recorded bindings, or by the breadth-first walk (`fallbacks`), with
/// the naming work summed over both sides.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MergeCounters {
    pub merges: u64,
    pub fallbacks: u64,
    /// In-lane steps after the head moved again, the paths they decided,
    /// and those refused as too large (#355).
    pub lane_steps: u64,
    pub lane_paths: u64,
    pub lane_refused: u64,
    pub naming: layerfs_content::compare::NamingWork,
}
/// One global Store binding, with mounted readers and a shared immutable cache.
/// Opened providers must belong to the same validated authority. Application
/// composition owns that selection; this adapter knows only the public ports.
pub struct Store {
    pub(super) writer: Arc<dyn PackPersistence>,
    pub(super) history: Arc<dyn HistoryCatalog>,
    pub(super) readers: Arc<ReadSet>,
    policy: StoragePolicy,
    reservations: ReservationBlocks,
    pub(super) cache: Arc<CanonicalCache>,
    /// Reads objects already in `cache` and nothing else: no reader, no I/O.
    pub(super) resident: Arc<CanonicalClient>,
    pub(super) counts: Arc<Counts>,
    /// Unconsumed local serials below which a create refills early.
    pub(super) serial_low_water: AtomicU64,
    /// The write recognizer's fixed slots, shared by every Workspace.
    pub(super) recognizer: super::Recognizer,
    saves: Mutex<SaveCounters>,
    /// One idle producer handle any Workspace may take (#352 F-025 fix 4).
    pub(super) idle_producer: super::producer::Producer,
    /// Reserved, unhanded serials per scope, `(next, end)` (#352 F-025 fix 4).
    serial_pool: Mutex<std::collections::HashMap<ObjectId, (u64, u64)>>,
    /// Merge build records in the Store file (#355); a Store without them
    /// refuses merges.
    merge_scratch: Option<layerfs_persistence::MergeScratch>,
    /// Merge record work of every merge so far, including writer holds.
    merge_work: Mutex<layerfs_persistence::MergeRecordsWork>,
}
impl Store {
    /// Takes the control reader and a mount-time factory under explicit capacity.
    pub fn new(
        writer: Arc<dyn PackPersistence>,
        history: Arc<dyn HistoryCatalog>,
        control: StoreReader,
        factory: ReaderFactory,
        cache_bytes: usize,
        reservations: ReservationBlocks,
        workspace_capacity: usize,
    ) -> StorageResult<Self> {
        let reservations = reservations.validate()?;
        let policy = writer.policy()?.validated()?;
        if workspace_capacity == 0 || control.storage.policy() != policy {
            return Err(StorageError::Integrity("Store reader capacity or policy"));
        }
        let counts = Arc::new(Counts::default());
        let cache = Arc::new(CanonicalCache::new(cache_bytes));
        let readers = ReadSet::new(control, factory, counts.clone(), workspace_capacity, policy);
        Ok(Self {
            writer,
            history,
            readers,
            policy,
            reservations,
            resident: Arc::new(CanonicalClient::resident(cache.clone())),
            cache,
            counts,
            serial_low_water: AtomicU64::new(0),
            recognizer: super::Recognizer::new(),
            saves: Mutex::default(),
            idle_producer: Default::default(),
            serial_pool: Mutex::default(),
            merge_scratch: None,
            merge_work: Mutex::default(),
        })
    }
    /// Merge build records over the Store's writable session (#355).
    #[must_use]
    pub fn with_merge_scratch(mut self, scratch: layerfs_persistence::MergeScratch) -> Self {
        self.merge_scratch = Some(scratch);
        self
    }
    /// Explicit configuration. Application assembly calls this once, before
    /// any Workspace is served; nothing else in the product calls it. Zero,
    /// the value until then, makes no early attempt.
    pub fn set_serial_low_water(&self, serials: u64) {
        self.serial_low_water.store(serials, Ordering::Relaxed);
    }
    /// Independent mutable producer state over the same opened write provider.
    /// No shared Save or whole-Commit ownership is acquired here.
    pub fn producer(&self) -> StorageResult<Storage> {
        self.counts.producers.fetch_add(1, Ordering::Relaxed);
        Storage::with_reservations(self.writer.clone(), self.reservations)
    }
    /// Runs one merge over fresh build records in the Store file and removes
    /// them when `body` returns. Returns the records' first failure with the
    /// body's result, so a refused merge reports the persistence cause.
    pub fn merge_records<T>(
        &self,
        body: impl FnOnce(&mut layerfs_persistence::MergeRecords) -> T,
    ) -> Result<
        (T, Option<layerfs_storage::port::PersistenceError>),
        layerfs_storage::port::PersistenceError,
    > {
        let scratch = self
            .merge_scratch
            .as_ref()
            .ok_or(layerfs_storage::port::PersistenceError::BackendUnavailable)?;
        let mut records = scratch.begin()?;
        let value = body(&mut records);
        let failure = records.failure().cloned();
        let work = records.end()?;
        if let Ok(mut total) = self.merge_work.lock() {
            total.add(work);
        }
        Ok((value, failure))
    }
    /// Merge record work of every merge so far: jobs and writer holds.
    pub fn merge_record_work(&self) -> layerfs_persistence::MergeRecordsWork {
        self.merge_work
            .lock()
            .map_or_else(|poison| *poison.into_inner(), |work| *work)
    }
    pub const fn policy(&self) -> StoragePolicy {
        self.policy
    }
    pub fn history(&self) -> &dyn HistoryCatalog {
        self.history.as_ref()
    }
    /// One lookup in published history, through the control reader's
    /// read-only session, so it never queues behind a Commit holding the
    /// publication lane (#352 F-018). It sees what is published when it runs,
    /// and the reader's admission, stop and failure custody apply to it.
    pub fn published<T>(
        &self,
        lookup: impl FnOnce(&dyn HistoryCatalog) -> layerfs_history::HistoryResult<T>,
    ) -> Result<T, Arc<PortError>> {
        self.readers
            .session(None)
            .map_err(|error| Arc::new(PortError::ReadAdmission(error)))?
            .published(lookup)
    }
    /// Adds one finished Save's selection counts to the daemon's totals.
    pub fn note_save(&self, saved: &WriteOutcome) {
        if let Ok(mut saves) = self.saves.lock() {
            saves.delta.accumulate(&saved.delta);
            saves.pool.accumulate(&saved.pool);
        }
    }
    /// `count` serials of `scope` from the pool, when it holds that many.
    pub(super) fn pooled_serials(&self, scope: ObjectId, count: u64) -> Option<(u64, u64)> {
        let mut pool = self.serial_pool.lock().ok()?;
        let (next, end) = pool.get_mut(&scope)?;
        if *end - *next < count {
            return None;
        }
        let start = *next;
        *next += count;
        Some((start, count))
    }
    /// Makes `[next, end)` the pool of `scope`; what it held before stays
    /// consumed.
    pub(super) fn pool_serials(&self, scope: ObjectId, next: u64, end: u64) {
        if let Ok(mut pool) = self.serial_pool.lock() {
            pool.insert(scope, (next, end));
        }
    }
    /// Adds one merge's path naming to the totals; a merge that ended before
    /// naming (one side unchanged) adds nothing.
    pub fn note_merge(&self, merged: &layerfs_content::merge::MergeOutcome) {
        let named = merged.naming;
        if !merged.hinted && named.inodes == 0 {
            return;
        }
        if let Ok(mut saves) = self.saves.lock() {
            saves.merge.merges += 1;
            saves.merge.fallbacks += u64::from(!merged.hinted);
            saves.merge.naming.add(&named);
        }
    }
    /// Counts one in-lane merge step: its decided paths, or a refusal.
    pub fn note_lane(&self, paths: Option<usize>) {
        if let Ok(mut saves) = self.saves.lock() {
            match paths {
                Some(paths) => {
                    saves.merge.lane_steps += 1;
                    saves.merge.lane_paths += paths as u64;
                }
                None => saves.merge.lane_refused += 1,
            }
        }
    }
    /// The selection counts of every Save this daemon finished.
    pub fn save_counters(&self) -> SaveCounters {
        self.saves.lock().map(|saves| *saves).unwrap_or_default()
    }
    pub fn read_handles(&self) -> usize {
        self.readers.work().readers
    }
    pub fn cache_work(&self) -> layerfs_content::ContentResult<ClientWork> {
        self.cache.diagnostics()
    }
    pub fn recognizer_work(&self) -> super::RecognizerWork {
        self.recognizer.work()
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
    pub fn reuse_work(&self) -> ReuseWork {
        ReuseWork {
            readers_opened: self.counts.readers_opened.load(Ordering::Relaxed),
            readers_reused: self.counts.readers_reused.load(Ordering::Relaxed),
            producers: self.counts.producers.load(Ordering::Relaxed),
        }
    }
    pub fn mount_reader(&self, workspace: WorkspaceId) -> Result<(), Arc<PortError>> {
        self.readers.mount(workspace, false)
    }
    pub fn close_reader(&self, workspace: WorkspaceId) -> Result<(), Arc<PortError>> {
        self.readers.close(workspace)
    }
    pub(super) fn reader(
        &self,
        workspace: Option<WorkspaceId>,
    ) -> Result<Arc<super::read_handle::ReadSession>, super::ReadAdmissionError> {
        self.readers.session(workspace)
    }
    pub fn read_work(&self) -> ReadServiceWork {
        self.readers.work()
    }
    /// One nonblocking Storage-counter snapshot of an opened reader identity.
    /// A contended or unavailable reader returns None without acquisition.
    pub fn reader_storage_diagnostics(&self, index: usize) -> Option<layerfs_storage::Diagnostics> {
        self.readers.storage_diagnostics(index)
    }
    pub fn reader_failures(&self) -> Vec<(usize, Arc<PortError>)> {
        self.readers.failures()
    }
    /// Whole-Store shutdown only. Workspace unmount must not stop other readers.
    pub fn stop_reads(&self) {
        self.readers.stop();
    }
}
