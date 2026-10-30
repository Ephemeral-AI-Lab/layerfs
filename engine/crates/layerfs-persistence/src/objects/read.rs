//! Common physical read contract; backend supplies engine rows/BLOB mechanics.
use crate::backend::Mechanics;
use layerfs_storage::{policy, port::*};
pub(crate) fn read<T: Mechanics + ?Sized>(
    tx: &T,
    ids: &[i64],
    out: &mut Vec<PersistedPack>,
) -> Result<(), PersistenceError> {
    if ids.len() > policy::READ_OBJECT_LIMIT || ids.iter().any(|id| *id <= 0) {
        return Err(PersistenceError::Malformed);
    }
    tx.read_packs(ids, out)?;
    if out.iter().any(|row| row.info().length != row.body().len()) {
        return Err(PersistenceError::Malformed);
    }
    Ok(())
}
