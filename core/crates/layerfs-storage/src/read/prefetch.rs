//! Dependency locations acquired level by level before canonical reconstruction.

use super::Fetch;
use crate::{
    encoding::{codec::DecompressionWorkspace, delta::read::stored_base, GroupCache},
    error::{StorageError, StorageResult},
    location::ObjectLocation,
    source::Source,
};
use layerfs_content::ObjectId;
use std::collections::BTreeSet;

pub(crate) fn chains(
    source: &Fetch,
    roots: &[ObjectLocation],
    packs: &mut crate::encoding::PackCache,
    workspace: &mut DecompressionWorkspace,
    groups: &mut GroupCache,
) -> StorageResult<crate::encoding::pool::PoolReadCounters> {
    let mut frontier = roots.to_vec();
    let mut visited = BTreeSet::new();
    let mut pooled = crate::encoding::pool::PoolReadCounters::default();
    for _ in 0..=layerfs_content::MAXIMUM_DELTA_MAX_DEPTH {
        if frontier.is_empty() {
            return Ok(pooled);
        }
        // Consume each byte-bounded physical cohort before acquiring another.
        // Discovery and reconstruction share the owning operation's group cache.
        frontier.sort_unstable_by_key(|row| (row.pack_id, row.group_number, row.record_number));
        let mut next = BTreeSet::new();
        let mut from = 0;
        let mut decodes = 0;
        while from < frontier.len() {
            let end = source.cohort_end(&frontier, from)?;
            source.fetch_packs(
                &frontier[from..end]
                    .iter()
                    .map(|row| row.pack_id)
                    .collect::<Vec<_>>(),
                packs,
            )?;
            for location in &frontier[from..end] {
                if !visited.insert(location.object_id) {
                    continue;
                }
                let before = decodes;
                if let Some(base) =
                    stored_base(source, packs, groups, workspace, &mut decodes, location)?
                {
                    next.insert(base);
                }
                if location.role == layerfs_content::ObjectRole::InodeLeaf && decodes > before {
                    pooled.physical_group_decodes += decodes - before;
                    pooled.physical_group_decoded_bytes += groups
                        .get(location.pack_id, location.group_number)
                        .ok_or(StorageError::Integrity("prefetch decoded group"))?
                        .len() as u64;
                }
            }
            from = end;
        }
        source.note(|c| c.prefetch_group_decodes += decodes);
        let ids: Vec<ObjectId> = next.into_iter().collect();
        source.locate(&ids)?;
        frontier = ids
            .into_iter()
            .filter(|id| !visited.contains(id))
            .map(|id| {
                source
                    .location(id, i64::MAX)?
                    .ok_or(StorageError::ObjectMissing(id))
            })
            .collect::<StorageResult<Vec<_>>>()?;
    }
    if frontier.is_empty() {
        Ok(pooled)
    } else {
        Err(StorageError::Integrity("prefetch dependency depth"))
    }
}
