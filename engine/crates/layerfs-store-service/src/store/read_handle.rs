//! One Workspace's opened read-only session and exact unknown-outcome custody.
use super::{open::Counts, PortError, ReadAdmissionError};
use layerfs_content::ObjectId;
use layerfs_history::{BranchId, BranchSnapshot, HistoryCatalog};
use layerfs_storage::{Diagnostics, KeptReads, ReadCache, Storage};
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::Instant,
};

pub struct StoreReader {
    pub(super) storage: Storage,
    pub(super) history: Arc<dyn HistoryCatalog>,
    closed: Option<Box<dyn FnOnce() + Send>>,
}
impl StoreReader {
    pub fn new(storage: Storage, history: Arc<dyn HistoryCatalog>) -> Self {
        Self {
            storage,
            history,
            closed: None,
        }
    }
    pub fn on_close(mut self, closed: impl FnOnce() + Send + 'static) -> Self {
        self.closed = Some(Box::new(closed));
        self
    }
}
impl Drop for StoreReader {
    fn drop(&mut self) {
        if let Some(closed) = self.closed.take() {
            closed();
        }
    }
}

struct State {
    reader: Option<StoreReader>,
    /// A Workspace reader's storage caches, kept across its demands for as
    /// long as it is mounted (see [`ReadSession::cached`]).
    cache: Option<KeptReads>,
}
pub(super) struct ReadSession {
    pub index: usize,
    state: Mutex<State>,
    counts: Arc<Counts>,
    failure: OnceLock<Arc<PortError>>,
    pub requests: AtomicU64,
    pub wait_ns: AtomicU64,
}
impl ReadSession {
    pub fn new(index: usize, reader: StoreReader, counts: Arc<Counts>) -> Self {
        Self {
            index,
            state: Mutex::new(State {
                reader: Some(reader),
                cache: None,
            }),
            counts,
            failure: OnceLock::new(),
            requests: AtomicU64::new(0),
            wait_ns: AtomicU64::new(0),
        }
    }
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, State>, Arc<PortError>> {
        self.state.lock().map_err(|_| {
            self.failure
                .get_or_init(|| Arc::new(PortError::Poisoned))
                .clone()
        })
    }

