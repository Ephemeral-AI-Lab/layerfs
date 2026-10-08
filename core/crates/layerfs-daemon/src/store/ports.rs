//! One operation's original failures over batched public Store demands.
use super::{ReadAdmissionError, ReadLease, ReadTicket, Store};
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult, ObjectId};
use layerfs_history::{HistoryError, ReserveRequest, WorkspaceId};
use layerfs_storage::StorageError;
use layerfs_workspace::{
    CanonicalClient, FileLengths, InodeSerials, WorkspaceError, WorkspaceResult,
};
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
/// Fresh demand custody for one operation, never stored in the immutable cache.
pub struct StorePorts {
    store: Arc<Store>,
    scope: ObjectId,
    workspace: Option<WorkspaceId>,
    failure: Mutex<Option<Arc<PortError>>>,
    attempting: AtomicBool,
}
impl Store {
    pub fn ports(self: &Arc<Self>, scope: ObjectId) -> Arc<StorePorts> {
        self.ports_in(scope, None)
    }
    pub fn ports_for(self: &Arc<Self>, scope: ObjectId, workspace: WorkspaceId) -> Arc<StorePorts> {
        self.ports_in(scope, Some(workspace))
    }
    fn ports_in(
        self: &Arc<Self>,
        scope: ObjectId,
        workspace: Option<WorkspaceId>,
    ) -> Arc<StorePorts> {
        Arc::new(StorePorts {
            store: self.clone(),
            scope,
            workspace,
            failure: Mutex::new(None),
            attempting: AtomicBool::new(false),
        })
    }
}
impl StorePorts {
    /// Bind one already admitted reader to the existing canonical client/cache.
    /// Its last client/view consumer returns the reader. No admission or wait
    /// occurs inside these provider calls; a concurrent demand is refused.
    pub fn client_on(
        self: &Arc<Self>,
        reader: ReadLease,
    ) -> Result<Arc<CanonicalClient>, Arc<PortError>> {
        self.check_reader(&reader)?;
        Ok(super::read_scope::client(
            self.clone(),
            reader,
            self.store.cache.clone(),
        ))
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
        self.attempting
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .map_err(|_| Arc::new(PortError::ConcurrentDemand))?;
        let _attempt = Attempt(&self.attempting);
        if let Some(error) = self.failure().map_err(Arc::new)? {
            return Err(error);
        }
        call().inspect_err(|error| {
            // Retain this original failure even if an earlier observer poisoned
            // the mutex. Subsequent access still reports the poison explicitly.
            let mut first = self
                .failure
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            *first = Some(error.clone());
        })
    }
    /// Native callers await admission before executing a provider demand.
    pub fn read_ticket(&self) -> Result<ReadTicket, Arc<PortError>> {
        if let Some(error) = self.failure().map_err(Arc::new)? {
            return Err(error);
        }
        self.store
            .read_ticket(self.workspace)
            .map_err(|e| Arc::new(PortError::ReadAdmission(e)))
    }
    fn check_reader(&self, reader: &ReadLease) -> Result<(), Arc<PortError>> {
        if Arc::ptr_eq(&reader.pool, &self.store.readers) && reader.workspace() == self.workspace {
            Ok(())
        } else {
            Err(Arc::new(PortError::Storage(StorageError::Integrity(
                "foreign Store/Workspace reader",
            ))))
        }
    }
    pub fn objects_on(
        &self,
        reader: &mut ReadLease,
        ids: &[ObjectId],
    ) -> Result<Vec<Vec<u8>>, Arc<PortError>> {
        self.attempt(|| {
            self.check_reader(reader)?;
            reader.objects(ids)
        })
    }
    pub fn file_lengths_on(
        &self,
        reader: &mut ReadLease,
        ids: &[ObjectId],
    ) -> Result<Vec<u64>, Arc<PortError>> {
        self.attempt(|| {
            self.check_reader(reader)?;
            reader.file_lengths(ids)
        })
    }
    /// Bounded grouped metadata demand, with original cardinality/order.
    pub fn file_lengths(&self, ids: &[ObjectId]) -> Result<Vec<u64>, Arc<PortError>> {
        self.attempt(|| {
            let mut reader = self
                .read_ticket()?
                .wait()
                .map_err(|e| Arc::new(PortError::ReadAdmission(e)))?;
            reader.file_lengths(ids)
        })
    }
}
impl AuthenticatedObjects for StorePorts {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.attempt(|| {
            let mut reader = self
                .read_ticket()?
                .wait()
                .map_err(|e| Arc::new(PortError::ReadAdmission(e)))?;
            reader.objects(ids)
        })
        .map_err(|error| match error.as_ref() {
            PortError::Storage(StorageError::ObjectMissing(_)) => ContentError::MissingObject,
            PortError::Storage(StorageError::Content(error)) => error.clone(),
            _ => ContentError::ProviderFailure {
                what: "Store object demand",
            },
        })
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
}
impl InodeSerials for StorePorts {
    fn reserve(&self, count: u64) -> WorkspaceResult<(u64, u64)> {
        self.attempt(|| {
            self.store.counts.serials.fetch_add(1, Ordering::Relaxed);
            let range = self
                .store
                .history
                .reserve_inodes(&ReserveRequest {
                    scope: self.scope,
                    count,
                })
                .map_err(|error| Arc::new(PortError::History(error)))?;
            if range.scope != self.scope || range.count != count {
                return Err(Arc::new(PortError::History(HistoryError::Integrity(
                    "inode reservation identity",
                ))));
            }
            range
                .end()
                .map_err(|error| Arc::new(PortError::History(error)))?;
            Ok((range.start, range.count))
        })
        .map_err(|e| WorkspaceError::Service(Box::new(e)))
    }
}

struct Attempt<'a>(&'a AtomicBool);
impl Drop for Attempt<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
