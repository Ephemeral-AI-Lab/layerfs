//! Bounded Metadata reads; the shared modules check every bound around them.
use super::super::{schema::SCHEMA_VERSION, session::transaction::Transaction};
use crate::backend::records::{pack_info, BackendError, Record};
use layerfs_content::{ObjectId, ObjectRole};
use layerfs_storage::{
    location::{InodeValueRow, LocatedObject, ObjectLocation, SignatureRow},
    port::PersistenceError,
    StoragePolicy,
};
use std::collections::BTreeSet;

pub(crate) fn policy(tx: &Transaction<'_>) -> Result<StoragePolicy, PersistenceError> {
    let rows = tx.query("SELECT format_profile,small_file_threshold_bytes,whole_file_delta_max_depth,chunk_delta_max_depth,metadata_delta_max_depth,schema_version FROM store_policy WHERE id=1", vec![])?;
    let r = rows
        .first()
        .filter(|_| rows.len() == 1)
        .ok_or(PersistenceError::Missing)?;
    let byte = |i| u8::try_from(r.get::<i64>(i)?).map_err(|_| BackendError::Integrity);
    let threshold = u64::try_from(r.get::<i64>(1)?).map_err(|_| BackendError::Integrity)?;
    if r.get::<i64>(5)? != SCHEMA_VERSION {
        return Err(PersistenceError::Malformed);
    }
    StoragePolicy::new(byte(0)?, threshold, byte(2)?, byte(3)?)
        .with_metadata_depth(byte(4)?)
        .validated()
        .map_err(|_| PersistenceError::Malformed)
}

pub(crate) fn cursor(tx: &Transaction<'_>) -> Result<(i64, i64), BackendError> {
    let rows = tx.query(
        "SELECT next_pack_id,next_ordinal FROM store_policy WHERE id=1",
        vec![],
    )?;
    let r = rows.first().ok_or(BackendError::Integrity)?;
    Ok((r.get(0)?, r.get(1)?))
}

/// Every stored location among `ids`, in one statement that names the
/// schema, so it also runs outside a transaction.
pub(crate) fn locate(
    tx: &Transaction<'_>,
    ids: &[ObjectId],
    out: &mut Vec<LocatedObject>,
) -> Result<(), PersistenceError> {
    let ids: Vec<Vec<u8>> = ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|id| id.as_bytes().to_vec())
        .collect();
    let sql = format!(
        "SELECT o.object_id,o.role,o.canonical_length,o.group_number,o.record_number,p.pack_id,p.domain,p.digest,p.length FROM {s}.object_location o JOIN {s}.pack p ON p.pack_id=o.pack_id WHERE o.object_id = ANY($1::bytea[])",
        s = tx.schema
    );
    let rows = tx.typed(&sql, &[&ids], ids.len() as u64 * 32)?;
    for row in rows {
        let r = super::super::session::rows::record(&row)?;
        out.push(location(&r)?);
    }
    Ok(())
}

fn location(r: &Record) -> Result<LocatedObject, BackendError> {
    let object_id =
        ObjectId::from_bytes(&r.get::<Vec<u8>>(0)?).map_err(|_| BackendError::Integrity)?;
    let pack = pack_info(r.get(5)?, r.get(6)?, &r.get::<Vec<u8>>(7)?, r.get(8)?)?;
    let role =
        ObjectRole::from_code(u8::try_from(r.get::<i64>(1)?).map_err(|_| BackendError::Integrity)?)
            .map_err(|_| BackendError::Integrity)?;
    let number = |index| usize::try_from(r.get::<i64>(index)?).map_err(|_| BackendError::Integrity);
    Ok(LocatedObject {
        location: ObjectLocation {
            object_id,
            role,
            canonical_length: number(2)?,
            pack_id: pack.pack_id,
            group_number: number(3)?,
            record_number: number(4)?,
        },
        pack,
    })
}

/// Every stored value among ascending distinct ordinals, in ordinal order.
pub(crate) fn inode_values(
    tx: &Transaction<'_>,
    ordinals: &[u32],
    out: &mut Vec<InodeValueRow>,
) -> Result<(), PersistenceError> {
    let ordinals: Vec<i64> = ordinals.iter().map(|o| i64::from(*o)).collect();
    let sql = format!(
        "SELECT ordinal,value FROM {}.inode_value WHERE ordinal = ANY($1::bigint[]) ORDER BY ordinal",
        tx.schema
    );
    let rows = tx.typed(&sql, &[&ordinals], ordinals.len() as u64 * 8)?;
    for row in rows {
        let r = super::super::session::rows::record(&row)?;
        let ordinal = u32::try_from(r.get::<i64>(0)?).map_err(|_| BackendError::Integrity)?;
        let value = r
            .get::<Vec<u8>>(1)?
            .try_into()
            .map_err(|_| BackendError::Integrity)?;
        out.push(InodeValueRow { ordinal, value });
    }
    Ok(())
}

/// The signature ring in insertion (stamp) order.
pub(crate) fn signatures(
    tx: &Transaction<'_>,
    out: &mut Vec<SignatureRow>,
) -> Result<(), PersistenceError> {
    let rows = tx.query(
        "SELECT slot,stamp,object_id,signature FROM content_signature ORDER BY slot LIMIT 8193",
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
