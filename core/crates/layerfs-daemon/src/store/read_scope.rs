//! Synchronous canonical algorithms using one already admitted native reader.
use super::{PortError, ReadLease, StorePorts};
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult, ObjectId};
use layerfs_storage::StorageError;
use layerfs_workspace::{
    CanonicalCache, CanonicalClient, FileLengths, WorkspaceError, WorkspaceResult,
};
use std::sync::{Arc, Mutex, TryLockError};

struct Admitted {
    ports: Arc<StorePorts>,
    reader: Mutex<ReadLease>,
}
pub(super) fn client(
    ports: Arc<StorePorts>,
    reader: ReadLease,
    cache: Arc<CanonicalCache>,
) -> Arc<CanonicalClient> {
    let admitted = Arc::new(Admitted {
        ports,
        reader: Mutex::new(reader),
    });
    Arc::new(CanonicalClient::with_cache(
        admitted.clone(),
        Some(admitted),
        cache,
    ))
}
impl Admitted {
    fn demand<T>(
        &self,
        call: impl FnOnce(&mut ReadLease) -> Result<T, Arc<PortError>>,
    ) -> Result<T, Arc<PortError>> {
        let mut reader = self.reader.try_lock().map_err(|error| {
            Arc::new(match error {
                TryLockError::WouldBlock => PortError::ConcurrentDemand,
                TryLockError::Poisoned(_) => PortError::Poisoned,
            })
        })?;
        call(&mut reader)
    }
}
impl AuthenticatedObjects for Admitted {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.demand(|reader| self.ports.objects_on(reader, ids))
            .map_err(|error| match error.as_ref() {
                PortError::Storage(StorageError::ObjectMissing(_)) => ContentError::MissingObject,
                PortError::Storage(StorageError::Content(error)) => error.clone(),
                _ => ContentError::ProviderFailure {
                    what: "admitted Store object demand",
                },
            })
    }
}
impl FileLengths for Admitted {
    fn file_length(&self, id: ObjectId) -> WorkspaceResult<u64> {
        let values = self
            .demand(|reader| self.ports.file_lengths_on(reader, &[id]))
            .map_err(|error| WorkspaceError::Service(Box::new(error)))?;
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
