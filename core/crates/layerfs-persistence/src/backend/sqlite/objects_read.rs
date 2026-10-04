//! Read budgets are checked before any BLOB is acquired into the result buffer.
use crate::{
    backend::metadata_locations::info,
    backend::{
        records::{BackendError, Param},
        Transaction,
    },
};
use layerfs_storage::{policy, port::*};
use std::collections::BTreeSet;
pub(crate) fn read(
    tx: &Transaction<'_>,
    ids: &[i64],
    out: &mut Vec<PersistedPack>,
) -> Result<(), PersistenceError> {
    if ids.len() > policy::READ_OBJECT_LIMIT || ids.iter().any(|id| *id <= 0) {
        return Err(PersistenceError::Malformed);
    }
    let ids: Vec<_> = ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut descriptors = Vec::new();
    let mut total = 0usize;
    for page in ids.chunks(tx.input_limit(1, 128, 2)?) {
        let sql = format!(
            "SELECT pack_id,domain,digest,length FROM pack WHERE pack_id IN({}) ORDER BY pack_id",
            std::iter::repeat_n("?", page.len())
                .collect::<Vec<_>>()
                .join(",")
        );
        for r in tx.query(&sql, page.iter().map(|id| Param::I64(*id)).collect())? {
            let descriptor = info(&r, 0)?;
            total = total
                .checked_add(descriptor.length)
                .ok_or(BackendError::Capacity)?;
            descriptors.push(descriptor);
        }
    }
    if descriptors.len() != ids.len() {
        return Err(PersistenceError::Missing);
    }
    if total > policy::DEPENDENCY_PACK_CACHE_BYTES
        && !(ids.len() == 1 && total <= policy::SINGLETON_PACK_LIMIT)
    {
        return Err(BackendError::Capacity.into());
    }
    for info in descriptors {
        let mut rows = tx.query(
            "SELECT body FROM pack WHERE pack_id=?1",
            vec![Param::I64(info.pack_id)],
        )?;
        let body: Vec<u8> = rows
            .first_mut()
            .ok_or(PersistenceError::Missing)?
            .take_bytes(0)?;
        out.push(PersistedPack::authenticate(info, body)?);
    }
    Ok(())
}
