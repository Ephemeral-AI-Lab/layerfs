//! Catalogue-bound immutable extents use the existing pack authentication plans.
use super::{
    segment_layout::{Segment, INLINE_BYTES},
    transaction::Transaction,
};
use crate::backend::records::Param;
use layerfs_storage::{
    location::{PackDomain, PackInfo},
    policy,
    port::*,
};
pub(crate) struct Extent {
    segment: Segment,
    offset: usize,
}
pub(crate) type OffsetRead<'a> = dyn FnMut(usize, &mut [u8]) -> Result<(), PersistenceError> + 'a;
pub(crate) fn extent(
    tx: &Transaction<'_>,
    info: PackInfo,
) -> Result<Option<Extent>, PersistenceError> {
    let rows = tx.query("SELECT p.body IS NULL,p.segment_id,p.segment_offset,s.device,s.inode,s.length FROM pack p LEFT JOIN body_segment s ON s.segment_id=p.segment_id WHERE p.pack_id=?1", vec![Param::I64(info.pack_id)])?;
    let row = rows
        .first()
        .filter(|_| rows.len() == 1)
        .ok_or(PersistenceError::Missing)?;
    let external = info.domain == PackDomain::Payload && info.length > INLINE_BYTES;
    if row.get::<i64>(0)? != i64::from(external) {
        return Err(PersistenceError::Malformed);
    }
    let id: Option<i64> = row.get(1)?;
    let offset: Option<i64> = row.get(2)?;
    if !external {
        if id.is_some() || offset.is_some() {
            return Err(PersistenceError::Malformed);
        }
        return Ok(None);
    }
    let id = id.filter(|id| *id > 0).ok_or(PersistenceError::Malformed)?;
    let offset = usize::try_from(offset.ok_or(PersistenceError::Malformed)?)
        .map_err(|_| PersistenceError::Malformed)?;
    let device: i64 = row.get(3)?;
    let inode: i64 = row.get(4)?;
    let length = usize::try_from(row.get::<i64>(5)?).map_err(|_| PersistenceError::Malformed)?;
    if device < 0
        || inode <= 0
        || !(INLINE_BYTES + 1..=policy::SINGLETON_PACK_LIMIT).contains(&length)
        || offset
            .checked_add(info.length)
            .filter(|end| *end <= length)
            .is_none()
    {
        return Err(PersistenceError::Malformed);
    }
    Ok(Some(Extent {
        segment: Segment {
            id,
            device,
            inode,
            length,
        },
        offset,
    }))
}
pub(crate) fn acquire<T>(
    tx: &Transaction<'_>,
    info: PackInfo,
    extent: &Extent,
    operation: impl FnOnce(PackInfo, &mut OffsetRead<'_>) -> Result<T, PersistenceError>,
) -> Result<T, PersistenceError> {
    #[cfg(target_os = "macos")]
    {
        let owner = tx.segment_owner()?;
        if extent.segment.device != owner.identity().0 {
            return Err(PersistenceError::Malformed);
        }
        owner.read(
            &extent.segment,
            extent.offset,
            info.length,
            tx.work,
            |read_at| {
                Ok(operation(info, &mut |offset, bytes| {
                    read_at(offset, bytes).map_err(Into::into)
                }))
            },
        )?
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (tx, info, extent.segment.id, extent.offset, operation);
        Err(crate::backend::records::BackendError::Integrity.into())
    }
}
pub(crate) fn whole(
    tx: &Transaction<'_>,
    info: PackInfo,
    extent: &Extent,
) -> Result<PersistedPack, PersistenceError> {
    acquire(tx, info, extent, |info, read_at| {
        let mut body = vec![0; info.length];
        read_at(0, &mut body)?;
        PersistedPack::authenticate(info, body)
    })
}
