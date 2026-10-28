//! Bounded signature ring, insertion-stamp order preserved.
use crate::backend::{records::BackendError, Transaction};
use layerfs_content::ObjectId;
use layerfs_storage::{location::SignatureRow, port::PersistenceError};
pub(crate) fn read(
    tx: &Transaction<'_>,
    out: &mut Vec<SignatureRow>,
) -> Result<(), PersistenceError> {
    // Slot order is the primary key's: no temporary sort over the ring
    // (#352 record 034 S1-a). Rows are put in insertion (stamp) order here;
    // the schema ties each stamp to its slot, and the reader still refuses a
    // stamp that does not rise.
    let rows = tx.mapped(
        "SELECT slot,stamp,object_id,signature FROM content_signature ORDER BY slot LIMIT 8193",
        &[],
        0,
        8193,
        |r| {
            let number = |index| {
                r.get::<_, i64>(index)
                    .map_err(super::rows::error)
                    .and_then(|value| u64::try_from(value).map_err(|_| BackendError::Integrity))
            };
            Ok(SignatureRow {
                slot: usize::try_from(number(0)?).map_err(|_| BackendError::Integrity)?,
                stamp: number(1)?,
                object_id: ObjectId::from_bytes(super::rows::blob(r, 2)?)
                    .map_err(|_| BackendError::Integrity)?,
                signature: super::rows::blob(r, 3)?
                    .try_into()
                    .map_err(|_| BackendError::Integrity)?,
            })
        },
    )?;
    if rows.len() > 8192 {
        return Err(PersistenceError::Malformed);
    }
    out.extend(rows);
    out.sort_unstable_by_key(|row| row.stamp);
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
