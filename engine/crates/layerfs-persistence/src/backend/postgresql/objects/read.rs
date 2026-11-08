//! Whole and ranged pack reads; read budgets are checked before any body.
use super::super::session::{rows, transaction::Transaction};
use crate::backend::records::{pack_info, BackendError, Record};
use layerfs_storage::{
    location::PackInfo,
    policy,
    port::{AcquiredPackRead, PackReadPlan, PersistedPack, PersistedPackRead, PersistenceError},
};

fn info(r: &Record) -> Result<PackInfo, BackendError> {
    pack_info(r.get(0)?, r.get(1)?, &r.get::<Vec<u8>>(2)?, r.get(3)?)
}

pub(crate) fn read(
    tx: &Transaction<'_>,
    ids: &[i64],
    out: &mut Vec<PersistedPack>,
) -> Result<(), PersistenceError> {
    let mut ids = ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    let budget = policy::DEPENDENCY_PACK_CACHE_BYTES as i64;
    let singleton = policy::SINGLETON_PACK_LIMIT as i64;
    // Descriptors and bodies in one statement. The server sends the bodies
    // only when every id is present and their total is within the read
    // budget, so the budget is still checked before any body is read.
    let found = tx.typed(
        &format!(
            "WITH d AS (SELECT count(*) AS n, coalesce(sum(length),0) AS total FROM {s}.pack WHERE pack_id = ANY($1::bigint[])) \
             SELECT p.pack_id,p.domain,p.digest,p.length,\
             CASE WHEN d.n = cardinality($1::bigint[]) AND (d.total <= $2::bigint OR (d.n = 1 AND d.total <= $3::bigint)) AND octet_length(p.body)=p.length AND p.length BETWEEN 32 AND $3::bigint THEN p.body END \
             FROM {s}.pack p, d WHERE p.pack_id = ANY($1::bigint[]) ORDER BY p.pack_id",
            s = tx.schema
        ),
        &[&ids, &budget, &singleton],
        ids.len() as u64 * 8 + 16,
    )?;
    if found.len() != ids.len() {
        return Err(PersistenceError::Missing);
    }
    let mut infos = Vec::with_capacity(found.len());
    let mut total = 0usize;
    for row in &found {
        let descriptor = info(&rows::record_first(row, 4)?)?;
        total = total
            .checked_add(descriptor.length)
            .ok_or(BackendError::Capacity)?;
        infos.push(descriptor);
    }
    if total > policy::DEPENDENCY_PACK_CACHE_BYTES
        && !(ids.len() == 1 && total <= policy::SINGLETON_PACK_LIMIT)
    {
        return Err(BackendError::Capacity.into());
    }
    for (info, row) in infos.into_iter().zip(found) {
        let body: Option<Vec<u8>> = row.try_get(4).map_err(|e| rows::error(&e))?;
        out.push(PersistedPack::authenticate(
            info,
            body.ok_or(PersistenceError::Malformed)?,
        )?);
    }
    Ok(())
}

pub(crate) fn selection(
    tx: &Transaction<'_>,
    id: i64,
    plan: &mut dyn PackReadPlan,
) -> Result<PersistedPackRead, PersistenceError> {
    read_with(tx, id, |descriptor, read_at| {
        PersistedPackRead::acquire(descriptor, plan, read_at)
    })
}

pub(crate) fn scoped(
    tx: &Transaction<'_>,
    id: i64,
    plan: &mut dyn PackReadPlan,
) -> Result<AcquiredPackRead, PersistenceError> {
    read_with(tx, id, |descriptor, read_at| {
        AcquiredPackRead::acquire(descriptor, plan, read_at)
    })
}

type OffsetRead<'a> = dyn FnMut(usize, &mut [u8]) -> Result<(), PersistenceError> + 'a;

/// Bytes from the start of a body fetched with its descriptor: the header
/// and directory reads that open every acquisition then cost no round trip,
/// and a pack up to the pack limit, which this provider takes whole
/// ([`whole_pack_bytes`](layerfs_storage::port::PackPersistence::whole_pack_bytes)),
/// arrives complete in that one statement.
const HEAD_BYTES: i32 = policy::PACK_LIMIT as i32;

/// Exact ranges of one body: the body is stored uncompressed out of line
/// (`STORAGE EXTERNAL`), so `substring` fetches only the chunks it covers.
/// The descriptor and the body's head come in one statement; a read the
/// head covers is copied from it, any other is one ranged statement.
fn read_with<T>(
    tx: &Transaction<'_>,
    id: i64,
    acquire: impl FnOnce(PackInfo, &mut OffsetRead<'_>) -> Result<T, PersistenceError>,
) -> Result<T, PersistenceError> {
    if id <= 0 {
        return Err(PersistenceError::Malformed);
    }
    let found = tx.typed(
        &format!(
            "SELECT pack_id,domain,digest,length,substring(body FROM 1 FOR $2) FROM {}.pack WHERE pack_id=$1",
            tx.schema
        ),
        &[&id, &HEAD_BYTES],
        12,
    )?;
    let row = found.first().ok_or(PersistenceError::Missing)?;
    let descriptor = info(&rows::record_first(row, 4)?)?;
    if found.len() != 1 || descriptor.pack_id != id {
        return Err(PersistenceError::Malformed);
    }
    let head: Vec<u8> = row.try_get(4).map_err(|e| rows::error(&e))?;
    if head.len() != descriptor.length.min(HEAD_BYTES as usize) {
        return Err(PersistenceError::Malformed);
    }
    let ranged = format!(
        "SELECT substring(body FROM $2 FOR $3) FROM {}.pack WHERE pack_id=$1",
        tx.schema
    );
    acquire(descriptor, &mut |offset, bytes| {
        if let Some(held) = offset
            .checked_add(bytes.len())
            .and_then(|end| head.get(offset..end))
        {
            bytes.copy_from_slice(held);
            return Ok(());
        }
        let from = i32::try_from(offset + 1).map_err(|_| PersistenceError::Malformed)?;
        let count = i32::try_from(bytes.len()).map_err(|_| PersistenceError::Malformed)?;
        let rows = tx.typed(&ranged, &[&id, &from, &count], 16)?;
        let slice: Vec<u8> = rows
            .first()
            .ok_or(PersistenceError::Missing)?
            .try_get(0)
            .map_err(|e| PersistenceError::from(rows::error(&e)))?;
        if slice.len() != bytes.len() {
            return Err(PersistenceError::Malformed);
        }
        bytes.copy_from_slice(&slice);
        Ok(())
    })
}
