//! Logical pack offsets mapped to independent immutable unit BLOBs, one
//! transaction, and the bounded read-only unit cursor one acquisition owns.
use crate::backend::sqlite::session::rows;
use crate::backend::sqlite::session::transaction::Transaction;
use crate::backend::{error::BackendError, metadata_locations::typed_info};
use layerfs_storage::{location::PackInfo, pack::layout, port::*};
use std::time::Instant;
pub(super) struct Unit {
    pub(super) id: i64,
    pub(super) start: usize,
    pub(super) end: usize,
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
        let prefix_limit = PACK_READ_PREFIX_BYTES as i64;
        let mut descriptors = tx.mapped(
            "SELECT CASE WHEN (CASE WHEN typeof(control)='blob' THEN length(CAST(control AS BLOB)) ELSE NULL END) BETWEEN 24 AND ?2 THEN control END AS control,pack_id,domain,CASE WHEN (CASE WHEN typeof(digest)='blob' THEN length(CAST(digest AS BLOB)) ELSE NULL END)=32 THEN digest END,length,(CASE WHEN typeof(control)='blob' THEN length(CAST(control AS BLOB)) ELSE NULL END) FROM pack WHERE pack_id=?1",
            &[&id, &prefix_limit], 16, 1,
            |row| {
                let info = typed_info(row, 1)?;
                let length = row.get::<_, i64>(5).map_err(rows::error)?;
                if !(24..=prefix_limit).contains(&length) { return Err(BackendError::Integrity); }
                let control = rows::blob(row, 0)?;
                if control.len() != length as usize { return Err(BackendError::Integrity); }
                Ok((info, control.to_vec()))
            },
        )?;
        let (info, control) = descriptors.pop().ok_or(PersistenceError::Missing)?;
        if !descriptors.is_empty() {
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
        if at < end {
            self::read(tx, &self.units, at, end, &mut bytes[at - offset..])?;
        }
        Ok(())
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

// One bounded acquisition owns one read-only unit cursor and checked close.
pub(super) fn read(
    tx: &Transaction<'_>,
    units: &[Unit],
    offset: usize,
    end: usize,
    bytes: &mut [u8],
) -> Result<(), PersistenceError> {
    let mut selected = units
        .iter()
        .filter(|unit| unit.end > offset && unit.start < end);
    let first = selected.next().ok_or(PersistenceError::Malformed)?;
    if first.start > offset || end - offset != bytes.len() {
        return Err(PersistenceError::Malformed);
    }
    tx.work.borrow_mut().blob_open_calls += 1;
    let mut blob = tx
        .connection
        .blob_open("main", "pack_unit", "body", first.id, true)
        .map_err(rows::error)?;
    let result = (|| {
        let mut unit = first;
        let mut at = offset;
        loop {
            if unit.start > at || blob.len() != unit.end - unit.start {
                return Err(PersistenceError::Malformed);
            }
            let to = unit.end.min(end);
            let output = &mut bytes[at - offset..to - offset];
            {
                let mut work = tx.work.borrow_mut();
                work.blob_read_calls += 1;
                work.blob_requested_bytes += output.len() as u64;
            }
            let start = Instant::now();
            let read = blob
                .read_at_exact(output, at - unit.start)
                .map_err(rows::error);
            {
                let mut work = tx.work.borrow_mut();
                work.blob_read_ns += start.elapsed().as_nanos() as u64;
                if read.is_ok() {
                    work.blob_read_bytes += output.len() as u64;
                }
            }
            read?;
            at = to;
            if at == end {
                break;
            }
            unit = selected.next().ok_or(PersistenceError::Malformed)?;
            tx.work.borrow_mut().blob_reopen_calls += 1;
            blob.reopen(unit.id).map_err(rows::error)?;
        }
        Ok(())
    })();
    tx.work.borrow_mut().blob_close_calls += 1;
    let close: Result<(), PersistenceError> = blob.close().map_err(rows::error).map_err(Into::into);
    match (result, close) {
        (Err(PersistenceError::Uncertain), _) | (_, Err(PersistenceError::Uncertain)) => {
            Err(PersistenceError::Uncertain)
        }
        (Err(error), _) | (_, Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}
