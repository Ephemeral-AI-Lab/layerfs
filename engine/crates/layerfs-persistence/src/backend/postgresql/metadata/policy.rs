//! The Store policy row, read as the shared `StoragePolicy`.
use super::super::{schema::SCHEMA_VERSION, session::transaction::Transaction};
use crate::backend::error::BackendError;
use layerfs_storage::{port::PersistenceError, StoragePolicy};

pub(crate) fn policy(tx: &Transaction<'_>) -> Result<StoragePolicy, PersistenceError> {
    let rows = tx.query("SELECT format_profile,small_file_threshold_bytes,whole_file_delta_max_depth,chunk_delta_max_depth,metadata_delta_max_depth,schema_version FROM store_policy WHERE id=1", vec![])?;
    let r = rows
        .first()
        .filter(|_| rows.len() == 1)
        .ok_or(PersistenceError::Missing)?;
    let byte = |i| u8::try_from(r.get::<i64>(i)?).map_err(|_| BackendError::Integrity);
    let threshold = u64::try_from(r.get::<i64>(1)?).map_err(|_| BackendError::Integrity)?;
    if r.get::<i64>(5)? != SCHEMA_VERSION {
        return Err(PersistenceError::Malformed);
    }
    StoragePolicy::new(byte(0)?, threshold, byte(2)?, byte(3)?)
        .with_metadata_depth(byte(4)?)
        .validated()
        .map_err(|_| PersistenceError::Malformed)
}
