//! Membership, exact reuse and one preparation wave.
use super::{source::WaveSource, state::State};
use crate::{
    encoding::delta::read::{BodyCaches, Resolver},
    error::{StorageError, StorageResult},
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
        let membership = self.storage.work.span(super::Stage::Membership);
        self.storage.source.begin_demand();
        let ids: Vec<_> = objects
            .iter()
            .flat_map(|o| {
                std::iter::once(o.id())
                    .chain(o.references().iter().copied())
                    .chain(o.predecessors().ids())
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        self.storage.source.locate(&ids)?;
        // Exact reuse and representation selection walk and authenticate their
        // own required chains. Consume those inputs with the object demand:
        // a wave-wide physical prewalk can exceed retention and evict them
        // before admission, or acquire candidates selection never needs.
        drop(membership);
        let admission = self.storage.work.span(super::Stage::Admission);
        // Keep the allocation block, but check the operation that can consume
        // IDs rather than replacing a tail for an entire hypothetical wave.
        let mut prepared = BTreeMap::<ObjectId, usize>::new();
        for (index, object) in objects.iter().enumerate() {
            if let Some(prior) = prepared.get(&object.id()).copied() {
                if objects[prior].canonical() != object.canonical()
                    || objects[prior].role() != object.role()
                {
                    return Err(StorageError::Collision(object.id()));
                }
                self.outcome.reused += 1;
                continue;
            }
            prepared.insert(object.id(), index);
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
                prefer_packer: false,
            };
            if let Some(location) = view.location(object.id(), i64::MAX)? {
                // Exact reuse trusts the content address (owner, 2026-10-19;
                // #352 record 034 A-1): the id is the hash of the canonical
                // bytes, so a stored object with this id, role and length is
                // not read back. A role or length mismatch is still refused
                // here. A corrupt stored copy fails later, at its first read,
                // where every read authenticates bytes against the id.
                if location.role != object.role()
                    || location.canonical_length != object.canonical_len()
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
        }
        drop(admission);
        let flush = self.storage.work.span(super::Stage::Flush);
        // Keep the existing bounded partial queues until pressure, dependency
        // closure or finish requires them. Ready packs still publish this wave.
        self.flush_signatures()?;
        drop(flush);
        self.register_ready()
    }
    pub(super) fn flush_signatures(&mut self) -> StorageResult<()> {
        let view = WaveSource {
            fetch: &self.storage.source,
            packer: &self.packer,
            signatures: &self.signatures,
            values: &self.values,
            prefer_packer: false,
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
            prefer_packer: true,
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
