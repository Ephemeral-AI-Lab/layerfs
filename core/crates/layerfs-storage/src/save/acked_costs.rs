//! Chain costs for this Save's acknowledged FULL whole-file winners.
use crate::{
    encoding::delta::{read::record_width, select::DepthCache},
    error::StorageResult,
    location::PackDomain,
    port::Publication,
};
use layerfs_content::{ObjectId, ObjectRole};
use std::collections::{BTreeMap, BTreeSet};

/// The exact selected width is read from the immutable pack layout. Only
/// winners with a ring signature are offered to the existing bounded cache.
/// A loser may have the same canonical bytes but a different physical chain.
pub(super) fn seed(
    costs: &mut DepthCache,
    batch: &Publication,
    lost: &BTreeSet<ObjectId>,
) -> StorageResult<()> {
    if batch.signatures.is_empty() {
        return Ok(());
    }
    let signed: BTreeSet<_> = batch.signatures.iter().map(|row| row.object_id).collect();
    let packs: BTreeMap<_, _> = batch
        .packs
        .iter()
        .filter(|pack| pack.info().domain == PackDomain::Payload)
        .map(|pack| (pack.info().pack_id, pack))
        .collect();
    for row in &batch.objects {
        if row.role != ObjectRole::WholeFile
            || row.base.is_some()
            || lost.contains(&row.object_id)
            || !signed.contains(&row.object_id)
        {
            continue;
        }
        if let Some(pack) = packs.get(&row.pack_id) {
            let width = record_width(pack.body(), row)?;
            costs.seed_full(row.object_id, row.canonical_length as u64, width);
        }
    }
    Ok(())
}
