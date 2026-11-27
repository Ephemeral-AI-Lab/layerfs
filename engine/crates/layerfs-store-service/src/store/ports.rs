//! One operation's original failures over batched public Store demands.
use super::{read_handle::ReadSession, ReadAdmissionError, Store};
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult, ObjectId};
use layerfs_history::{HistoryError, ReserveRequest, WorkspaceId};
use layerfs_storage::{read::CapturedLocators, StorageError};
use layerfs_vfs::{CanonicalClient, FileLengths, InodeSerials, Workspace, WorkspaceError, WorkspaceResult};
use std::{
    fmt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

/// Original provider failure, retained independently from shared cache state.
#[derive(Debug)]
pub enum PortError {
    Storage(StorageError),
    History(HistoryError),
    Poisoned,
    ConcurrentDemand,
    ReadAdmission(ReadAdmissionError),
}
impl fmt::Display for PortError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Store port: {self:?}")
    }
}
impl PortError {
    /// This failure as an object demand's Content error. A missing object and
    /// a Content failure keep their own errors; every other failure is named
    /// by its kind, so a refusal says which one it was (#508).
    pub fn object_demand(&self) -> ContentError {
        use ReadAdmissionError as Admission;
        let what = match self {
            Self::Storage(StorageError::ObjectMissing(_)) => return ContentError::MissingObject,
            Self::Storage(StorageError::Content(error)) => return error.clone(),
            Self::ConcurrentDemand => "Store object demand: concurrent demand in one operation",
            Self::Poisoned => "Store object demand: operation state poisoned",
            Self::History(_) => "Store object demand: history failure",
            Self::ReadAdmission(Admission::Capacity) => "Store object demand: no reader capacity",
            Self::ReadAdmission(Admission::Stopped) => "Store object demand: Store reader stopped",
            Self::ReadAdmission(Admission::NoReaders) => "Store object demand: no Store readers",
            Self::ReadAdmission(Admission::Poisoned) => "Store object demand: reader poisoned",
            Self::Storage(StorageError::Io(_)) => "Store object demand: storage I/O failure",
            Self::Storage(StorageError::Integrity(_)) => "Store object demand: storage integrity",
            Self::Storage(StorageError::Unpublished(_)) => {
                "Store object demand: unpublished object"
            }
            Self::Storage(StorageError::VisibilityCeiling { .. }) => {
                "Store object demand: beyond the visibility ceiling"
            }
            Self::Storage(StorageError::CapacityExceeded { .. }) => {
                "Store object demand: storage capacity exceeded"
            }
            Self::Storage(_) => "Store object demand: storage failure",
        };
        ContentError::ProviderFailure { what }
    }
}
impl PortError {
    /// Whether this failure is the requested object's own (#818 H5): a
    /// storage failure the storage reader reports per object, exactly the
    /// complement of its whole-demand failures
    /// ([`layerfs_storage::read::call_failure`]: the session, transport,
    /// admission, an unknown outcome). A refused or malformed pack, a missing
    /// dependency, a collision or a bound met by one object is that
    /// object's; anything outside storage fails the demand.
    pub fn object_owned(&self) -> bool {
        match self {
            Self::Storage(error) => !layerfs_storage::read::call_failure(error),
            _ => false,
        }
    }
}
impl std::error::Error for PortError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Storage(e) => Some(e),
            Self::History(e) => Some(e),
            Self::ReadAdmission(e) => Some(e),
            Self::Poisoned | Self::ConcurrentDemand => None,
        }
    }
}
/// One object's read in a per-object demand: its bytes or its own failure.
pub type ObjectRead = Result<Vec<u8>, Arc<PortError>>;
/// One regular file's length in a per-object demand, or its own failure.
pub type LengthRead = Result<u64, Arc<PortError>>;
/// Fresh demand custody for one operation, never stored in the immutable cache.
pub struct StorePorts {
    store: Arc<Store>,
    scope: ObjectId,
    reader: Result<Arc<ReadSession>, Arc<PortError>>,
    captured_reads: Option<Arc<CapturedLocators>>,
    failure: Mutex<Option<Arc<PortError>>>,
    attempting: AtomicBool,
}
impl Store {
    pub fn ports(self: &Arc<Self>, scope: ObjectId) -> Arc<StorePorts> {
        self.ports_in(scope, None, None)
    }
    pub fn ports_for(self: &Arc<Self>, scope: ObjectId, workspace: WorkspaceId) -> Arc<StorePorts> {
        self.ports_in(scope, Some(workspace), None)
    }
    pub(super) fn captured_ports_for(
        self: &Arc<Self>,
        scope: ObjectId,
        workspace: WorkspaceId,
        captured_reads: Option<Arc<CapturedLocators>>,
    ) -> Arc<StorePorts> {
        self.ports_in(scope, Some(workspace), captured_reads)
    }
    fn ports_in(
        self: &Arc<Self>,
        scope: ObjectId,
        workspace: Option<WorkspaceId>,
        captured_reads: Option<Arc<CapturedLocators>>,
    ) -> Arc<StorePorts> {
        Arc::new(StorePorts {
            store: self.clone(),
            scope,
            // the control reader may open here, at its first use (#64)
            reader: match workspace {
                None => self.readers.control(),
                Some(_) => self
                    .reader(workspace)
                    .map_err(|e| Arc::new(PortError::ReadAdmission(e))),
            },
            captured_reads,
            failure: Mutex::new(None),
            attempting: AtomicBool::new(false),
        })
    }
}
impl StorePorts {
    pub fn resident(&self) -> Arc<CanonicalClient> {
        self.store.resident.clone()
    }
    /// The daemon's write-recognizer slots.
    pub fn recognizer(&self) -> &super::Recognizer {
        &self.store.recognizer
    }
    /// Fresh demand custody over the same scope and opened reader. A failure in
    /// it stays there and never becomes this operation's.
    pub fn fresh(&self) -> Arc<Self> {
        Arc::new(Self {
            store: self.store.clone(),
            scope: self.scope,
            reader: self.reader.clone(),
            captured_reads: self.captured_reads.clone(),
            failure: Mutex::new(None),
            attempting: AtomicBool::new(false),
        })
    }
    pub fn client(self: &Arc<Self>) -> Arc<CanonicalClient> {
        Arc::new(CanonicalClient::with_cache(
            self.clone(),
            Some(self.clone()),
            self.store.cache.clone(),
        ))
    }
    /// The first exact failure; poison is itself an explicit refusal.
    pub fn failure(&self) -> Result<Option<Arc<PortError>>, PortError> {
        self.failure
            .lock()
            .map(|first| first.clone())
            .map_err(|_| PortError::Poisoned)
    }
    fn attempt<T>(
        &self,
        call: impl FnOnce() -> Result<T, Arc<PortError>>,
    ) -> Result<T, Arc<PortError>> {
        self.attempt_keeping(call, |_| true)
    }
    /// [`Self::attempt`], retaining as this operation's failure only the
    /// failures `retain` names.
    fn attempt_keeping<T>(
        &self,
        call: impl FnOnce() -> Result<T, Arc<PortError>>,
        retain: impl FnOnce(&PortError) -> bool,
    ) -> Result<T, Arc<PortError>> {
        self.attempting
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .map_err(|_| Arc::new(PortError::ConcurrentDemand))?;
        let _attempt = Attempt(&self.attempting);
        if let Some(error) = self.failure().map_err(Arc::new)? {
            return Err(error);
        }
        call().inspect_err(|error| {
            if !retain(error) {
                return;
            }
            // Retain this original failure even if an earlier observer poisoned
            // the mutex. Subsequent access still reports the poison explicitly.
            let mut first = self
                .failure
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            *first = Some(error.clone());
        })
    }
    /// One unused serial for a create. An early refill below the Store's
    /// low-water is made in a custody scope of its own: its failure is
    /// returned beside the serial and never becomes this operation's.
    pub fn take_serial(
        &self,
        workspace: &Workspace,
    ) -> WorkspaceResult<(u64, Option<WorkspaceError>)> {
        let low_water = self.store.serial_low_water.load(Ordering::Relaxed);
        workspace.next_serial_with_low_water(self, &EarlyRefill(self), low_water)
    }
    fn reader(&self) -> Result<&Arc<ReadSession>, Arc<PortError>> {
        self.store
            .readers
            .admit()
            .map_err(|e| Arc::new(PortError::ReadAdmission(e)))?;
        self.reader.as_ref().map_err(Arc::clone)
    }
    /// Bounded grouped metadata demand, with original cardinality/order.
    pub fn file_lengths(&self, ids: &[ObjectId]) -> Result<Vec<u64>, Arc<PortError>> {
        self.attempt(|| {
            let reader = self.reader()?;
            reader.file_lengths(ids)
        })
    }
}
impl StorePorts {
    /// One object demand through caches the caller keeps across its demands,
    /// under this operation's custody (#352 F-035).
    pub fn objects_with(
        &self,
        cache: &mut layerfs_storage::ReadCache,
        ids: &[ObjectId],
    ) -> Result<Vec<Vec<u8>>, Arc<PortError>> {
        self.attempt(|| self.reader()?.objects_with(cache, ids))
    }
    /// Each object's own result, in request order (#818 H5), in one demand
    /// (`Storage::read_each_with`, the batched read's calls): an object's own
    /// failure ([`PortError::object_owned`]) is returned for it and is not
    /// retained as this operation's failure, so the other objects and the
    /// operation's later demands go on; a failure of the demand, or any other
    /// failure an object meets, fails the call and is retained, as
    /// [`Self::objects_with`] retains it.
    pub fn objects_each_with(
        &self,
        cache: &mut layerfs_storage::ReadCache,
        ids: &[ObjectId],
    ) -> Result<Vec<ObjectRead>, Arc<PortError>> {
        self.attempt(|| each(ids.len(), self.reader()?.objects_each_with(cache, ids)?))
    }
    /// Whether the Store pays a round trip per demand and reads a grouped
    /// demand in one (PostgreSQL; `Store::batched_reads`).
    pub fn batched_reads(&self) -> bool {
        self.store.batched_reads()
    }
    /// Whether [`Self::objects_each_with`] and [`Self::lengths_each_with`]
    /// read their objects in one demand: the storage reader does.
    pub fn reads_each_at_once(&self) -> bool {
        true
    }
    /// [`Self::file_lengths`] through the same caller-kept caches (#818 R4).
    pub fn lengths_with(
        &self,
        cache: &mut layerfs_storage::ReadCache,
        ids: &[ObjectId],
    ) -> Result<Vec<u64>, Arc<PortError>> {
        self.attempt(|| self.reader()?.lengths_with(cache, ids))
    }
    /// Each regular file's length or its own failure (#818 H6), in one
    /// demand (`Storage::lengths_each_with`), kept as
    /// [`Self::objects_each_with`] keeps an object's.
    pub fn lengths_each_with(
        &self,
        cache: &mut layerfs_storage::ReadCache,
        ids: &[ObjectId],
    ) -> Result<Vec<LengthRead>, Arc<PortError>> {
        self.attempt(|| each(ids.len(), self.reader()?.lengths_each_with(cache, ids)?))
    }
}
/// A per-object demand's results for `requested` objects: each object's own
/// failure stays its own; the first failure that is not an object's fails
/// the demand.
fn each<T>(
    requested: usize,
    results: Vec<layerfs_storage::StorageResult<T>>,
) -> Result<Vec<Result<T, Arc<PortError>>>, Arc<PortError>> {
    if results.len() != requested {
        return Err(Arc::new(PortError::Storage(StorageError::Integrity(
            "per-object demand cardinality",
        ))));
    }
    results
        .into_iter()
        .map(|result| match result.map_err(PortError::Storage) {
            Ok(value) => Ok(Ok(value)),
            Err(error) if error.object_owned() => Ok(Err(Arc::new(error))),
            Err(error) => Err(Arc::new(error)),
        })
        .collect()
}
impl AuthenticatedObjects for StorePorts {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.attempt(|| {
            let reader = self.reader()?;
            reader.objects(ids, self.captured_reads.as_ref())
        })
        .map_err(|error| error.object_demand())
    }
}
impl FileLengths for StorePorts {
    fn file_length(&self, id: ObjectId) -> WorkspaceResult<u64> {
        let values = self
            .file_lengths(&[id])
            .map_err(|e| WorkspaceError::Service(Box::new(e)))?;
        if values.len() != 1 {
            return Err(ContentError::BatchCardinality {
                requested: 1,
                returned: values.len(),
            }
            .into());
        }
        Ok(values[0])
    }
    fn file_lengths(&self, ids: &[ObjectId]) -> WorkspaceResult<Vec<u64>> {
        let values = StorePorts::file_lengths(self, ids)
            .map_err(|e| WorkspaceError::Service(Box::new(e)))?;
        if values.len() != ids.len() {
            return Err(ContentError::BatchCardinality {
                requested: ids.len(),
                returned: values.len(),
            }
            .into());
        }
        Ok(values)
    }
}
impl InodeSerials for StorePorts {
    fn reserve(&self, count: u64) -> WorkspaceResult<(u64, u64)> {
        self.reserve_block(count, count)
    }
}
/// Serials one catalog reservation takes for the Store's pool (#352 F-025
/// fix 4): an SDK file call the pool can serve opens no writer transaction.
const POOL_BLOCK: u64 = 4096;
impl StorePorts {
    /// `count` serials of this scope for an SDK file call: from the Store's
    /// pool when it holds them (memory only), else one catalog reservation of
    /// `POOL_BLOCK`, whose rest becomes the pool. Mounted Workspaces keep
    /// their own windows ([`InodeSerials::reserve`]). One attempt; serials
    /// stay consumed whether used or not, as every reservation's do.
    pub fn reserve_pooled(&self, count: u64) -> WorkspaceResult<(u64, u64)> {
        if let Some(range) = self.store.pooled_serials(self.scope, count) {
            return Ok(range);
        }
        self.reserve_block(count, count.max(POOL_BLOCK))
    }
    /// One catalog reservation of `block` serials; the first `count` are the
    /// answer and any rest becomes the Store's pool for this scope.
    fn reserve_block(&self, count: u64, block: u64) -> WorkspaceResult<(u64, u64)> {
        self.attempt(|| {
            self.store.counts.serials.fetch_add(1, Ordering::Relaxed);
            let range = self
                .store
                .history
                .reserve_inodes(&ReserveRequest {
                    scope: self.scope,
                    count: block,
                })
                .map_err(|error| Arc::new(PortError::History(error)))?;
            if range.scope != self.scope || range.count != block {
                return Err(Arc::new(PortError::History(HistoryError::Integrity(
                    "inode reservation identity",
                ))));
            }
            let end = range
                .end()
                .map_err(|error| Arc::new(PortError::History(error)))?;
            if block > count {
                self.store
                    .pool_serials(self.scope, range.start + count, end);
            }
            Ok((range.start, count))
        })
        .map_err(|e| WorkspaceError::Service(Box::new(e)))
    }
}
/// Fresh ports over the same scope, created only when the attempt is made.
struct EarlyRefill<'a>(&'a StorePorts);
impl InodeSerials for EarlyRefill<'_> {
    fn reserve(&self, count: u64) -> WorkspaceResult<(u64, u64)> {
        let ports = self.0;
        ports.fresh().reserve(count)
    }
}

struct Attempt<'a>(&'a AtomicBool);
impl Drop for Attempt<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
