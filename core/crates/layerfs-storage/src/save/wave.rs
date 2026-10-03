//! Membership, exact reuse and one preparation wave.
use super::{source::WaveSource, state::State};
use crate::{
    encoding::delta::read::{BodyCaches, Resolver},
    error::{StorageError, StorageResult},
    port::Reserve,
    source::Source,
};
use layerfs_content::{FinalizedObject, ObjectId};
use std::collections::{BTreeMap, BTreeSet};
impl State<'_> {
    pub(super) fn reserve_packs(&mut self, count: usize) -> StorageResult<()> {
        self.storage.source.note(|c| c.reserve += 1);
        let reserved = self.storage.source.metadata.reserve(Reserve {
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
        let roots = ids
            .iter()
            .filter_map(|id| self.storage.source.location(*id, i64::MAX).transpose())
            .collect::<StorageResult<Vec<_>>>()?;
        crate::read::chains(
            &self.storage.source,
            &roots,
            &mut self.packs,
            &mut self.decode,
        )?;
        let mut reserved = false;
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
                if !reserved {
                    self.reserve_packs(objects.len() + 5)?;
                    reserved = true;
                }
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
            };
            if let Some(location) = view.location(object.id(), i64::MAX)? {
                if location.role != object.role()
                    || location.canonical_length != object.canonical_len()
                    || self.resolve(object.id())? != object.canonical()
                {
                    return Err(StorageError::Collision(object.id()));
                }
                self.outcome.reused += 1;
            } else {
                if !reserved {
                    self.reserve_packs(objects.len() + 5)?;
                    reserved = true;
                }
                self.offer(object)?;
            }
        }
        self.packer.flush(&mut self.next_pack, self.pack_end)?;
        self.flush_signatures()?;
        self.register_ready()
    }
    pub(super) fn flush_signatures(&mut self) -> StorageResult<()> {
        let view = WaveSource {
            fetch: &self.storage.source,
            packer: &self.packer,
            signatures: &self.signatures,
        };
        self.candidates.flush(&view)?;
        Ok(())
    }
    pub(super) fn resolve(&mut self, id: ObjectId) -> StorageResult<Vec<u8>> {
        let view = WaveSource {
            fetch: &self.storage.source,
            packer: &self.packer,
            signatures: &self.signatures,
        };
        let capacities = self.storage.capacities();
        Resolver::new(
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
        .map(|v| v.0)
    }
}
