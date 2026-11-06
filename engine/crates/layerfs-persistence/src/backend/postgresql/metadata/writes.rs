//! The allocation cursor; a publication's rows are written by `objects::publish`.
use super::super::session::transaction::Transaction;
use crate::backend::records::{BackendError, Param};

pub(crate) fn advance(tx: &Transaction<'_>, pack: i64, ordinal: i64) -> Result<(), BackendError> {
    tx.query(
        "UPDATE store_policy SET next_pack_id=$1,next_ordinal=$2 WHERE id=1",
        vec![Param::I64(pack), Param::I64(ordinal)],
    )?;
    Ok(())
}

/// Takes both blocks in one `UPDATE … RETURNING`; the bounds are checked in
/// the statement, so a refused block leaves the cursor unchanged. No row
/// means a bound was reached, or the policy row is gone (read to tell them
/// apart, as the two-statement form does).
pub(crate) fn allocate(
    tx: &Transaction<'_>,
    packs: i64,
    ordinals: i64,
) -> Result<(i64, i64), BackendError> {
    let rows = tx.query(
        "UPDATE store_policy SET next_pack_id=next_pack_id+$1,next_ordinal=next_ordinal+$2 \
         WHERE id=1 AND next_pack_id<=9223372036854775807-$1::bigint AND next_ordinal<=$3::bigint-$2::bigint \
         RETURNING next_pack_id-$1,next_ordinal-$2",
        vec![
            Param::I64(packs),
            Param::I64(ordinals),
            Param::I64(crate::backend::ORDINAL_END),
        ],
    )?;
    match rows.as_slice() {
        [row] => Ok((row.get(0)?, row.get(1)?)),
        [] => {
            super::reads::cursor(tx)?;
            Err(BackendError::Capacity)
        }
        _ => Err(BackendError::Integrity),
    }
}
