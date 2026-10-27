//! Common policy validation; schema ownership stays in the backend.
use crate::backend::{metadata_policy, Transaction};
use layerfs_storage::{port::PersistenceError, StoragePolicy};
pub(crate) fn read(tx: &Transaction<'_>) -> Result<StoragePolicy, PersistenceError> {
    metadata_policy::read(tx)?
        .validated()
        .map_err(|_| PersistenceError::Malformed)
}
