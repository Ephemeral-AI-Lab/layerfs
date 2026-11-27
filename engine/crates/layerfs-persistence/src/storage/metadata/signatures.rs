//! Shared signature-ring count/stamp validation.
use crate::backend::Mechanics;
use layerfs_storage::{location::SignatureRow, port::PersistenceError};
pub(crate) fn read<T: Mechanics + ?Sized>(
    tx: &T,
    out: &mut Vec<SignatureRow>,
) -> Result<(), PersistenceError> {
    tx.signatures(out)?;
    if out.len() > 8192
        || out.iter().any(|r| {
            r.slot >= 8192
                || r.stamp == 0
                || r.stamp > i64::MAX as u64
                || (r.stamp - 1) % 8192 != r.slot as u64
        })
    {
        return Err(PersistenceError::Malformed);
    }
    Ok(())
}
