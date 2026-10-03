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
pub(crate) fn info(r: &Record, start: usize) -> Result<PackInfo, BackendError> {
    let pack_id = r.get::<i64>(start)?;
    let domain = match r.get::<i64>(start + 1)? {
        0 => PackDomain::Metadata,
        1 => PackDomain::Payload,
        _ => return Err(BackendError::Integrity),
    };
    let digest = r
        .get::<Vec<u8>>(start + 2)?
        .try_into()
        .map_err(|_| BackendError::Integrity)?;
    let length = usize::try_from(r.get::<i64>(start + 3)?).map_err(|_| BackendError::Integrity)?;
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
    let ids: Vec<_> = ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    for page in ids.chunks(tx.variable_limit()?) {
        let sql=format!("SELECT o.object_id,o.role,o.canonical_length,o.group_number,o.record_number,p.pack_id,p.domain,p.digest,p.length FROM object_location o JOIN pack p ON p.pack_id=o.pack_id WHERE o.object_id IN({})",std::iter::repeat_n("?",page.len()).collect::<Vec<_>>().join(","));
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
