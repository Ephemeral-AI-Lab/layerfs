//! Logical pack offsets mapped to independent immutable unit BLOBs, one transaction.
use super::{rows, transaction::Transaction, unit_io};
use crate::backend::{metadata_locations::typed_info, records::BackendError};
use layerfs_storage::{location::PackInfo, pack::layout, port::*};
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
            "SELECT CASE WHEN length(control) BETWEEN 24 AND ?2 THEN control END AS control,pack_id,domain,digest,length,length(control) FROM pack WHERE pack_id=?1",
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
            unit_io::read(tx, &self.units, at, end, &mut bytes[at - offset..])?;
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
