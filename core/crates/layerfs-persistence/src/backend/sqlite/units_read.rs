//! Logical pack offsets mapped to independent immutable unit BLOBs, one transaction.
use super::{rows, transaction::Transaction};
use crate::backend::{
    metadata_locations::info,
    records::{BackendError, Param},
};
use layerfs_storage::{location::PackInfo, pack::layout, port::*};
use std::time::Instant;
struct Unit {
    id: i64,
    start: usize,
    end: usize,
}
struct Input {
    info: PackInfo,
    control: Vec<u8>,
    units: Vec<Unit>,
}
impl Input {
    fn load(tx: &Transaction<'_>, id: i64) -> Result<Self, PersistenceError> {
        if id <= 0 {
            return Err(PersistenceError::Malformed);
        }
        // CASE bounds extraction inside SQLite, even if stored constraints were bypassed.
        let mut descriptors = tx.query(
            "SELECT CASE WHEN length(control) BETWEEN 24 AND ?2 THEN control END AS control,pack_id,domain,digest,length,length(control) FROM pack WHERE pack_id=?1",
            vec![Param::I64(id), Param::I64(PACK_READ_PREFIX_BYTES as i64)],
        )?;
        let row = descriptors.first_mut().ok_or(PersistenceError::Missing)?;
        let info = info(row, 1)?;
        let control_length = row.get::<i64>(5)?;
        if descriptors.len() != 1 || !(24..=PACK_READ_PREFIX_BYTES as i64).contains(&control_length)
        {
            return Err(PersistenceError::Malformed);
        }
        let control = descriptors[0].take_bytes(0)?;
        if control.len() != control_length as usize {
            return Err(PersistenceError::Malformed);
        }
        drop(descriptors);
        let header = layout::parse_directory_header(&control, info.length)
            .map_err(|_| PersistenceError::Malformed)?;
        if control.len() != header.body_offset
            || layerfs_storage::location::PackDomain::for_lane(header.lane) != info.domain
        {
            return Err(PersistenceError::Malformed);
        }
        let views = layout::directory_group_views(&control, header)
            .map_err(|_| PersistenceError::Malformed)?;
        let mut number = 0;
        let units = tx.mapped(
            "SELECT unit_id,group_number,offset,length FROM pack_unit WHERE pack_id=?1 ORDER BY group_number LIMIT 257",
            &[&id],
            8,
            views.len(),
            |row| {
                let view = views.get(number).ok_or(BackendError::Integrity)?;
                let id: i64 = row.get(0).map_err(rows::error)?;
                if id <= 0
                    || row.get::<_, i64>(1).map_err(rows::error)? != number as i64
                    || row.get::<_, i64>(2).map_err(rows::error)? != view.start as i64
                    || row.get::<_, i64>(3).map_err(rows::error)? != (view.end - view.start) as i64
                {
                    return Err(BackendError::Integrity);
                }
                number += 1;
                Ok(Unit { id, start: view.start, end: view.end })
            },
        )?;
        if units.len() != views.len() {
            return Err(PersistenceError::Malformed);
        }
        Ok(Self {
            info,
            control,
            units,
        })
    }
    fn read_at(
        &self,
        tx: &Transaction<'_>,
        offset: usize,
        bytes: &mut [u8],
    ) -> Result<(), PersistenceError> {
        let end = offset
            .checked_add(bytes.len())
            .filter(|end| *end <= self.info.length)
            .ok_or(PersistenceError::Malformed)?;
        let mut at = offset;
        if at < self.control.len() {
            let copied = (self.control.len() - at).min(bytes.len());
            bytes[..copied].copy_from_slice(&self.control[at..at + copied]);
            at += copied;
        }
        for unit in &self.units {
            if at >= end {
                break;
            }
            if unit.end <= at {
                continue;
            }
            if unit.start > at {
                return Err(PersistenceError::Malformed);
            }
            let to = unit.end.min(end);
            read_unit(
                tx,
                unit,
                &mut bytes[at - offset..to - offset],
                at - unit.start,
            )?;
            at = to;
        }
        if at != end {
            return Err(PersistenceError::Malformed);
        }
        Ok(())
    }
}
fn read_unit(
    tx: &Transaction<'_>,
    unit: &Unit,
    bytes: &mut [u8],
    offset: usize,
) -> Result<(), PersistenceError> {
    tx.work.borrow_mut().blob_open_calls += 1;
    let blob = tx
        .connection
        .blob_open("main", "pack_unit", "body", unit.id, true)
        .map_err(rows::error)?;
    let result = if blob.len() != unit.end - unit.start {
        Err(PersistenceError::Malformed)
    } else {
        {
            let mut w = tx.work.borrow_mut();
            w.blob_read_calls += 1;
            w.blob_requested_bytes += bytes.len() as u64;
        }
        let start = Instant::now();
        let result = blob.read_at_exact(bytes, offset).map_err(rows::error);
        let mut w = tx.work.borrow_mut();
        w.blob_read_ns += start.elapsed().as_nanos() as u64;
        if result.is_ok() {
            w.blob_read_bytes += bytes.len() as u64;
        }
        result.map_err(Into::into)
    };
    tx.work.borrow_mut().blob_close_calls += 1;
    let close: Result<(), PersistenceError> = blob.close().map_err(rows::error).map_err(Into::into);
    match (result, close) {
        (Err(PersistenceError::Uncertain), _) | (_, Err(PersistenceError::Uncertain)) => {
            Err(PersistenceError::Uncertain)
        }
        (Err(e), _) | (_, Err(e)) => Err(e),
        (Ok(()), Ok(())) => Ok(()),
    }
}
pub(crate) fn scoped(
    tx: &Transaction<'_>,
    id: i64,
    plan: &mut dyn PackReadPlan,
) -> Result<AcquiredPackRead, PersistenceError> {
    let input = Input::load(tx, id)?;
    AcquiredPackRead::acquire(input.info, plan, |offset, bytes| {
        input.read_at(tx, offset, bytes)
    })
}
pub(crate) fn strict(
    tx: &Transaction<'_>,
    id: i64,
    plan: &mut dyn PackReadPlan,
) -> Result<PersistedPackRead, PersistenceError> {
    let input = Input::load(tx, id)?;
    PersistedPackRead::acquire(input.info, plan, |offset, bytes| {
        input.read_at(tx, offset, bytes)
    })
}
pub(crate) fn whole(
    tx: &Transaction<'_>,
    info: PackInfo,
) -> Result<PersistedPack, PersistenceError> {
    let input = Input::load(tx, info.pack_id)?;
    if input.info != info {
        return Err(PersistenceError::Malformed);
    }
    let mut body = vec![0; info.length];
    input.read_at(tx, 0, &mut body)?;
    PersistedPack::authenticate(info, body)
}
