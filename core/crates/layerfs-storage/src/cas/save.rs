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
    // The membership snapshot below is taken once, before any of this wave's own
    // writes, so a group sealed during the wave publishes rows the snapshot cannot
    // know about. Identities sealed earlier in this wave are therefore consulted
    // beside it: an identity whose row this wave already wrote must not reach
    // `offer` again, or the `objects` primary key refuses the second row.
    let whole = std::time::Instant::now();
    owner.sealed_rows.clear();
    let ids: Vec<ObjectId> = objects.iter().map(|object| object.id()).collect();
    let wave_started = std::time::Instant::now();
    let locations = {
        let _guard = crate::sqlite::ownership::lock(&owner.arbitration)?;
        lookup::locations(owner.connection(), &ids, i64::MAX)?
    };
    let mut by_id: BTreeMap<ObjectId, lookup::ObjectLocation> = BTreeMap::new();
    for location in locations {
        by_id.insert(location.object_id, location);
    }
    let mut availability = Availability::new(by_id.keys().copied());
    // One presence query for the whole wave (P2-5). Every direct reference its
    // offered objects carry is asked about together - the lookup pages its own
    // identifiers - so the check costs one query set per wave instead of one per
    // object that happens to name something outside it.
    let queries = {
        let _guard = crate::sqlite::ownership::lock(&owner.arbitration)?;
        availability.seed(
            owner.connection(),
            objects
                .iter()
                .flat_map(|object| object.references().iter().copied()),
            i64::MAX,
        )?
    };
    owner.note_presence_queries(queries);
    crate::cas::owner::SaveProfile::charge(&mut owner.profile.diag.wave_ns, wave_started);
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
            None => {
                // An identity with no row at the snapshot is one of two things: a
                // member still waiting in an open group, whose group is sealed on
                // demand, or a member a seal earlier in this same wave already wrote.
                // Both are answered from the row that now exists, through the same
                // exact byte comparison as any other reuse; only an identity that has
                // no row at all is offered for storage.
                let sealed = if owner.pending_member(object.id()) {
                    owner.seal_pending(std::slice::from_ref(&object.id()))?;
                    true
                } else {
                    owner.sealed_rows.contains(&object.id())
                };
                if sealed {
                    let location = {
                        let _guard = crate::sqlite::ownership::lock(&owner.arbitration)?;
                        lookup::location(owner.connection(), object.id(), i64::MAX)?
                            .ok_or(StorageError::Integrity("sealed identity has no row"))?
                    };
                    membership::reuse_or_collide(owner, object, location)?;
                    owner.note_reuse();
                } else {
                    let advisory: Vec<ObjectId> = object.predecessors().ids().collect();
                    owner.offer(object, &advisory, &mut availability)?
                }
            }
        }
    }
    owner.flush_candidates()?;
    crate::cas::owner::SaveProfile::charge(&mut owner.profile.diag.flush_batch_ns, whole);
    Ok(())
}
