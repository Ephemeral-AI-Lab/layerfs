//! Bounded locator reads and immutable pack descriptor decoding.
use crate::backend::{
    records::{BackendError, Param, Record},
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
    crate::backend::records::pack_info(
        r.get(start)?,
        r.get(start + 1)?,
        &r.get::<Vec<u8>>(start + 2)?,
        r.get(start + 3)?,
    )
}
pub(crate) fn typed_info(r: &rusqlite::Row<'_>, start: usize) -> Result<PackInfo, BackendError> {
    crate::backend::records::pack_info(
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
    let probe = crate::backend::sqlite::session::profile::probe();
    if probe.locate_typed || probe.locate_bucket || probe.locate_split {
        return probe_read(tx, &ids, out, probe);
    }
    for page in ids.chunks(tx.input_limit(1, 256, 2)?) {
        let sql = locate_sql(page.len());
        let records = layerfs_telemetry::timer::stages::time(
            layerfs_telemetry::timer::stages::Stage::LocateSql,
            || {
                tx.query(
                    &sql,
                    page.iter()
                        .map(|id| Param::Bytes(id.as_bytes().to_vec()))
                        .collect(),
                )
            },
        )?;
        for r in records {
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

const OBJECT_PREFIX: &str = "SELECT CASE WHEN (CASE WHEN typeof(object_id)='blob' THEN length(CAST(object_id AS BLOB)) ELSE NULL END)=32 THEN object_id END,role,canonical_length,group_number,record_number,pack_id FROM object_location WHERE object_id IN";
const PACK_PREFIX: &str = "SELECT pack_id,domain,CASE WHEN (CASE WHEN typeof(digest)='blob' THEN length(CAST(digest AS BLOB)) ELSE NULL END)=32 THEN digest END,length FROM pack WHERE pack_id IN";
fn placeholders(count: usize) -> String {
    std::iter::repeat_n("?", count)
        .collect::<Vec<_>>()
        .join(",")
}
/// Prototype (`LAYERFS_PROBE_SQLITE` `locate=`): multi-id locates decoded in
/// place, optionally padded to a power-of-two id count (repeating the last
/// id, which `IN` ignores) and optionally split into an `object_location`
/// read and one read of each distinct pack descriptor. `ids` is sorted and
/// distinct. Every row passes the same checks as [`typed_location`].
fn probe_read(
    tx: &Transaction<'_>,
    ids: &[ObjectId],
    out: &mut Vec<LocatedObject>,
    probe: crate::backend::sqlite::session::profile::Probe,
) -> Result<(), PersistenceError> {
    use layerfs_telemetry::timer::stages::{time, Stage};
    let limit = tx.input_limit(1, 256, 2)?;
    let padded = |len: usize| match probe.locate_bucket {
        true => len.next_power_of_two().max(16).min(limit),
        false => len,
    };
    for page in ids.chunks(limit) {
        let count = padded(page.len());
        let last = page[page.len() - 1];
        let keys: Vec<&[u8]> = page
            .iter()
            .chain(std::iter::repeat_n(&last, count - page.len()))
            .map(|id| id.as_bytes().as_slice())
            .collect();
        let values: Vec<&dyn rusqlite::ToSql> =
            keys.iter().map(|key| key as &dyn rusqlite::ToSql).collect();
        let bound = 32 * count as u64;
        if !probe.locate_split {
            let sql = locate_sql(count);
            let found = time(Stage::LocateSql, || {
                tx.mapped(&sql, &values, bound, page.len(), typed_location)
            })?;
            out.extend(found);
            continue;
        }
        let sql = format!("{OBJECT_PREFIX}({})", placeholders(count));
        let objects = time(Stage::LocateSql, || {
            tx.mapped(&sql, &values, bound, page.len(), |r| {
                let object_id =
                    ObjectId::from_bytes(crate::backend::sqlite::session::rows::blob(r, 0)?)
                        .map_err(|_| BackendError::Integrity)?;
                let number = |index| {
                    r.get::<_, i64>(index)
                        .map_err(crate::backend::sqlite::session::rows::error)
                };
                Ok((
                    object_id,
                    number(1)?,
                    number(2)?,
                    number(3)?,
                    number(4)?,
                    number(5)?,
                ))
            })
        })?;
        let packs: Vec<i64> = objects
            .iter()
            .map(|row| row.5)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut infos = std::collections::BTreeMap::new();
        if let Some(&last) = packs.last() {
            let count = padded(packs.len());
            let keys: Vec<i64> = packs
                .iter()
                .copied()
                .chain(std::iter::repeat_n(last, count - packs.len()))
                .collect();
            let values: Vec<&dyn rusqlite::ToSql> =
                keys.iter().map(|key| key as &dyn rusqlite::ToSql).collect();
            let sql = format!("{PACK_PREFIX}({})", placeholders(count));
            let found = time(Stage::LocateSql, || {
                tx.mapped(&sql, &values, 8 * count as u64, packs.len(), |r| {
                    typed_info(r, 0)
                })
            })?;
            for info in found {
                infos.insert(info.pack_id, info);
            }
        }
        for (object_id, role, canonical_length, group_number, record_number, pack_id) in objects {
            let pack = *infos.get(&pack_id).ok_or(BackendError::Integrity)?;
            let number = |value: i64| usize::try_from(value).map_err(|_| BackendError::Integrity);
            out.push(LocatedObject {
                location: ObjectLocation {
                    object_id,
                    role: ObjectRole::from_code(
                        u8::try_from(role).map_err(|_| BackendError::Integrity)?,
                    )
                    .map_err(|_| BackendError::Integrity)?,
                    canonical_length: number(canonical_length)?,
                    pack_id: pack.pack_id,
                    group_number: number(group_number)?,
                    record_number: number(record_number)?,
                },
                pack,
            });
        }
    }
    Ok(())
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
        },
        pack,
    })
}
