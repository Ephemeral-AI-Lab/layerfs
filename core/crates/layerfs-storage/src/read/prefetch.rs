//! Dependency locations acquired level by level before canonical reconstruction.

use super::{
    objects::{call_failure, duplicate},
    Fetch,
};
use crate::{
    encoding::{codec::DecompressionWorkspace, delta::read::stored_base, GroupCache},
    error::{StorageError, StorageResult},
    location::ObjectLocation,
    source::Source,
};
use layerfs_content::ObjectId;
use std::collections::{BTreeMap, BTreeSet};

/// The roots whose chains [`chains_each`] could not walk, by position in its
/// `roots`, each with the failure its chain met first.
pub(crate) type ChainFailures = Vec<(usize, StorageError)>;

/// [`chains`] with one outcome per root (#818 H5): the same calls in the
/// same order. A failure of one unit (a pack's acquisition, a record's base,
/// an absent base, a chain past the depth bound) ends the chains through
/// that unit and is returned for every root whose chain reaches it, the
/// first such unit along each chain, which is where that root's own walk
/// fails; the other chains go on. Within a per-object demand
/// ([`Fetch::per_object`]) a pack refused there, by a group read or an
/// acquisition, is not acquired again. A failed call over the level's units
/// (its group read and its locate) and a session or transport failure fail
/// the call.
pub(crate) fn chains_each(
    source: &Fetch,
    roots: &[ObjectLocation],
    packs: &mut crate::encoding::PackCache,
    workspace: &mut DecompressionWorkspace,
    groups: &mut GroupCache,
) -> StorageResult<(crate::encoding::pool::PoolReadCounters, ChainFailures)> {
    let mut frontier = roots.to_vec();
    let mut visited = BTreeSet::new();
    // Each walked element's base and each failed element's failure: at most
    // one entry per visited element, as `visited`.
    let mut bases = BTreeMap::new();
    let mut failed = BTreeMap::new();
    let mut pooled = crate::encoding::pool::PoolReadCounters::default();
    for level in 0..=layerfs_content::MAXIMUM_DELTA_MAX_DEPTH {
        if frontier.is_empty() {
            break;
        }
        source.prefetch_groups(&frontier, packs)?;
        frontier.sort_unstable_by_key(|row| (row.pack_id, row.group_number, row.record_number));
        let mut next = BTreeSet::new();
        let mut edges = Vec::new();
        let mut from = 0;
        let mut decodes = 0;
        while from < frontier.len() {
            let id = frontier[from].pack_id;
            let end = from
                + frontier[from..]
                    .iter()
                    .take_while(|row| row.pack_id == id)
                    .count();
            let needed: Vec<_> = frontier[from..end]
                .iter()
                .map(|row| row.group_number)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            // A pack refused earlier in the demand is not acquired again.
            let acquired = match source.refused_pack(id) {
                Some(error) => Err(error),
                None => packs.acquire_groups(source, id, &needed).map(drop),
            };
            if let Err(error) = acquired {
                if call_failure(&error) {
                    return Err(error);
                }
                for location in &frontier[from..end] {
                    if visited.insert(location.object_id) {
                        failed.insert(location.object_id, duplicate(&error));
                    }
                }
                source.refuse(id, error);
                from = end;
                continue;
            }
            for location in &frontier[from..end] {
                if !visited.insert(location.object_id) {
                    continue;
                }
                let before = decodes;
                let unhinted = level == 0 && !source.has_hint(location.object_id);
                let base =
                    match stored_base(source, packs, groups, workspace, &mut decodes, location) {
                        Ok(base) => base,
                        Err(error) if call_failure(&error) => return Err(error),
                        Err(error) => {
                            failed.insert(location.object_id, error);
                            continue;
                        }
                    };
                if unhinted && base.is_some() {
                    source.note(|c| c.chain_unhinted_roots += 1);
                }
                edges.extend(base.map(|base| (*location, base)));
                next.extend(base);
                if let Some(base) = base {
                    bases.insert(location.object_id, base);
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
        let ahead = source.chain_ahead(&ids, &edges, packs, groups, workspace);
        source.locate(&[ids.as_slice(), ahead.as_slice()].concat())?;
        frontier = Vec::new();
        let unvisited: Vec<_> = ids.into_iter().filter(|id| !visited.contains(id)).collect();
        for id in unvisited {
            // A base whose pack row is gone fails as its level's locate does.
            if let Some(error) = source.unpacked(id) {
                visited.insert(id);
                failed.insert(id, error);
                continue;
            }
            match source.base_location(id, i64::MAX)? {
                Some(location) => frontier.push(location),
                None => {
                    visited.insert(id);
                    failed.insert(id, StorageError::ObjectMissing(id));
                }
            }
        }
    }
    for location in frontier {
        if visited.insert(location.object_id) {
            failed.insert(
                location.object_id,
                StorageError::Integrity("prefetch dependency depth"),
            );
        }
    }
    let mut outcomes = Vec::new();
    for (at, root) in roots.iter().enumerate() {
        let mut id = root.object_id;
        // The root, its bases to the depth bound, and the element past it.
        for _ in 0..=layerfs_content::MAXIMUM_DELTA_MAX_DEPTH + 1 {
            if let Some(error) = failed.get(&id) {
                outcomes.push((at, duplicate(error)));
                break;
            }
            match bases.get(&id) {
                Some(base) => id = *base,
                None => break,
            }
        }
    }
    Ok((pooled, outcomes))
}

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
    for level in 0..=layerfs_content::MAXIMUM_DELTA_MAX_DEPTH {
        if frontier.is_empty() {
            return Ok(pooled);
        }
        // One call for the frontier's and its chains' uncached groups (#758).
        source.prefetch_groups(&frontier, packs)?;
        // Consume each byte-bounded physical cohort before acquiring another.
        // Discovery and reconstruction share the owning operation's group cache.
        frontier.sort_unstable_by_key(|row| (row.pack_id, row.group_number, row.record_number));
        let mut next = BTreeSet::new();
        let mut edges = Vec::new();
        let mut from = 0;
        let mut decodes = 0;
        while from < frontier.len() {
            let id = frontier[from].pack_id;
            let end = from
                + frontier[from..]
                    .iter()
                    .take_while(|row| row.pack_id == id)
                    .count();
            let needed: Vec<_> = frontier[from..end]
                .iter()
                .map(|row| row.group_number)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            packs.acquire_groups(source, id, &needed)?;
            for location in &frontier[from..end] {
                if !visited.insert(location.object_id) {
                    continue;
                }
                let before = decodes;
                let unhinted = level == 0 && !source.has_hint(location.object_id);
                let base = stored_base(source, packs, groups, workspace, &mut decodes, location)?;
                if unhinted && base.is_some() {
                    source.note(|c| c.chain_unhinted_roots += 1);
                }
                edges.extend(base.map(|base| (*location, base)));
                next.extend(base);
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
        // One locate for this level's bases and the deeper bases its chains
        // let the cached records name (#758): authoritative rows only.
        let ahead = source.chain_ahead(&ids, &edges, packs, groups, workspace);
        source.locate(&[ids.as_slice(), ahead.as_slice()].concat())?;
        frontier = ids
            .into_iter()
            .filter(|id| !visited.contains(id))
            .map(|id| {
                source
                    .base_location(id, i64::MAX)?
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
