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

/// Bounded ordered UPSERT pages share the enclosing publication transaction.
pub(crate) fn write(
    tx: &super::transaction::Transaction<'_>,
    rows: &[SignatureRow],
) -> Result<(), BackendError> {
    if rows.is_empty() {
        return Ok(());
    }
    const PREFIX: &str = "INSERT INTO content_signature(slot,stamp,object_id,signature) VALUES ";
    const SUFFIX: &str = " ON CONFLICT(slot) DO UPDATE SET stamp=excluded.stamp,object_id=excluded.object_id,signature=excluded.signature WHERE excluded.stamp>=content_signature.stamp";
    const ROW: &str = "(?,?,?,?)";
    let limit = tx
        .input_limit(4, PREFIX.len() + SUFFIX.len(), ROW.len() + 1)?
        .min(layerfs_storage::policy::BATCH_OBJECT_LIMIT);
    for page in rows.chunks(limit) {
        let sql = format!(
            "{}{}{}",
            PREFIX,
            std::iter::repeat_n(ROW, page.len())
                .collect::<Vec<_>>()
                .join(","),
            SUFFIX
        );
        // Borrow the immutable blobs; only bounded scalar/binding descriptors
        // are staged, never another signature-body copy.
        let encoded = page
            .iter()
            .map(|s| {
                (
                    s.slot as i64,
                    s.stamp as i64,
                    s.object_id.as_bytes().as_slice(),
                    s.signature.as_slice(),
                )
            })
            .collect::<Vec<_>>();
        let values = encoded
            .iter()
            .flat_map(|r| [&r.0 as &dyn rusqlite::ToSql, &r.1, &r.2, &r.3])
            .collect::<Vec<_>>();
        tx.borrowed(&sql, &values, page.len() as u64 * 80)?;
    }
    Ok(())
}
