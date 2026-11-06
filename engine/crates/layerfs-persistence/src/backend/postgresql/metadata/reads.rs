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
    let keys: Vec<i64> = ids
        .iter()
        .map(|id| ObjectId::from_bytes(id).map(|id| super::super::objects::publish::prefix(&id)))
        .collect::<Result<_, _>>()
        .map_err(|_| BackendError::Integrity)?;
    // Each id finds its rows through the prefix index, then the full id is
    // compared (#90).
    let sql = format!(
        "SELECT o.object_id,o.role::bigint,o.canonical_length::bigint,o.group_number::bigint,o.record_number::bigint,p.pack_id,p.domain,p.digest,p.length \
         FROM unnest($1::bytea[],$2::bigint[]) k(i,k) JOIN {s}.object_location o \
         ON ('x'||encode(substring(o.object_id FROM 1 FOR 8),'hex'))::bit(64)::bigint=k.k AND o.object_id=k.i \
         JOIN {s}.pack p ON p.pack_id=o.pack_id",
        s = tx.schema
    );
    let rows = tx.typed(&sql, &[&ids, &keys], ids.len() as u64 * 40)?;
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

/// Every stored value among ascending distinct ordinals, in ordinal order,
/// in one statement that names the schema. Each run of consecutive ordinals
/// seeks the last block starting at or before it and takes the blocks from
/// there to its end; each block is returned once, in ordinal order.
pub(crate) fn inode_values(
    tx: &Transaction<'_>,
    ordinals: &[u32],
    out: &mut Vec<InodeValueRow>,
) -> Result<(), PersistenceError> {
    let (mut lows, mut highs) = (Vec::new(), Vec::new());
    for &ordinal in ordinals {
        match highs.last_mut() {
            Some(high) if *high + 1 == i64::from(ordinal) => *high += 1,
            _ => {
                lows.push(i64::from(ordinal));
                highs.push(i64::from(ordinal));
            }
        }
    }
    let sql = format!(
        "SELECT b.first_ordinal,b.last_ordinal,b.body FROM {s}.inode_value_block b WHERE b.first_ordinal IN (\
         SELECT x.first_ordinal FROM unnest($1::bigint[],$2::bigint[]) r(lo,hi) JOIN {s}.inode_value_block x \
         ON x.first_ordinal BETWEEN coalesce((SELECT max(y.first_ordinal) FROM {s}.inode_value_block y \
         WHERE y.first_ordinal<=r.lo),0) AND r.hi) ORDER BY b.first_ordinal",
        s = tx.schema
    );
    let rows = tx.typed(&sql, &[&lows, &highs], lows.len() as u64 * 16)?;
    let ordinal = |v: i64| u32::try_from(v).map_err(|_| BackendError::Integrity);
    crate::blocks::values::pick(
        ordinals,
        rows.iter().map(|row| {
            let first: i64 = row.try_get(0).map_err(|_| BackendError::Integrity)?;
            let last: i64 = row.try_get(1).map_err(|_| BackendError::Integrity)?;
            let body: Vec<u8> = row.try_get(2).map_err(|_| BackendError::Integrity)?;
            Ok((ordinal(first)?, ordinal(last)?, body))
        }),
        out,
    )?;
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
