//! The content signature ring, read in insertion order.
use super::super::session::transaction::Transaction;
use crate::backend::error::BackendError;
use layerfs_content::ObjectId;
use layerfs_storage::{location::SignatureRow, port::PersistenceError};

/// The signature ring in insertion (stamp) order.
pub(crate) fn signatures(
    tx: &Transaction<'_>,
    out: &mut Vec<SignatureRow>,
) -> Result<(), PersistenceError> {
    let rows = tx.query(
        "SELECT slot,stamp,CASE WHEN octet_length(object_id)=32 THEN object_id END,CASE WHEN octet_length(signature)=32 THEN signature END FROM content_signature ORDER BY slot LIMIT 8193",
        vec![],
    )?;
    if rows.len() > 8192 {
        return Err(PersistenceError::Malformed);
    }
    for r in rows {
        let number =
            |index| u64::try_from(r.get::<i64>(index)?).map_err(|_| BackendError::Integrity);
        out.push(SignatureRow {
            slot: usize::try_from(number(0)?).map_err(|_| BackendError::Integrity)?,
            stamp: number(1)?,
            object_id: ObjectId::from_bytes(&r.get::<Vec<u8>>(2)?)
                .map_err(|_| BackendError::Integrity)?,
            signature: r
                .get::<Vec<u8>>(3)?
                .try_into()
                .map_err(|_| BackendError::Integrity)?,
        });
    }
    out.sort_unstable_by_key(|row| row.stamp);
    Ok(())
}
