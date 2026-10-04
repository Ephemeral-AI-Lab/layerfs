//! Logical pack offsets mapped to independent immutable unit BLOBs, one transaction.
use super::{rows, transaction::Transaction};
use crate::backend::{metadata_locations::info, records::Param};
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
        let mut descriptors = tx.query(
            "SELECT pack_id,domain,digest,length,length(control) FROM pack WHERE pack_id=?1",
            vec![Param::I64(id)],
        )?;
        let info = info(
            descriptors
                .first()
                .filter(|_| descriptors.len() == 1)
                .ok_or(PersistenceError::Missing)?,
            0,
        )?;
        let control_length = descriptors[0].get::<i64>(4)?;
        if !(24..=PACK_READ_PREFIX_BYTES as i64).contains(&control_length) {
            return Err(PersistenceError::Malformed);
        }
        descriptors.clear();
        let mut controls = tx.query(
            "SELECT control FROM pack WHERE pack_id=?1",
            vec![Param::I64(id)],
        )?;
        if controls.len() != 1 {
            return Err(PersistenceError::Malformed);
        }
        let control = controls[0].take_bytes(0)?;
        if control.len() > PACK_READ_PREFIX_BYTES {
            return Err(PersistenceError::Malformed);
        }
        let header = layout::parse_directory_header(&control, info.length)
            .map_err(|_| PersistenceError::Malformed)?;
        if control.len() != header.body_offset
            || layerfs_storage::location::PackDomain::for_lane(header.lane) != info.domain
        {
            return Err(PersistenceError::Malformed);
        }
        let views = layout::directory_group_views(&control, header)
            .map_err(|_| PersistenceError::Malformed)?;
        let records=tx.query("SELECT unit_id,group_number,offset,length FROM pack_unit WHERE pack_id=?1 ORDER BY group_number LIMIT 257",vec![Param::I64(id)])?;
        if records.len() != views.len() {
            return Err(PersistenceError::Malformed);
        }
        let mut units = Vec::with_capacity(records.len());
        for (number, (row, view)) in records.iter().zip(views).enumerate() {
            let id = row.get::<i64>(0)?;
            if id <= 0
                || row.get::<i64>(1)? != number as i64
                || row.get::<i64>(2)? != view.start as i64
                || row.get::<i64>(3)? != (view.end - view.start) as i64
            {
                return Err(PersistenceError::Malformed);
            }
            units.push(Unit {
                id,
                start: view.start,
                end: view.end,
            });
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
