//! Cross-Objects/Metadata bounds and atomic publication orchestration.
use crate::backend::{publish, Transaction};
use layerfs_storage::{location::PackDomain, policy, port::*};
use std::collections::BTreeSet;
pub(crate) fn publish(
    tx: &Transaction<'_>,
    batch: &Publication,
) -> Result<Published, PersistenceError> {
    publish::run(tx, batch).map_err(Into::into)
}
pub(crate) fn validate(batch: &Publication) -> Result<(), PersistenceError> {
    let count = batch.packs.len()
        + batch.objects.len()
        + batch.value_groups.len()
        + batch.signatures.len()
        + usize::from(batch.window_start.is_some())
        + usize::from(batch.release_ordinals.is_some());
    if count > policy::TRANSACTION_ROW_LIMIT as usize {
        return Err(PersistenceError::Malformed);
    }
    let physical_bytes = batch
        .packs
        .iter()
        .map(|row| row.body.len())
        .try_fold(0usize, |total, n| total.checked_add(n))
        .ok_or(PersistenceError::Malformed)?;
    let canonical_bytes = batch
        .objects
        .iter()
        .map(|row| row.canonical_length)
        .try_fold(0usize, |total, n| total.checked_add(n))
        .ok_or(PersistenceError::Malformed)?;
    let singleton = batch.objects.len() == 1
        && batch.packs.len() == 1
        && batch.packs[0].info.domain == PackDomain::Payload;
    if (canonical_bytes > policy::TRANSACTION_CANONICAL_BYTES_LIMIT as usize
        || physical_bytes > policy::TRANSACTION_PHYSICAL_BYTES_LIMIT as usize)
        && !singleton
    {
        return Err(PersistenceError::Malformed);
    }
    let mut packs = BTreeSet::new();
    for row in &batch.packs {
        let info = row.info;
        if info.pack_id <= 0
            || !packs.insert(info.pack_id)
            || !(32..=policy::SINGLETON_PACK_LIMIT).contains(&info.length)
            || row.body.len() != info.length
            || layerfs_storage::port::ObjectKey::for_bytes(&row.body) != info.key
            || (info.domain == PackDomain::Metadata && info.length > policy::PACK_LIMIT)
        {
            return Err(PersistenceError::Malformed);
        }
        let header = layerfs_storage::pack::layout::parse_header(&row.body)
            .map_err(|_| PersistenceError::Malformed)?;
        if PackDomain::for_lane(header.lane) != info.domain
            || layerfs_storage::pack::layout::declared_length(&row.body)
                .map_err(|_| PersistenceError::Malformed)?
                != info.length
        {
            return Err(PersistenceError::Malformed);
        }
    }
    let mut objects = BTreeSet::new();
    for row in &batch.objects {
        if !objects.insert(row.object_id)
            || row.pack_id <= 0
            || row.canonical_length == 0
            || row.canonical_length > policy::CANONICAL_LIMIT
            || row.group_number >= policy::GROUP_COUNT_LIMIT
            || row.record_number >= policy::RECORD_COUNT_LIMIT
        {
            return Err(PersistenceError::Malformed);
        }
    }
    let mut groups = BTreeSet::new();
    for row in &batch.value_groups {
        if !groups.insert(row.first_ordinal)
            || row.first_ordinal == 0
            || row.count == 0
            || row.count > policy::VALUES_PER_GROUP
            || row.pack_id <= 0
            || row.group_number >= policy::GROUP_COUNT_LIMIT
            || u64::from(row.first_ordinal) + row.count as u64 > u64::from(u32::MAX) + 1
        {
            return Err(PersistenceError::Malformed);
        }
    }
    let mut slots = BTreeSet::new();
    for row in &batch.signatures {
        if !slots.insert(row.slot)
            || row.slot >= 8192
            || row.stamp == 0
            || row.stamp > i64::MAX as u64
            || (row.stamp - 1) % 8192 != row.slot as u64
        {
            return Err(PersistenceError::Malformed);
        }
    }
    if batch.window_start == Some(0)
        || batch.release_ordinals.is_some_and(|(first, count)| {
            first == 0
                || count > policy::METADATA_INDEX_VALUES
                || u64::from(first) + count as u64 > u64::from(u32::MAX) + 1
        })
    {
        return Err(PersistenceError::Malformed);
    }
    Ok(())
}
