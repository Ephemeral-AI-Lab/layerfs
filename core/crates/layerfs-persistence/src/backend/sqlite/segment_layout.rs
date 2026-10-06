//! Versioned hybrid placement and conservative catalogue/directory charges.
#[cfg(target_os = "macos")]
use crate::backend::records::BackendError;
use layerfs_storage::{location::PackDomain, policy, port::*};
/// Small physical payloads and all pooled metadata stay in the catalogue.
pub(crate) const INLINE_BYTES: usize = 64 * 1024;
// One segment row/name plus one extent descriptor per pack, conservatively
// charged per external pack even though a publication shares its segment row.
const EXTENT_CHARGE: u64 = 128;
pub(crate) fn external(pack: &PublishedPack) -> bool {
    pack.info.domain == PackDomain::Payload && pack.info.length > INLINE_BYTES
}
pub(crate) fn cost(pack: &PublishedPack) -> (usize, u64) {
    if external(pack) {
        (2, pack.body.len() as u64 + EXTENT_CHARGE)
    } else {
        (1, pack.body.len() as u64)
    }
}
pub(crate) fn validate(batch: &Publication) -> Result<(), PersistenceError> {
    let extra = batch.packs.iter().filter(|p| external(p)).count();
    let rows = batch.packs.len()
        + extra
        + batch.objects.len()
        + batch.value_groups.len()
        + batch.signatures.len()
        + usize::from(batch.window_start.is_some())
        + usize::from(batch.release_ordinals.is_some());
    let bytes: u64 = batch.packs.iter().map(|p| cost(p).1).sum();
    let singleton = batch.packs.len() == 1 && batch.objects.len() == 1 && external(&batch.packs[0]);
    if rows > policy::TRANSACTION_ROW_LIMIT as usize
        || bytes > policy::TRANSACTION_PHYSICAL_BYTES_LIMIT && !singleton
    {
        return Err(PersistenceError::Malformed);
    }
    Ok(())
}
pub(crate) struct Segment {
    pub(crate) id: i64,
    pub(crate) device: i64,
    pub(crate) inode: i64,
    pub(crate) length: usize,
}
#[cfg(target_os = "macos")]
pub(crate) fn identity(device: u64, inode: u64) -> Result<(i64, i64), BackendError> {
    Ok((
        device.try_into().map_err(|_| BackendError::Capacity)?,
        inode.try_into().map_err(|_| BackendError::Capacity)?,
    ))
}
