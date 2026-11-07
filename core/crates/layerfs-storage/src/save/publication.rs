//! Reference closure, acknowledged upload and bounded atomic registration.
use super::state::State;
use crate::{
    error::{StorageError, StorageResult},
    location::ObjectLocation,
    port::{Publication, PublishedPack},
    source::Source,
};
use layerfs_content::ObjectId;
use std::collections::{BTreeMap, BTreeSet};
impl State<'_> {
    fn pack_cost(&self, pack: &PublishedPack) -> StorageResult<(usize, u64)> {
        let cost = self.storage.source.metadata.publication_pack_cost(pack)?;
        let body = pack.body().len() as u64;
        if cost.0 == 0
            || cost.0 > crate::policy::GROUP_COUNT_LIMIT + 1
            || cost.1 < body
            || cost.1 > body + 56 + 32 * crate::policy::GROUP_COUNT_LIMIT as u64
        {
            return Err(StorageError::Integrity("publication pack cost"));
        }
        Ok(cost)
    }
    pub(super) fn register_ready(&mut self) -> StorageResult<()> {
        let _work = self.storage.work.span(super::Stage::Publication);
        // Every reference to an unframed member forces its owning lane to seal.
        // Repeating this bounded closure walk discovers prerequisites of those seals;
        // it never repeats an engine operation.
        loop {
            let needed: Vec<_> = self
                .packer
                .ready
                .iter()
                .flat_map(|p| &p.members)
                .flat_map(|(_, m)| m.references.iter().copied())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            let before = self.packer.ready.len();
            let forced = self.packer.seal_pending(
                &needed,
                &mut self.compression,
                &mut self.next_pack,
                self.pack_end,
            )?;
            self.storage
                .source
                .note(|c| c.forced_seals += forced as u64);
            if self.packer.ready.len() == before {
                break;
            }
        }
        let members: BTreeMap<_, _> = self
            .packer
            .ready
            .iter()
            .flat_map(|p| &p.members)
            .map(|(l, m)| (l.object_id, (*l, m.references.clone(), m.prefix)))
            .collect();
        let external: Vec<_> = members
            .values()
            .flat_map(|(_, refs, _)| refs.iter().copied())
            .filter(|id| !members.contains_key(id))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        self.storage.source.locate(&external)?;
        let mut available = BTreeSet::new();
        for id in external {
            if self.storage.source.location(id, i64::MAX)?.is_none() {
                let parent = members
                    .iter()
                    .find(|(_, (_, refs, _))| refs.contains(&id))
                    .map(|(id, _)| *id)
                    .ok_or(StorageError::Integrity("closure parent"))?;
                return Err(StorageError::MissingDependency {
                    object: parent,
                    reference: id,
                });
            }
            available.insert(id);
        }
        let mut counts = BTreeMap::new();
        let mut dependents = BTreeMap::<ObjectId, Vec<ObjectId>>::new();
        let mut frontier = BTreeSet::new();
        for (id, (_, refs, _)) in &members {
            let unresolved: Vec<_> = refs
                .iter()
                .filter(|id| !available.contains(id))
                .copied()
                .collect();
            counts.insert(*id, unresolved.len());
            if unresolved.is_empty() {
                frontier.insert(*id);
            }
            for reference in unresolved {
                dependents.entry(reference).or_default().push(*id);
            }
        }
        let mut order = Vec::with_capacity(members.len());
        while let Some(id) = frontier.pop_first() {
            order.push(id);
            if let Some(children) = dependents.remove(&id) {
                for child in children {
                    let count = counts
                        .get_mut(&child)
                        .ok_or(StorageError::Integrity("closure count"))?;
                    *count -= 1;
                    if *count == 0 {
                        frontier.insert(child);
                    }
                }
            }
        }
        if order.len() != members.len() {
            return Err(StorageError::Integrity("registration reference cycle"));
        }
        let limits = self.storage.capacities();
        let mut batch = Publication::default();
        let mut canonical_bytes = 0_u64;
        let mut physical_bytes = 0_u64;
        let mut pack_rows = 0usize;
        let mut packs = BTreeSet::new();
        // Every value this Save assigned so far is published before any leaf
        // that may name it: in this batch or in one acknowledged before it.
        for (ordinal, value) in std::mem::take(&mut self.values) {
            if batch.inode_values.len() + 1 > limits.transaction_rows as usize {
                self.acknowledge(&mut batch, &members)?;
                batch = Publication::default();
            }
            batch
                .inode_values
                .push(crate::location::InodeValueRow { ordinal, value });
        }
        for id in order {
            let (location, _, _) = &members[&id];
            let sealed = self
                .packer
                .ready
                .iter()
                .find(|p| p.pack.info().pack_id == location.pack_id)
                .ok_or(StorageError::Integrity("registered pack missing"))?;
            let added_pack = !packs.contains(&location.pack_id);
            let (added_rows, body_bytes) = if added_pack {
                self.pack_cost(&sealed.pack)?
            } else {
                (0, 0)
            };
            let canonical_cost = location.canonical_length as u64;
            if (!batch.objects.is_empty() || !batch.packs.is_empty())
                && (canonical_bytes.saturating_add(canonical_cost) > limits.transaction_bytes
                    || physical_bytes.saturating_add(body_bytes)
                        > limits.transaction_physical_bytes
                    || batch.objects.len() + pack_rows + batch.inode_values.len() + added_rows + 1
                        > limits.transaction_rows as usize)
            {
                self.acknowledge(&mut batch, &members)?;
                batch = Publication::default();
                canonical_bytes = 0;
                physical_bytes = 0;
                pack_rows = 0;
            }
            if added_pack {
                let sealed = self
                    .packer
                    .ready
                    .iter()
                    .find(|p| p.pack.info().pack_id == location.pack_id)
                    .ok_or(StorageError::Integrity("registered pack missing"))?;
                batch.packs.push(sealed.pack.clone());
                packs.insert(location.pack_id);
                pack_rows += added_rows;
            }
            canonical_bytes += canonical_cost;
            physical_bytes += body_bytes;
            batch.objects.push(*location);
        }
        let pending_signatures: Vec<_> = self.signatures.borrow().values().copied().collect();
        for row in pending_signatures {
            if !members.contains_key(&row.object_id) && self.packer.pending(row.object_id) {
                continue;
            }
            if !members.contains_key(&row.object_id)
                && self
                    .storage
                    .source
                    .location(row.object_id, i64::MAX)?
                    .is_none()
            {
                return Err(StorageError::Integrity("signature names absent object"));
            }
            if batch.objects.len() + pack_rows + batch.inode_values.len() + batch.signatures.len()
                >= limits.transaction_rows as usize
            {
                self.acknowledge(&mut batch, &members)?;
                batch = Publication::default();
                pack_rows = 0;
            }
            batch.signatures.push(row);
        }
        if self.finishing && self.next_ordinal < self.ordinal_end {
            batch.release_ordinals = Some((
                u32::try_from(self.next_ordinal)
                    .map_err(|_| StorageError::Integrity("unused ordinal tail"))?,
                usize::try_from(self.ordinal_end - self.next_ordinal)
                    .map_err(|_| StorageError::Integrity("unused ordinal count"))?,
            ));
        }
        if !batch.objects.is_empty()
            || !batch.signatures.is_empty()
            || !batch.packs.is_empty()
            || !batch.inode_values.is_empty()
            || batch.release_ordinals.is_some()
        {
            self.acknowledge(&mut batch, &members)?;
        }
        self.packer.ready.clear();
        // Ordinary dependency bodies name sealed immutable packs. Publication
        // changes locator winners, not these bytes; keep bounded bodies across
        // waves. The private pooled tail still releases its own cache on append.
        Ok(())
    }
    fn acknowledge(
        &mut self,
        batch: &mut Publication,
        members: &BTreeMap<ObjectId, (ObjectLocation, Vec<ObjectId>, bool)>,
    ) -> StorageResult<()> {
        self.storage.source.note(|c| c.publish += 1);
        let result = {
            let _work = self.storage.work.span(super::Stage::Metadata);
            self.storage.source.metadata.publish(batch)?
        };
        self.storage.source.note(|c| {
            c.pack_write_bytes += batch
                .packs
                .iter()
                .map(|pack| pack.body().len() as u64)
                .sum::<u64>();
            c.pooled_groups += batch.inode_values.len() as u64;
        });
        let mut lost = BTreeSet::new();
        for id in &result.lost {
            if !batch.objects.iter().any(|row| row.object_id == *id) || !lost.insert(*id) {
                return Err(StorageError::Integrity("registration lost identities"));
            }
        }
        if !lost.is_empty() {
            self.depths = crate::encoding::delta::select::DepthCache::new();
            let ids: Vec<_> = lost.iter().copied().collect();
            let expected = ids
                .iter()
                .map(|id| self.resolve(*id))
                .collect::<StorageResult<Vec<_>>>()?;
            self.storage.source.invalidate(&ids);
            let actual = self.storage.reader()?.read_objects(&ids)?;
            for ((id, expected), actual) in ids.iter().zip(expected).zip(actual) {
                let winner = self
                    .storage
                    .source
                    .location(*id, i64::MAX)?
                    .ok_or(StorageError::ObjectMissing(*id))?;
                if actual != expected || winner.role != members[id].0.role {
                    return Err(StorageError::Collision(*id));
                }
            }
        }
        self.storage.source.published(batch, &lost)?;
        for row in &batch.objects {
            if lost.contains(&row.object_id) {
                self.outcome.reused += 1;
            } else {
                self.outcome.inserted += 1;
                self.outcome.canonical_bytes += row.canonical_length as u64;
                if members[&row.object_id].2 {
                    self.outcome.prefix_records += 1;
                } else {
                    self.outcome.full_records += 1;
                }
            }
        }
        self.outcome.metadata_bytes += batch
            .packs
            .iter()
            .filter(|pack| pack.info().domain == crate::location::PackDomain::Metadata)
            .map(|pack| pack.info().length as u64)
            .sum::<u64>();
        self.outcome.packs += batch.packs.len() as u64;
        self.outcome.stored_bytes += batch
            .packs
            .iter()
            .map(|pack| pack.info().length as u64)
            .sum::<u64>();
        for row in &batch.signatures {
            self.signatures.borrow_mut().remove(&row.slot);
        }
        Ok(())
    }
}
