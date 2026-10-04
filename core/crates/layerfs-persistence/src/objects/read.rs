//! Common physical read contract; backend supplies engine rows/BLOB mechanics.
use crate::backend::{objects_read, Transaction};
use layerfs_storage::{policy, port::*};
pub(crate) fn read(
    tx: &Transaction<'_>,
    ids: &[i64],
    out: &mut Vec<PersistedPack>,
) -> Result<(), PersistenceError> {
    if ids.len() > policy::READ_OBJECT_LIMIT || ids.iter().any(|id| *id <= 0) {
        return Err(PersistenceError::Malformed);
    }
    objects_read::read(tx, ids, out)?;
    if out.iter().any(|row| row.info().length != row.body().len()) {
        return Err(PersistenceError::Malformed);
    }
    Ok(())
}
