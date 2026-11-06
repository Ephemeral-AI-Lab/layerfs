//! Shared acknowledged allocation range arithmetic; no id is inferred after failure.
use crate::backend::{records::BackendError, Mechanics};
use layerfs_storage::{policy, port::*};
pub(crate) fn reserve<T: Mechanics + ?Sized>(
    tx: &T,
    request: Reserve,
) -> Result<Reserved, PersistenceError> {
    if request.packs > policy::TRANSACTION_ROW_LIMIT as usize
        || request.ordinals > policy::METADATA_INDEX_VALUES
    {
        return Err(PersistenceError::Malformed);
    }
    let (pack, ordinal) = tx.allocate(request.packs as i64, request.ordinals as i64)?;
    Ok(Reserved {
        first_pack_id: if request.packs == 0 { 0 } else { pack },
        first_ordinal: if request.ordinals == 0 {
            0
        } else {
            u32::try_from(ordinal).map_err(|_| BackendError::Capacity)?
        },
    })
}
