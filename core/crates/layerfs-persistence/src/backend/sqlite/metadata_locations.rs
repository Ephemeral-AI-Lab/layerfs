//! Bounded locator reads and immutable pack descriptor decoding.
use crate::backend::{
    records::{BackendError, Param, Record},
    Transaction,
};
use layerfs_content::{ObjectId, ObjectRole};
use layerfs_storage::{
    location::{LocatedObject, ObjectLocation, PackDomain, PackInfo},
    policy,
    port::{ObjectKey, PersistenceError},
};
use std::collections::BTreeSet;
const LOCATE_PREFIX: &str = "SELECT o.object_id,o.role,o.canonical_length,o.group_number,o.record_number,p.pack_id,p.domain,p.digest,p.length FROM object_location o JOIN pack p ON p.pack_id=o.pack_id WHERE o.object_id IN";
const LOCATE_ONE: &str = "SELECT o.object_id,o.role,o.canonical_length,o.group_number,o.record_number,p.pack_id,p.domain,p.digest,p.length FROM object_location o JOIN pack p ON p.pack_id=o.pack_id WHERE o.object_id IN(?)";
fn locate_sql(count: usize) -> String {
    format!(
        "{}({})",
        LOCATE_PREFIX,
        std::iter::repeat_n("?", count)
            .collect::<Vec<_>>()
            .join(",")
    )
}

pub(crate) fn info(r: &Record, start: usize) -> Result<PackInfo, BackendError> {
    checked_info(
        r.get(start)?,
        r.get(start + 1)?,
        &r.get::<Vec<u8>>(start + 2)?,
        r.get(start + 3)?,
    )
}
pub(crate) fn typed_info(r: &rusqlite::Row<'_>, start: usize) -> Result<PackInfo, BackendError> {
    checked_info(
        r.get(start).map_err(super::rows::error)?,
        r.get(start + 1).map_err(super::rows::error)?,
        super::rows::blob(r, start + 2)?,
        r.get(start + 3).map_err(super::rows::error)?,
    )
}
fn checked_info(
    pack_id: i64,
    domain: i64,
    digest: &[u8],
    length: i64,
) -> Result<PackInfo, BackendError> {
    let domain = match domain {
        0 => PackDomain::Metadata,
        1 => PackDomain::Payload,
        _ => return Err(BackendError::Integrity),
    };
    let digest = digest.try_into().map_err(|_| BackendError::Integrity)?;
    let length = usize::try_from(length).map_err(|_| BackendError::Integrity)?;
    if pack_id <= 0
        || !(32..=policy::SINGLETON_PACK_LIMIT).contains(&length)
        || domain == PackDomain::Metadata && length > policy::PACK_LIMIT
    {
        return Err(BackendError::Integrity);
    }
    Ok(PackInfo {
        pack_id,
        domain,
        key: ObjectKey::from_bytes(digest),
        length,
    })
}
pub(crate) fn read(
    tx: &Transaction<'_>,
    ids: &[ObjectId],
    out: &mut Vec<LocatedObject>,
) -> Result<(), PersistenceError> {
    if ids.len() > policy::READ_OBJECT_LIMIT {
        return Err(PersistenceError::Malformed);
    }
    if let [id] = ids {
        tx.input_limit(1, 256, 2)?;
        let bytes = id.as_bytes().as_slice();
        let found = tx.mapped(LOCATE_ONE, &[&bytes], 32, 1, typed_location)?;
        out.extend(found);
        return Ok(());
    }
    let ids: Vec<_> = ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    for page in ids.chunks(tx.input_limit(1, 256, 2)?) {
        let sql = locate_sql(page.len());
        for r in tx.query(
            &sql,
            page.iter()
                .map(|id| Param::Bytes(id.as_bytes().to_vec()))
                .collect(),
        )? {
            let object_id = ObjectId::from_bytes(&r.get::<Vec<u8>>(0)?)
                .map_err(|_| PersistenceError::Malformed)?;
            let pack = info(&r, 5)?;
            let role = ObjectRole::from_code(
                u8::try_from(r.get::<i64>(1)?).map_err(|_| BackendError::Integrity)?,
            )
            .map_err(|_| PersistenceError::Malformed)?;
            let canonical_length =
                usize::try_from(r.get::<i64>(2)?).map_err(|_| BackendError::Integrity)?;
            let group_number =
                usize::try_from(r.get::<i64>(3)?).map_err(|_| BackendError::Integrity)?;
            let record_number =
                usize::try_from(r.get::<i64>(4)?).map_err(|_| BackendError::Integrity)?;
            out.push(LocatedObject {
                location: ObjectLocation {
                    object_id,
                    role,
                    canonical_length,
                    pack_id: pack.pack_id,
                    group_number,
                    record_number,
                },
                pack,
            });
        }
    }
    Ok(())
}

fn typed_location(r: &rusqlite::Row<'_>) -> Result<LocatedObject, BackendError> {
    let object_id =
        ObjectId::from_bytes(super::rows::blob(r, 0)?).map_err(|_| BackendError::Integrity)?;
    let pack = typed_info(r, 5)?;
    let role = ObjectRole::from_code(
        u8::try_from(r.get::<_, i64>(1).map_err(super::rows::error)?)
            .map_err(|_| BackendError::Integrity)?,
    )
    .map_err(|_| BackendError::Integrity)?;
    let number = |index| {
        usize::try_from(r.get::<_, i64>(index).map_err(super::rows::error)?)
            .map_err(|_| BackendError::Integrity)
    };
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

pub(crate) fn explain(
    tx: &Transaction<'_>,
    ids: &[ObjectId],
) -> Result<Vec<Vec<String>>, PersistenceError> {
    if ids.len() > policy::READ_OBJECT_LIMIT {
        return Err(PersistenceError::Malformed);
    }
    let ids: Vec<_> = ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut plans = Vec::new();
    for page in ids.chunks(tx.input_limit(1, 256, 2)?) {
        let sql = if page.len() == 1 {
            LOCATE_ONE.to_owned()
        } else {
            locate_sql(page.len())
        };
        let plan = tx
            .query(
                &format!("EXPLAIN QUERY PLAN {sql}"),
                page.iter()
                    .map(|id| Param::Bytes(id.as_bytes().to_vec()))
                    .collect(),
            )?
            .iter()
            .map(|row| row.get::<String>(3))
            .collect::<Result<Vec<_>, _>>()?;
        plans.push(plan);
    }
    Ok(plans)
}
