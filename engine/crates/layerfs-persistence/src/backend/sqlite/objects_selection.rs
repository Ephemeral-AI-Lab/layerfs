//! Exact incremental BLOB I/O; C2 plans and authenticates selected acquisition.
use super::{rows, transaction::Transaction};
use crate::backend::{metadata_locations::info, records::Param};
use layerfs_storage::{
    location::PackInfo,
    port::{AcquiredPackRead, PackReadPlan, PersistedPackRead, PersistenceError},
};
use std::time::Instant;

pub(crate) fn read(
    tx: &Transaction<'_>,
    id: i64,
    plan: &mut dyn PackReadPlan,
) -> Result<PersistedPackRead, PersistenceError> {
    if tx.layout().uses_units() {
        return super::units_read::strict(tx, id, plan);
    }
    read_with(tx, id, |descriptor, read_at| {
        PersistedPackRead::acquire(descriptor, plan, read_at)
    })
}
pub(crate) fn read_scoped(
    tx: &Transaction<'_>,
    id: i64,
    plan: &mut dyn PackReadPlan,
) -> Result<AcquiredPackRead, PersistenceError> {
    if tx.layout().uses_units() {
        return super::units_read::scoped(tx, id, plan);
    }
    read_with(tx, id, |descriptor, read_at| {
        AcquiredPackRead::acquire(descriptor, plan, read_at)
    })
}
type OffsetRead<'a> = dyn FnMut(usize, &mut [u8]) -> Result<(), PersistenceError> + 'a;
fn read_with<T>(
    tx: &Transaction<'_>,
    id: i64,
    acquire: impl FnOnce(PackInfo, &mut OffsetRead<'_>) -> Result<T, PersistenceError>,
) -> Result<T, PersistenceError> {
    if id <= 0 {
        return Err(PersistenceError::Malformed);
    }
    let descriptors = tx.query(
        "SELECT pack_id,domain,digest,length FROM pack WHERE pack_id=?1",
        vec![Param::I64(id)],
    )?;
    let descriptor = info(descriptors.first().ok_or(PersistenceError::Missing)?, 0)?;
    if descriptors.len() != 1 || descriptor.pack_id != id {
        return Err(PersistenceError::Malformed);
    }
    tx.work.borrow_mut().blob_open_calls += 1;
    let blob = tx
        .connection
        .blob_open("main", "pack", "body", id, true)
        .map_err(rows::error)?;
    let result = if blob.len() != descriptor.length {
        Err(PersistenceError::Malformed)
    } else {
        acquire(descriptor, &mut |offset, bytes| {
            {
                let mut work = tx.work.borrow_mut();
                work.blob_read_calls += 1;
                work.blob_requested_bytes += bytes.len() as u64;
            }
            let start = Instant::now();
            let result = blob.read_at_exact(bytes, offset).map_err(rows::error);
            let mut work = tx.work.borrow_mut();
            work.blob_read_ns += start.elapsed().as_nanos() as u64;
            if result.is_ok() {
                work.blob_read_bytes += bytes.len() as u64;
            }
            result.map_err(Into::into)
        })
    };
    tx.work.borrow_mut().blob_close_calls += 1;
    let close: Result<(), PersistenceError> = blob.close().map_err(rows::error).map_err(Into::into);
    match (result, close) {
        (Err(PersistenceError::Uncertain), _) | (_, Err(PersistenceError::Uncertain)) => {
            Err(PersistenceError::Uncertain)
        }
        (Err(error), _) | (_, Err(error)) => Err(error),
        (Ok(value), Ok(())) => Ok(value),
    }
}
