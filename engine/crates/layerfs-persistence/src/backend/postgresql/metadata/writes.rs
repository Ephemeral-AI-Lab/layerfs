//! Metadata rows of one publication, each kind in one array statement.
use super::super::session::transaction::Transaction;
use crate::backend::records::{BackendError, Param};
use layerfs_storage::location::{InodeValueRow, SignatureRow};

pub(crate) fn advance(tx: &Transaction<'_>, pack: i64, ordinal: i64) -> Result<(), BackendError> {
    tx.query(
        "UPDATE store_policy SET next_pack_id=$1,next_ordinal=$2 WHERE id=1",
        vec![Param::I64(pack), Param::I64(ordinal)],
    )?;
    Ok(())
}

/// New values only: an ordinal that already has one fails the transaction.
pub(crate) fn inode_values(
    tx: &Transaction<'_>,
    rows: &[InodeValueRow],
) -> Result<(), BackendError> {
    if rows.is_empty() {
        return Ok(());
    }
    let ordinals: Vec<i64> = rows.iter().map(|r| i64::from(r.ordinal)).collect();
    let values: Vec<&[u8]> = rows.iter().map(|r| r.value.as_slice()).collect();
    tx.typed(
        "INSERT INTO inode_value(ordinal,value) SELECT * FROM unnest($1::bigint[],$2::bytea[])",
        &[&ordinals, &values],
        rows.len() as u64 * 81,
    )?;
    Ok(())
}

/// Ring slots move forward only: an older stamp never replaces a newer one.
pub(crate) fn signatures(tx: &Transaction<'_>, rows: &[SignatureRow]) -> Result<(), BackendError> {
    if rows.is_empty() {
        return Ok(());
    }
    let slots: Vec<i64> = rows.iter().map(|r| r.slot as i64).collect();
    let stamps: Vec<i64> = rows.iter().map(|r| r.stamp as i64).collect();
    let objects: Vec<&[u8]> = rows
        .iter()
        .map(|r| r.object_id.as_bytes().as_slice())
        .collect();
    let signatures: Vec<&[u8]> = rows.iter().map(|r| r.signature.as_slice()).collect();
    tx.typed(
        "INSERT INTO content_signature(slot,stamp,object_id,signature) SELECT * FROM unnest($1::bigint[],$2::bigint[],$3::bytea[],$4::bytea[]) ON CONFLICT(slot) DO UPDATE SET stamp=excluded.stamp,object_id=excluded.object_id,signature=excluded.signature WHERE excluded.stamp>=content_signature.stamp",
        &[&slots, &stamps, &objects, &signatures],
        rows.len() as u64 * 80,
    )?;
    Ok(())
}
