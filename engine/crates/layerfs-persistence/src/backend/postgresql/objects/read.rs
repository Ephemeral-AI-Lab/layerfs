//! Whole and ranged pack reads; read budgets are checked before any body.
use super::super::session::{rows, transaction::Transaction};
use crate::backend::records::{pack_info, BackendError, Record};
use layerfs_storage::{
    location::PackInfo,
    policy,
    port::{AcquiredPackRead, PackReadPlan, PersistedPack, PersistedPackRead, PersistenceError},
};
use std::time::Instant;

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
    let descriptors = tx.typed(
        "SELECT pack_id,domain,digest,length FROM pack WHERE pack_id = ANY($1::bigint[]) ORDER BY pack_id",
        &[&ids],
        ids.len() as u64 * 8,
    )?;
    let mut infos = Vec::with_capacity(descriptors.len());
    let mut total = 0usize;
    for row in descriptors {
        let descriptor = info(&rows::record(&row)?)?;
        total = total
            .checked_add(descriptor.length)
            .ok_or(BackendError::Capacity)?;
        infos.push(descriptor);
    }
    if infos.len() != ids.len() {
        return Err(PersistenceError::Missing);
    }
    if total > policy::DEPENDENCY_PACK_CACHE_BYTES
        && !(ids.len() == 1 && total <= policy::SINGLETON_PACK_LIMIT)
    {
        return Err(BackendError::Capacity.into());
    }
    for info in infos {
        let mut rows = tx.query(
            "SELECT body FROM pack WHERE pack_id=$1",
            vec![crate::backend::records::Param::I64(info.pack_id)],
        )?;
        let body = rows
            .first_mut()
            .ok_or(PersistenceError::Missing)?
            .take_bytes(0)?;
        out.push(PersistedPack::authenticate(info, body)?);
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

/// Exact ranges of one body: the body is stored uncompressed out of line
/// (`STORAGE EXTERNAL`), so `substring` fetches only the chunks it covers.
fn read_with<T>(
    tx: &Transaction<'_>,
    id: i64,
    acquire: impl FnOnce(PackInfo, &mut OffsetRead<'_>) -> Result<T, PersistenceError>,
) -> Result<T, PersistenceError> {
    if id <= 0 {
        return Err(PersistenceError::Malformed);
    }
    let descriptors = tx.query(
        "SELECT pack_id,domain,digest,length FROM pack WHERE pack_id=$1",
        vec![crate::backend::records::Param::I64(id)],
    )?;
    let descriptor = info(descriptors.first().ok_or(PersistenceError::Missing)?)?;
    if descriptors.len() != 1 || descriptor.pack_id != id {
        return Err(PersistenceError::Malformed);
    }
    acquire(descriptor, &mut |offset, bytes| {
        let start = Instant::now();
        let from = i32::try_from(offset + 1).map_err(|_| PersistenceError::Malformed)?;
        let count = i32::try_from(bytes.len()).map_err(|_| PersistenceError::Malformed)?;
        let rows = tx.typed(
            "SELECT substring(body FROM $2 FOR $3) FROM pack WHERE pack_id=$1",
            &[&id, &from, &count],
            16,
        )?;
        let slice: Vec<u8> = rows
            .first()
            .ok_or(PersistenceError::Missing)?
            .try_get(0)
            .map_err(|e| PersistenceError::from(rows::error(&e)))?;
        if slice.len() != bytes.len() {
            return Err(PersistenceError::Malformed);
        }
        bytes.copy_from_slice(&slice);
        let _ = start;
        Ok(())
    })
}