    fn attempt<T>(
        &self,
        call: impl FnOnce(&StoreReader, &mut Option<KeptReads>, &Counts) -> Result<T, PortError>,
    ) -> Result<T, Arc<PortError>> {
        let before = Instant::now();
        let mut state = self.lock()?;
        let State { reader, cache } = &mut *state;
        let waited = before.elapsed();
        layerfs_telemetry::timer::waits::add(
            layerfs_telemetry::timer::waits::Wait::ReadSession,
            waited,
        );
        self.wait_ns.fetch_add(
            waited.as_nanos().min(u64::MAX as u128) as u64,
            Ordering::Relaxed,
        );
        if let Some(error) = self.failure.get() {
            return Err(error.clone());
        }
        let reader = reader
            .as_ref()
            .ok_or_else(|| Arc::new(PortError::ReadAdmission(ReadAdmissionError::Stopped)))?;
        self.requests.fetch_add(1, Ordering::Relaxed);
        call(reader, cache, &self.counts).map_err(|error| {
            let error = Arc::new(error);
            if uncertain(&error) {
                self.failure.get_or_init(|| error).clone()
            } else {
                error
            }
        })
    }
    pub fn objects(&self, ids: &[ObjectId]) -> Result<Vec<Vec<u8>>, Arc<PortError>> {
        self.attempt(|handle, cache, counts| {
            counts.objects.fetch_add(1, Ordering::Relaxed);
            counts
                .object_ids
                .fetch_add(ids.len() as u64, Ordering::Relaxed);
            self.cached(cache, |cache| match cache {
                Some(cache) => handle.storage.read_kept(cache, ids),
                None => handle.storage.reader()?.read_objects(ids),
            })
        })
    }
    /// [`Self::objects`] through a caller's caches; the lock is held for
    /// this one demand only.
    pub fn objects_with(
        &self,
        cache: &mut ReadCache,
        ids: &[ObjectId],
    ) -> Result<Vec<Vec<u8>>, Arc<PortError>> {
        self.attempt(|handle, _, counts| {
            counts.objects.fetch_add(1, Ordering::Relaxed);
            counts
                .object_ids
                .fetch_add(ids.len() as u64, Ordering::Relaxed);
            handle
                .storage
                .read_with(cache, ids)
                .map_err(PortError::Storage)
        })
    }
    pub fn file_lengths(&self, ids: &[ObjectId]) -> Result<Vec<u64>, Arc<PortError>> {
        self.attempt(|handle, cache, counts| {
            counts.lengths.fetch_add(1, Ordering::Relaxed);
            counts
                .length_ids
                .fetch_add(ids.len() as u64, Ordering::Relaxed);
            self.cached(cache, |cache| match cache {
                Some(cache) => handle.storage.lengths_kept(cache, ids),
                None => handle.storage.reader()?.file_lengths(ids),
            })
        })
    }
    /// One demand through this session's kept caches. A Workspace reader
    /// keeps one [`KeptReads`] from its first demand until it is released,
    /// so a later demand reuses the pack bodies, decoded groups and pooled
    /// values an earlier one read instead of a fresh reader's. Its rows never
    /// change once visible and every object is authenticated as before; it
    /// keeps only what was read, never an absence. It is kept only after a
    /// demand that succeeded and left at most [`KeptReads::ORDINARY_BYTES`]
    /// (3 MiB; each demand's decode arena is its own, as before); otherwise
    /// it is dropped and the
    /// next demand starts empty. Session 0 is the Store's control reader
    /// (`ReadSet::new`), which serves every SDK call and Store-level
    /// operation: it keeps none, and each of its demands reads through a
    /// fresh reader as before.
    fn cached<T>(
        &self,
        slot: &mut Option<KeptReads>,
        read: impl FnOnce(Option<&mut KeptReads>) -> layerfs_storage::StorageResult<T>,
    ) -> Result<T, PortError> {
        if self.index == 0 {
            return read(None).map_err(PortError::Storage);
        }
        let cache = slot.get_or_insert_with(KeptReads::new);
        let result = read(Some(&mut *cache));
        if result.is_err() || cache.retained_bytes() > KeptReads::ORDINARY_BYTES {
            *slot = None;
        }
        result.map_err(PortError::Storage)
    }
    pub fn snapshot(&self, branch: BranchId) -> Result<Option<BranchSnapshot>, Arc<PortError>> {
        self.attempt(|handle, _, _| {
            handle
                .history
                .branch_snapshot(branch)
                .map_err(PortError::History)
        })
    }
    /// One lookup in this reader's read-only history.
    pub fn published<T>(
        &self,
        lookup: impl FnOnce(&dyn HistoryCatalog) -> layerfs_history::HistoryResult<T>,
    ) -> Result<T, Arc<PortError>> {
        self.attempt(|handle, _, _| lookup(handle.history.as_ref()).map_err(PortError::History))
    }
    pub fn close(&self) -> Result<(), Arc<PortError>> {
        self.release().map(drop)
    }
    /// Closes this session and hands back its reader, which served without
    /// a failure; a failed or poisoned session keeps its cause and gives none.
    pub fn release(&self) -> Result<Option<StoreReader>, Arc<PortError>> {
        let mut state = self.lock()?;
        if let Some(cause) = self.failure.get() {
            return Err(cause.clone());
        }
        state.cache = None;
        Ok(state.reader.take())
    }
    pub fn quarantined(&self) -> bool {
        self.failure.get().is_some() || self.state.is_poisoned()
    }
    pub fn failure(&self) -> Option<Arc<PortError>> {
        self.failure.get().cloned().or_else(|| {
            self.state.is_poisoned().then(|| {
                self.failure
                    .get_or_init(|| Arc::new(PortError::Poisoned))
                    .clone()
            })
        })
    }
    pub fn diagnostics(&self) -> Option<Diagnostics> {
        self.state
            .try_lock()
            .ok()?
            .reader
            .as_ref()
            .map(|r| r.storage.diagnostics())
    }
}
fn uncertain(error: &PortError) -> bool {
    match error {
        PortError::Storage(error) => error.is_unknown_outcome(),
        PortError::History(error) => error.unknown(),
        PortError::Poisoned => true,
        _ => false,
    }
}
