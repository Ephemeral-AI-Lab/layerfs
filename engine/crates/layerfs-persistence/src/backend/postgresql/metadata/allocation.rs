//! The allocation cursor; a publication's rows are written by `objects::publish`.
use super::super::session::transaction::Transaction;
use crate::backend::error::{BackendError, Param};

pub(crate) fn advance(tx: &Transaction<'_>, pack: i64, ordinal: i64) -> Result<(), BackendError> {
    tx.query(
        "UPDATE store_policy SET next_pack_id=$1,next_ordinal=$2 WHERE id=1",
        vec![Param::I64(pack), Param::I64(ordinal)],
    )?;
    Ok(())
}

/// The block allocation's statement ([`allocate`]), which the SDK open
/// prepares (`mount::create_texts`).
pub(crate) const ALLOCATE: &str =
    "UPDATE store_policy SET next_pack_id=next_pack_id+$1,next_ordinal=next_ordinal+$2 \
     WHERE id=1 AND next_pack_id<=9223372036854775807-$1::bigint AND next_ordinal<=$3::bigint-$2::bigint \
     RETURNING next_pack_id-$1,next_ordinal-$2";

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
        ALLOCATE,
        vec![
            Param::I64(packs),
            Param::I64(ordinals),
            Param::I64(crate::backend::ORDINAL_END),
        ],
    )?;
    match rows.as_slice() {
        [row] => Ok((row.get(0)?, row.get(1)?)),
        [] => {
            super::locations::cursor(tx)?;
            Err(BackendError::Capacity)
        }
        _ => Err(BackendError::Integrity),
    }
}
