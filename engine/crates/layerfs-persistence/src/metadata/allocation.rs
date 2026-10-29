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
    let (pack, ordinal) = tx.allocation_cursor()?;
    let end_pack = pack
        .checked_add(request.packs as i64)
        .filter(|v| *v > 0)
        .ok_or(BackendError::Capacity)?;
    let end_ordinal = ordinal
        .checked_add(request.ordinals as i64)
        .filter(|v| *v <= 4294967296)
        .ok_or(BackendError::Capacity)?;
    tx.advance_allocation(end_pack, end_ordinal)?;
    Ok(Reserved {
        first_pack_id: if request.packs == 0 { 0 } else { pack },
        first_ordinal: if request.ordinals == 0 {
            0
        } else {
            u32::try_from(ordinal).map_err(|_| BackendError::Capacity)?
        },
    })
}
