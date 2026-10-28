//! Bounds of one stored-value read, shared by every backend.
use crate::backend::{metadata_values, Transaction};
use layerfs_storage::{location::InodeValueRow, policy, port::*};
pub(crate) fn read(
    tx: &Transaction<'_>,
    ordinals: &[u32],
    out: &mut Vec<InodeValueRow>,
) -> Result<(), PersistenceError> {
    if ordinals.len() > policy::READ_OBJECT_LIMIT
        || ordinals.first() == Some(&0)
        || ordinals.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err(PersistenceError::Malformed);
    }
    metadata_values::read(tx, ordinals, out)?;
    // The reply names only asked ordinals, each once, ascending.
    let mut asked = ordinals.iter();
    for row in out.iter() {
        if !asked.any(|ordinal| *ordinal == row.ordinal) {
            return Err(PersistenceError::Malformed);
        }
    }
    Ok(())
}
