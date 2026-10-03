//! Reference closure, acknowledged upload and bounded atomic registration.
use super::state::State;
use crate::{
    error::{StorageError, StorageResult},
    location::{ObjectLocation, PackDomain},
    port::{RegisteredPack, Registration},
    source::Source,
};
use layerfs_content::ObjectId;
use std::collections::{BTreeMap, BTreeSet};
impl State<'_> {
    pub(super) fn register_ready(&mut self) -> StorageResult<()> {
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
        let mut batch = Registration::default();
        let mut bytes = 0_u64;
        let mut packs = BTreeSet::new();
        for id in order {
            let (location, _, _) = &members[&id];
            let pack = self
                .packer
                .ready
                .iter()
                .find(|p| p.info.pack_id == location.pack_id)
                .ok_or(StorageError::Integrity("registered pack missing"))?;
            let info = pack.info;
            let added_pack = !packs.contains(&location.pack_id);
            let body_bytes = if added_pack && info.domain == PackDomain::Metadata {
                info.length as u64
            } else {
                0
            };
            let cost = location.canonical_length as u64 + body_bytes;
            if !batch.objects.is_empty()
                && (bytes.saturating_add(cost) > limits.transaction_bytes
                    || batch.objects.len() + batch.packs.len() + usize::from(added_pack) + 1
                        > limits.transaction_rows as usize)
            {
                self.acknowledge(&batch, &members)?;
                batch = Registration::default();
                bytes = 0;
            }
            if added_pack {
                let pack = self
                    .packer
                    .ready
                    .iter()
                    .find(|p| p.info.pack_id == location.pack_id)
                    .ok_or(StorageError::Integrity("registered pack missing"))?;
                if pack.info.domain == PackDomain::Payload {
                    self.storage.source.note(|c| {
                        c.puts += 1;
                        c.put_bytes += pack.body.len() as u64;
                    });
                    self.storage
                        .source
                        .objects
                        .put_if_absent(pack.info.key, &pack.body)?;
                }
                batch.packs.push(RegisteredPack {
                    info: pack.info,
                    body: (pack.info.domain == PackDomain::Metadata).then(|| pack.body.clone()),
                });
                packs.insert(location.pack_id);
            }
            bytes += cost;
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
            if batch.objects.len() + batch.packs.len() + batch.signatures.len()
                >= limits.transaction_rows as usize
            {
                self.acknowledge(&batch, &members)?;
                batch = Registration::default();
            }
            batch.signatures.push(row);
        }
        if !batch.objects.is_empty() || !batch.signatures.is_empty() {
            self.acknowledge(&batch, &members)?;
        }
        self.packer.ready.clear();
        self.packs.clear();
        Ok(())
    }
    fn acknowledge(
        &mut self,
        batch: &Registration,
        members: &BTreeMap<ObjectId, (ObjectLocation, Vec<ObjectId>, bool)>,
    ) -> StorageResult<()> {
        self.storage.source.note(|c| c.register += 1);
        let result = self.storage.source.metadata.register(batch)?;
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
        let ids: Vec<_> = batch.objects.iter().map(|l| l.object_id).collect();
        self.storage.source.invalidate(&ids);
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
        self.outcome.packs += batch.packs.len() as u64;
        for row in &batch.signatures {
            self.signatures.borrow_mut().remove(&row.slot);
        }
        Ok(())
    }
}
