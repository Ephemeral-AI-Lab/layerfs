//! One operation's original failures over batched public Store demands.
use super::Store;
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult, ObjectId};
use layerfs_history::{HistoryError, ReserveRequest};
use layerfs_storage::StorageError;
use layerfs_workspace::{
    CanonicalClient, FileLengths, InodeSerials, WorkspaceError, WorkspaceResult,
};
use std::{
    fmt,
    sync::{atomic::Ordering, Arc, Mutex},
};

/// Original provider failure, retained independently from shared cache state.
#[derive(Debug)]
pub enum PortError {
    Storage(StorageError),
    History(HistoryError),
    Poisoned,
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
            Self::Poisoned => None,
        }
    }
}
/// Fresh demand custody for one operation, never stored in the immutable cache.
pub struct StorePorts {
    store: Arc<Store>,
    scope: ObjectId,
    failure: Mutex<Option<Arc<PortError>>>,
}
impl Store {
    pub fn ports(self: &Arc<Self>, scope: ObjectId) -> Arc<StorePorts> {
        Arc::new(StorePorts {
            store: self.clone(),
            scope,
            failure: Mutex::new(None),
        })
    }
}
impl StorePorts {
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
    fn attempt<T>(&self, call: impl FnOnce() -> Result<T, PortError>) -> Result<T, Arc<PortError>> {
        let mut first = self
            .failure
            .lock()
            .map_err(|_| Arc::new(PortError::Poisoned))?;
        if let Some(error) = first.as_ref() {
            return Err(error.clone());
        }
        call().map_err(|error| {
            let error = Arc::new(error);
            *first = Some(error.clone());
            error
        })
    }
    /// Bounded grouped metadata demand, with original cardinality/order.
    pub fn file_lengths(&self, ids: &[ObjectId]) -> Result<Vec<u64>, Arc<PortError>> {
        self.attempt(|| {
            self.store.counts.lengths.fetch_add(1, Ordering::Relaxed);
            self.store
                .counts
                .length_ids
                .fetch_add(ids.len() as u64, Ordering::Relaxed);
            self.store
                .read(|handle| handle.reader()?.file_lengths(ids))
                .map_err(PortError::Storage)
        })
    }
}
impl AuthenticatedObjects for StorePorts {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.attempt(|| {
            self.store.counts.objects.fetch_add(1, Ordering::Relaxed);
            self.store
                .counts
                .object_ids
                .fetch_add(ids.len() as u64, Ordering::Relaxed);
            self.store
                .read(|handle| handle.reader()?.read_objects(ids))
                .map_err(PortError::Storage)
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
                .map_err(PortError::History)?;
            if range.scope != self.scope || range.count != count {
                return Err(PortError::History(HistoryError::Integrity(
                    "inode reservation identity",
                )));
            }
            range.end().map_err(PortError::History)?;
            Ok((range.start, range.count))
        })
        .map_err(|e| WorkspaceError::Service(Box::new(e)))
    }
}
