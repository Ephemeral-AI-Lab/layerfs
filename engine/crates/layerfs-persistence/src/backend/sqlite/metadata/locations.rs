//! Bounded locator reads and immutable pack descriptor decoding.
use crate::backend::{
    error::{BackendError, Param, Record},
    Transaction,
};
use layerfs_content::{ObjectId, ObjectRole};
use layerfs_storage::{
    location::{LocatedObject, ObjectLocation, PackInfo},
    policy,
    port::PersistenceError,
};
use std::collections::BTreeSet;
const LOCATE_PREFIX: &str = "SELECT CASE WHEN (CASE WHEN typeof(o.object_id)='blob' THEN length(CAST(o.object_id AS BLOB)) ELSE NULL END)=32 THEN o.object_id END,o.role,o.canonical_length,o.group_number,o.record_number,p.pack_id,p.domain,CASE WHEN (CASE WHEN typeof(p.digest)='blob' THEN length(CAST(p.digest AS BLOB)) ELSE NULL END)=32 THEN p.digest END,p.length FROM object_location o JOIN pack p ON p.pack_id=o.pack_id WHERE o.object_id IN";
const LOCATE_ONE: &str = "SELECT CASE WHEN (CASE WHEN typeof(o.object_id)='blob' THEN length(CAST(o.object_id AS BLOB)) ELSE NULL END)=32 THEN o.object_id END,o.role,o.canonical_length,o.group_number,o.record_number,p.pack_id,p.domain,CASE WHEN (CASE WHEN typeof(p.digest)='blob' THEN length(CAST(p.digest AS BLOB)) ELSE NULL END)=32 THEN p.digest END,p.length FROM object_location o JOIN pack p ON p.pack_id=o.pack_id WHERE o.object_id IN(?)";
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
    crate::backend::error::pack_info(
        r.get(start)?,
        r.get(start + 1)?,
        &r.get::<Vec<u8>>(start + 2)?,
        r.get(start + 3)?,
    )
}
pub(crate) fn typed_info(r: &rusqlite::Row<'_>, start: usize) -> Result<PackInfo, BackendError> {
    crate::backend::error::pack_info(
        r.get(start)
            .map_err(crate::backend::sqlite::session::rows::error)?,
        r.get(start + 1)
            .map_err(crate::backend::sqlite::session::rows::error)?,
        crate::backend::sqlite::session::rows::blob(r, start + 2)?,
        r.get(start + 3)
            .map_err(crate::backend::sqlite::session::rows::error)?,
    )
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
                    base: None,
                },
                pack,
            });
        }
    }
    Ok(())
}

/// The locators that follow `after` in identity order, one bounded piece.
/// An identity is the table's key, so the piece is one seek and a walk along
/// the key, and an empty key precedes every identity.
const AFTER: &str = "SELECT object_id,pack_id,group_number,record_number,role,canonical_length FROM object_location WHERE object_id>?1 ORDER BY object_id LIMIT ?2";
/// One locator by identity, without its pack's descriptor.
const LOCATOR: &str = "SELECT object_id,pack_id,group_number,record_number,role,canonical_length FROM object_location WHERE object_id=?1";

fn locator(r: &rusqlite::Row<'_>) -> Result<ObjectLocation, BackendError> {
    use crate::backend::sqlite::session::rows::{blob, error};
    let number = |index| {
        usize::try_from(r.get::<_, i64>(index).map_err(error)?).map_err(|_| BackendError::Integrity)
    };
    Ok(ObjectLocation {
        object_id: ObjectId::from_bytes(blob(r, 0)?).map_err(|_| BackendError::Integrity)?,
        pack_id: r.get(1).map_err(error)?,
        group_number: number(2)?,
        record_number: number(3)?,
        role: ObjectRole::from_code(
            u8::try_from(r.get::<_, i64>(4).map_err(error)?)
                .map_err(|_| BackendError::Integrity)?,
        )
        .map_err(|_| BackendError::Integrity)?,
        canonical_length: number(5)?,
        base: None,
    })
}

pub(crate) fn after(
    tx: &Transaction<'_>,
    after: Option<&ObjectId>,
    limit: usize,
    out: &mut Vec<ObjectLocation>,
) -> Result<(), PersistenceError> {
    let key: &[u8] = after.map_or(&[], |id| id.as_bytes().as_slice());
    let count = i64::try_from(limit).map_err(|_| BackendError::Integrity)?;
    let found = tx
        .prepare(AFTER)?
        .mapped_bulk(&[&key, &count], 40, limit, locator)?;
    out.extend(found);
    Ok(())
}

pub(crate) fn one(
    tx: &Transaction<'_>,
    id: &ObjectId,
) -> Result<Option<ObjectLocation>, PersistenceError> {
    let key = id.as_bytes().as_slice();
    Ok(tx.mapped(LOCATOR, &[&key], 32, 1, locator)?.pop())
}

/// One locator by identity with the count of locators between `?1` and it:
/// a seek to the row and a walk along the key from `?1`, which reads no row.
const RANKED: &str = "SELECT object_id,pack_id,group_number,record_number,role,canonical_length,(SELECT count(*) FROM object_location WHERE object_id>?1 AND object_id<?2) FROM object_location WHERE object_id=?2";

pub(crate) fn ranked(
    tx: &Transaction<'_>,
    after: Option<&ObjectId>,
    id: &ObjectId,
) -> Result<crate::backend::Ranked, PersistenceError> {
    use crate::backend::sqlite::session::rows::error;
    let from: &[u8] = after.map_or(&[], |id| id.as_bytes().as_slice());
    let key = id.as_bytes().as_slice();
    let found = tx
        .prepare(RANKED)?
        .mapped_bulk(&[&from, &key], 72, 1, |r| {
            let before: i64 = r.get(6).map_err(error)?;
            Ok((
                u64::try_from(before).map_err(|_| BackendError::Integrity)?,
                locator(r)?,
            ))
        })?
        .pop();
    Ok(match found {
        Some((before, location)) => crate::backend::Ranked::Found(before, location),
        None => crate::backend::Ranked::Absent,
    })
}

fn typed_location(r: &rusqlite::Row<'_>) -> Result<LocatedObject, BackendError> {
    let object_id = ObjectId::from_bytes(crate::backend::sqlite::session::rows::blob(r, 0)?)
        .map_err(|_| BackendError::Integrity)?;
    let pack = typed_info(r, 5)?;
    let role = ObjectRole::from_code(
        u8::try_from(
            r.get::<_, i64>(1)
                .map_err(crate::backend::sqlite::session::rows::error)?,
        )
        .map_err(|_| BackendError::Integrity)?,
    )
    .map_err(|_| BackendError::Integrity)?;
    let number = |index| {
        usize::try_from(
            r.get::<_, i64>(index)
                .map_err(crate::backend::sqlite::session::rows::error)?,
        )
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
            base: None,
        },
        pack,
    })
}
