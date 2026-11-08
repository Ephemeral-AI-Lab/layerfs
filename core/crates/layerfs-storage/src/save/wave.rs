//! Membership, exact reuse and one preparation wave.
use super::{source::WaveSource, state::State};
use crate::{
    encoding::delta::read::{BodyCaches, Resolver},
    error::{StorageError, StorageResult},
    policy::READ_OBJECT_LIMIT,
    source::Source,
};
use layerfs_content::{FinalizedObject, ObjectId};
use std::collections::{BTreeMap, BTreeSet};
impl State<'_> {
    pub(super) fn reserve_packs(&mut self, required: usize) -> StorageResult<()> {
        if required as i64 <= self.pack_end.saturating_sub(self.next_pack) {
            return Ok(());
        }
        let _work = self.storage.work.span(super::Stage::PackReserve);
        self.storage.source.note(|c| {
            c.reserve += 1;
            c.reservation_refills += 1;
        });
        let count = required.max(self.storage.reservations.packs);
        let reserved = self.storage.source.metadata.reserve(crate::port::Reserve {
            packs: count,
            ordinals: 0,
        })?;
        if reserved.first_pack_id <= 0 {
            return Err(StorageError::Integrity("pack reservation"));
        }
        self.next_pack = reserved.first_pack_id;
        self.pack_end = self
            .next_pack
            .checked_add(
                i64::try_from(count)
                    .map_err(|_| StorageError::Integrity("pack allocation count"))?,
            )
            .ok_or(StorageError::Integrity("pack allocation overflow"))?;
        Ok(())
    }
    pub(super) fn wave(&mut self, objects: Vec<FinalizedObject>) -> StorageResult<()> {
        if objects.is_empty() {
            return Ok(());
        }
        self.storage.source.begin_demand();
        let mut prepared = BTreeMap::<ObjectId, usize>::new();
        let mut start = 0;
        while start < objects.len() {
            // Membership is located a slice at a time, each slice's ids within
            // the locator cache, so admission still finds every reference
            // located: a wave of mapping pages can name far more payloads than
            // the cache holds, and one demand for all of them would evict most
            // before their pages are admitted, each then located alone.
            let membership = self.storage.work.span(super::Stage::Membership);
            let (end, ids) = demand(&objects, start);
            self.storage.source.locate(&ids)?;
            // Exact reuse and representation selection walk and authenticate
            // their own required chains. Consume those inputs with the object
            // demand: a wave-wide physical prewalk can exceed retention and
            // evict them before admission, or acquire candidates selection
            // never needs.
            drop(membership);
            let admission = self.storage.work.span(super::Stage::Admission);
            for index in start..end {
                self.admit(&objects, index, &mut prepared)?;
            }
            drop(admission);
            start = end;
        }
        let flush = self.storage.work.span(super::Stage::Flush);
        // Keep the existing bounded partial queues until pressure, dependency
        // closure or finish requires them. Ready packs still publish this wave.
        self.flush_signatures()?;
        drop(flush);
        self.register_ready()
    }
    /// Admits `objects[index]`: a duplicate in the wave, an exact reuse or a
    /// new object offered for selection.
    fn admit(
        &mut self,
        objects: &[FinalizedObject],
        index: usize,
        prepared: &mut BTreeMap<ObjectId, usize>,
    ) -> StorageResult<()> {
        let object = &objects[index];
        if let Some(prior) = prepared.get(&object.id()).copied() {
            if objects[prior].canonical() != object.canonical()
                || objects[prior].role() != object.role()
            {
                return Err(StorageError::Collision(object.id()));
            }
            self.outcome.reused += 1;
            return Ok(());
        }
        prepared.insert(object.id(), index);
        // Keep the allocation block, but check the operation that can consume
        // IDs rather than replacing a tail for an entire hypothetical wave.
        if self.packer.pending(object.id()) {
            let required = self.packer.finish_pack_bound();
            self.reserve_packs(required)?;
            self.packer.seal_pending(
                &[object.id()],
                &mut self.compression,
                &mut self.next_pack,
                self.pack_end,
            )?;
        }
        let view = WaveSource {
            fetch: &self.storage.source,
            packer: &self.packer,
            signatures: &self.signatures,
            values: &self.values,
        };
        if let Some(location) = view.location(object.id(), i64::MAX)? {
            // Exact reuse trusts the content address (owner, 2026-10-19;
            // #352 record 034 A-1): the id is the hash of the canonical
            // bytes, so a stored object with this id, role and length is
            // not read back. A role or length mismatch is still refused
            // here. A corrupt stored copy fails later, at its first read,
            // where every read authenticates bytes against the id.
            if location.role != object.role() || location.canonical_length != object.canonical_len()
            {
                return Err(StorageError::Collision(object.id()));
            }
            self.outcome.reused += 1;
        } else {
            // One new object group plus every currently unfinished group/queue.
            let required = self.packer.finish_pack_bound() + 1;
            self.reserve_packs(required)?;
            self.offer(object)?;
        }
        Ok(())
    }
    pub(super) fn flush_signatures(&mut self) -> StorageResult<()> {
        let view = WaveSource {
            fetch: &self.storage.source,
            packer: &self.packer,
            signatures: &self.signatures,
            values: &self.values,
        };
        self.candidates.flush(&view)?;
        Ok(())
    }
    pub(super) fn resolve(&mut self, id: ObjectId) -> StorageResult<Vec<u8>> {
        if let Some(value) = self.resolved.get(id) {
            return Ok(value);
        }
        let view = WaveSource {
            fetch: &self.storage.source,
            packer: &self.packer,
            signatures: &self.signatures,
            values: &self.values,
        };
        let capacities = self.storage.capacities();
        let value = Resolver::new(
            &view,
            i64::MAX,
            &capacities,
            BodyCaches {
                packs: &mut self.packs,
                pool: &mut self.pool,
            },
            &mut self.groups,
            &mut self.decode,
            &mut self.chain,
        )
        .resolve(id)
        .map(|v| v.0)?;
        self.resolved.keep(id, &value);
        Ok(value)
    }
}

/// The end of the slice of `objects` from `start` whose ids, direct
/// references and predecessors fit the locator cache together, and those ids
/// sorted. A slice holds at least one object, however many ids it names.
fn demand(objects: &[FinalizedObject], start: usize) -> (usize, Vec<ObjectId>) {
    let mut ids = BTreeSet::new();
    let mut end = start;
    for object in &objects[start..] {
        let named = || {
            std::iter::once(object.id())
                .chain(object.references().iter().copied())
                .chain(object.predecessors().ids())
        };
        let added = named().filter(|id| !ids.contains(id)).count();
        if end > start && ids.len() + added > READ_OBJECT_LIMIT {
            break;
        }
        ids.extend(named());
        end += 1;
    }
    (end, ids.into_iter().collect())
}
