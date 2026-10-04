//! Dependency locations acquired level by level before canonical reconstruction.

use super::Fetch;
use crate::{
    encoding::{codec::DecompressionWorkspace, delta::read::ChainBases},
    error::{StorageError, StorageResult},
    location::ObjectLocation,
    source::Source,
};
use layerfs_content::ObjectId;
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn chains(
    source: &Fetch,
    roots: &[ObjectLocation],
    packs: &mut BTreeMap<i64, Vec<u8>>,
    workspace: &mut DecompressionWorkspace,
) -> StorageResult<()> {
    let mut frontier = roots.to_vec();
    let mut visited = BTreeSet::new();
    for _ in 0..=layerfs_content::MAXIMUM_DELTA_MAX_DEPTH {
        if frontier.is_empty() {
            return Ok(());
        }
        source.fetch_packs(
            &frontier.iter().map(|row| row.pack_id).collect::<Vec<_>>(),
            packs,
        )?;
        let mut next = BTreeSet::new();
        let mut bases = ChainBases::new(packs);
        for location in frontier {
            if !visited.insert(location.object_id) {
                continue;
            }
            if let Some(base) = bases.base_of(source, workspace, &location)? {
                next.insert(base);
            }
        }
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
        Ok(())
    } else {
        Err(StorageError::Integrity("prefetch dependency depth"))
    }
}
