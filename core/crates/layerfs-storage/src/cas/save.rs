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
    // One presence query for the whole wave (P2-5). Every direct reference its
    // offered objects carry is asked about together - the lookup pages its own
    // identifiers - so the check costs one query set per wave instead of one per
    // object that happens to name something outside it.
    availability
        .seed(
            owner.connection(),
            objects
                .iter()
                .flat_map(|object| object.references().iter().copied()),
            i64::MAX,
        )
        .map(|queries| owner.note_presence_queries(queries))?;
    // A wave may carry the same identity several times. The first occurrence
    // decides the row; every later occurrence still receives the required exact
    // comparison against the bytes that were actually prepared for this identity.
    // The same holds across waves: an identity whose group is still open has no row
    // yet, so it is sealed on demand and resolved through the ordinary verified path
    // instead of being inserted a second time. Inserting a second row is a hard
    // constraint failure, and a store that fails on repeated content fails on
    // exactly the workload it exists for.
    let mut prepared: BTreeMap<ObjectId, usize> = BTreeMap::new();
    for index in 0..objects.len() {
        if let Some(prior) = prepared.get(&objects[index].id()).copied() {
            if objects[prior].canonical() != objects[index].canonical() {
                return Err(StorageError::Collision(objects[index].id()));
            }
            owner.note_reuse();
            continue;
        }
        // The wave keeps ownership of every offered object: the owner reads the
        // identity, role, canonical bytes and length, so no per-object copy of the
        // canonical record is made on the prepared path.
        let object = &objects[index];
        prepared.insert(object.id(), index);
        match by_id.get(&object.id()).copied() {
            Some(location) => {
                membership::reuse_or_collide(owner, object, location)?;
                owner.note_reuse();
            }
            None if owner.pending_member(object.id()) => {
                owner.seal_pending(std::slice::from_ref(&object.id()))?;
                let location = lookup::locations(owner.connection(), &[object.id()], i64::MAX)?
                    .into_iter()
                    .next()
                    .ok_or(StorageError::Integrity("sealed identity has no row"))?;
                membership::reuse_or_collide(owner, object, location)?;
                owner.note_reuse();
            }
            None => {
                let advisory: Vec<ObjectId> = object.predecessors().ids().collect();
                owner.offer(object, &advisory, &mut availability)?
            }
        }
    }
    Ok(())
}
