//! Common policy validation; schema ownership stays in the backend.
use crate::backend::Mechanics;
use layerfs_storage::{port::PersistenceError, StoragePolicy};
pub(crate) fn read<T: Mechanics + ?Sized>(tx: &T) -> Result<StoragePolicy, PersistenceError> {
    tx.policy()?
        .validated()
        .map_err(|_| PersistenceError::Malformed)
}
