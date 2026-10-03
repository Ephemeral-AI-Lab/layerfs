//! Bounded signature ring, insertion-stamp order preserved.
use crate::backend::{records::BackendError, Transaction};
use layerfs_content::ObjectId;
use layerfs_storage::{location::SignatureRow, port::PersistenceError};
pub(crate) fn read(
    tx: &Transaction<'_>,
    out: &mut Vec<SignatureRow>,
) -> Result<(), PersistenceError> {
    for r in tx.query(
        "SELECT slot,stamp,object_id,signature FROM content_signature ORDER BY stamp LIMIT 8193",
        vec![],
    )? {
        out.push(SignatureRow {
            slot: usize::try_from(r.get::<i64>(0)?).map_err(|_| BackendError::Integrity)?,
            stamp: u64::try_from(r.get::<i64>(1)?).map_err(|_| BackendError::Integrity)?,
            object_id: ObjectId::from_bytes(&r.get::<Vec<u8>>(2)?)
                .map_err(|_| BackendError::Integrity)?,
            signature: r
                .get::<Vec<u8>>(3)?
                .try_into()
                .map_err(|_| BackendError::Integrity)?,
        });
    }
    if out.len() > 8192 {
        return Err(PersistenceError::Malformed);
    }
    Ok(())
}
