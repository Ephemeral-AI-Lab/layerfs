//! Durable body preparation precedes one bounded catalogue acknowledgement.
use super::{
    segment_layout::{self, Segment},
    transaction::Transaction,
};
use crate::backend::records::{BackendError, Param};
use layerfs_storage::port::PublishedPack;
pub(crate) fn write(tx: &Transaction<'_>, packs: &[PublishedPack]) -> Result<(), BackendError> {
    let external: Vec<_> = packs
        .iter()
        .filter(|p| segment_layout::external(p))
        .collect();
    if !external.is_empty() {
        let segment = prepare(tx, &external)?;
        tx.query(
            "INSERT INTO body_segment(segment_id,device,inode,length) VALUES(?1,?2,?3,?4)",
            vec![
                Param::I64(segment.id),
                Param::I64(segment.device),
                Param::I64(segment.inode),
                Param::I64(segment.length as i64),
            ],
        )?;
        let mut offset = 0i64;
        let mut statement = tx.prepare("INSERT INTO pack(pack_id,domain,digest,length,body,segment_id,segment_offset) VALUES(?1,1,?2,?3,NULL,?4,?5)")?;
        for pack in external {
            let length = pack.info.length as i64;
            let digest = pack.info.key.as_bytes().as_slice();
            statement.borrowed(
                &[&pack.info.pack_id, &digest, &length, &segment.id, &offset],
                64,
            )?;
            offset += length;
        }
    }
    let inline: Vec<_> = packs
        .iter()
        .filter(|p| !segment_layout::external(p))
        .cloned()
        .collect();
    super::publish::write_packs(tx, &inline)
}
fn prepare(tx: &Transaction<'_>, packs: &[&PublishedPack]) -> Result<Segment, BackendError> {
    #[cfg(target_os = "macos")]
    {
        tx.segment_owner()?.prepare(packs, tx.work)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (tx, packs);
        Err(BackendError::Integrity)
    }
}
pub(crate) fn initialize(tx: &Transaction<'_>) -> Result<(), BackendError> {
    #[cfg(target_os = "macos")]
    {
        let (device, inode) = tx.segment_owner()?.identity();
        tx.query(
            "INSERT INTO body_directory(id,device,inode) VALUES(1,?1,?2)",
            vec![Param::I64(device), Param::I64(inode)],
        )?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = tx;
        Err(BackendError::Integrity)
    }
}
pub(crate) fn check(tx: &Transaction<'_>) -> Result<(), BackendError> {
    #[cfg(target_os = "macos")]
    {
        let rows = tx.query("SELECT device,inode FROM body_directory WHERE id=1", vec![])?;
        let row = rows
            .first()
            .filter(|_| rows.len() == 1)
            .ok_or(BackendError::Integrity)?;
        if (row.get(0)?, row.get(1)?) != tx.segment_owner()?.identity() {
            return Err(BackendError::Integrity);
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = tx;
        Err(BackendError::Integrity)
    }
}
