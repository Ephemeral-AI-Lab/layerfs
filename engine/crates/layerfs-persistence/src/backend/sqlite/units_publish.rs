//! Immutable control/unit INSERTs in the existing publication transaction.
use super::{transaction::Transaction, unit_layout};
use crate::backend::records::BackendError;
use layerfs_storage::{location::PackDomain, port::PublishedPack};
pub(crate) fn write(tx: &Transaction<'_>, packs: &[PublishedPack]) -> Result<(), BackendError> {
    for pack in packs {
        let (header, views) = unit_layout::views(pack).map_err(|_| BackendError::Integrity)?;
        let id = pack.info.pack_id;
        let domain = i64::from(pack.info.domain == PackDomain::Payload);
        let length = pack.info.length as i64;
        let digest = pack.info.key.as_bytes().as_slice();
        let control = &pack.body[..header.body_offset];
        tx.borrowed(
            "INSERT INTO pack(pack_id,domain,digest,length,control) VALUES(?,?,?,?,?)",
            &[&id, &domain, &digest, &length, &control],
            56 + control.len() as u64,
        )?;
        for (number, view) in views.iter().enumerate() {
            let number = number as i64;
            let offset = view.start as i64;
            let length = (view.end - view.start) as i64;
            let body = &pack.body[view.start..view.end];
            tx.borrowed(
                "INSERT INTO pack_unit(pack_id,group_number,offset,length,body) VALUES(?,?,?,?,?)",
                &[&id, &number, &offset, &length, &body],
                32 + body.len() as u64,
            )?;
        }
        let mut work = tx.work.borrow_mut();
        work.sealed_inserts += 1 + views.len() as u64;
        work.sealed_body_bytes += pack.body.len() as u64;
    }
    Ok(())
}
