//! Shared covering-set/page semantics and chronological result validation.
use crate::backend::{metadata_pooling, Transaction};
use layerfs_storage::{policy, port::*};
use std::collections::BTreeMap;
pub(crate) fn read(
    tx: &Transaction<'_>,
    query: ValueGroupQuery<'_>,
) -> Result<ValueGroups, PersistenceError> {
    let window_start = metadata_pooling::window(tx)?;
    let mut next = None;
    let rows = match query {
        ValueGroupQuery::Page { from, limit } => {
            if from == 0 || limit > policy::READ_OBJECT_LIMIT {
                return Err(PersistenceError::Malformed);
            }
            let mut rows = metadata_pooling::page(tx, from, limit + usize::from(limit > 0))?;
            if rows.len() > limit {
                next = rows.pop().map(|r| r.first_ordinal);
            }
            rows
        }
        ValueGroupQuery::Ordinals(ids) => {
            if ids.len() > policy::READ_OBJECT_LIMIT || ids.contains(&0) {
                return Err(PersistenceError::Malformed);
            }
            let mut found = BTreeMap::new();
            for row in metadata_pooling::covering(tx, ids)? {
                found.insert(row.first_ordinal, row);
            }
            found.into_values().collect()
        }
    };
    let mut end = 0u64;
    for row in &rows {
        if row.first_ordinal == 0
            || u64::from(row.first_ordinal) < end
            || row.count == 0
            || row.count > policy::VALUES_PER_GROUP
            || row.pack_id <= 0
            || row.group_number >= policy::GROUP_COUNT_LIMIT
        {
            return Err(PersistenceError::Malformed);
        }
        end = u64::from(row.first_ordinal) + row.count as u64;
        if end > u64::from(u32::MAX) + 1 {
            return Err(PersistenceError::Malformed);
        }
    }
    Ok(ValueGroups {
        rows,
        window_start,
        next,
    })
}
