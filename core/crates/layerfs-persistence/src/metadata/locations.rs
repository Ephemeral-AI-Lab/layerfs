//! Common locator cardinality/identity bounds, checked around the backend read.
use crate::backend::{metadata_locations, Transaction};
use layerfs_content::ObjectId;
use layerfs_storage::{location::LocatedObject, policy, port::PersistenceError};
use std::collections::BTreeSet;
pub(crate) fn read(
    tx: &Transaction<'_>,
    ids: &[ObjectId],
    out: &mut Vec<LocatedObject>,
) -> Result<(), PersistenceError> {
    if ids.len() > policy::READ_OBJECT_LIMIT {
        return Err(PersistenceError::Malformed);
    }
    metadata_locations::read(tx, ids, out)?;
    let unique: BTreeSet<_> = ids.iter().copied().collect();
    let mut seen = BTreeSet::new();
    for row in out {
        let l = row.location;
        if !unique.contains(&l.object_id)
            || !seen.insert(l.object_id)
            || l.pack_id != row.pack.pack_id
            || l.canonical_length == 0
            || l.canonical_length > policy::CANONICAL_LIMIT
            || l.group_number >= policy::GROUP_COUNT_LIMIT
            || l.record_number >= policy::RECORD_COUNT_LIMIT
        {
            return Err(PersistenceError::Malformed);
        }
    }
    Ok(())
}
