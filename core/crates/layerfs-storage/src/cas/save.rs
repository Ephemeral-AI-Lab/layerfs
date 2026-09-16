//! Real batched save coordination.
//!
//! A preparation wave resolves membership for its offered identities with paged
//! queries, serves every exact reuse by byte comparison, and prepares the
//! remainder through dependency checks, FULL encoding, placement and one selected
//! pack write. Cross-file and cross-batch batching is preserved: nothing here
//! opens a transaction per object, per group or per file.

use std::collections::BTreeMap;

use layerfs_content::ObjectId;

use layerfs_content::FinalizedObject;

use crate::cas::dependencies::Availability;
use crate::cas::membership;
use crate::cas::owner::MutationOwner;
use crate::error::{StorageError, StorageResult};
use crate::sqlite::lookup;

/// Resolves and stores one preparation wave.
pub fn flush_batch(owner: &mut MutationOwner, objects: Vec<FinalizedObject>) -> StorageResult<()> {
    if objects.is_empty() {
        return Ok(());
    }
    let ids: Vec<ObjectId> = objects.iter().map(|object| object.id()).collect();
    let locations = lookup::locations(owner.connection(), &ids, i64::MAX)?;
    let mut by_id: BTreeMap<ObjectId, lookup::ObjectLocation> = BTreeMap::new();
    for location in locations {
        by_id.insert(location.object_id, location);
    }
    let mut availability = Availability::new(by_id.keys().copied());
    // A wave may carry the same identity several times. The first occurrence
    // decides the row; every later occurrence still receives the required exact
    // comparison against the bytes that were actually prepared for this identity.
    let mut prepared: BTreeMap<ObjectId, usize> = BTreeMap::new();
    for index in 0..objects.len() {
        if let Some(prior) = prepared.get(&objects[index].id()).copied() {
            if objects[prior].canonical() != objects[index].canonical() {
                return Err(StorageError::Collision(objects[index].id()));
            }
            owner.note_reuse();
            continue;
        }
        let object = objects[index].clone();
        prepared.insert(object.id(), index);
        match by_id.get(&object.id()).copied() {
            Some(location) => {
                membership::reuse_or_collide(owner, &object, location)?;
                owner.note_reuse();
            }
            None => owner.offer(object, &mut availability)?,
        }
    }
    Ok(())
}
