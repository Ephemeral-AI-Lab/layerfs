//! Acknowledged pack/ordinal blocks in one bounded transaction.
use crate::backend::{
    records::{BackendError, Param},
    Transaction,
};
pub(crate) fn cursor(tx: &Transaction<'_>) -> Result<(i64, i64), BackendError> {
    let rows = tx.query(
        "SELECT next_pack_id,next_ordinal FROM store_policy WHERE id=1",
        vec![],
    )?;
    let r = rows.first().ok_or(BackendError::Integrity)?;
    Ok((r.get(0)?, r.get(1)?))
}
pub(crate) fn advance(tx: &Transaction<'_>, pack: i64, ordinal: i64) -> Result<(), BackendError> {
    tx.query(
        "UPDATE store_policy SET next_pack_id=?1,next_ordinal=?2 WHERE id=1",
        vec![Param::I64(pack), Param::I64(ordinal)],
    )?;
    Ok(())
}
