//! Membership, exact reuse and one preparation wave.
use super::{source::WaveSource, state::State};
use crate::{
    encoding::{
        delta::read::{BodyCaches, ChainCounters, Resolver},
        pool::{branch::is_program_role, read::PooledCaches, PoolReader},
    },
    error::{StorageError, StorageResult},
    location::ObjectLocation,
    policy::READ_OBJECT_LIMIT,
    read::hint_failure,
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
        self.storage.source.begin_demand(false);
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
            // The stored bases, none of them private to the Save, of each
            // object that is not stored itself (an exact reuse reads none).
            let fetch = &self.storage.source;
            let bases: Vec<Vec<ObjectLocation>> = objects[start..end]
                .iter()
                .map(
                    |object| match fetch.located(std::iter::once(object.id())).is_empty() {
                        true => {
                            let ids = object.predecessors().ids();
                            fetch.located(ids.filter(|id| self.packer.location(*id).is_none()))
                        }
                        false => Vec::new(),
                    },
                )
                .collect();
            drop(membership);
            // Exact reuse and representation selection walk and authenticate
            // their own required chains. Consume those inputs with the object
            // demand: a wave-wide physical prewalk can exceed retention and
            // evict them before admission, or acquire candidates selection
            // never needs. When the provider batches chains (#758), the
            // objects are admitted a page at a time: one group read holds
            // their bases' chains, consumed before the next.
            let mut index = start;
            while index < end {
                let admission = self.storage.work.span(super::Stage::Admission);
                let pages = bases[index - start..].iter().map(Vec::as_slice);
                let next = match self.storage.source.page(pages, &mut self.packs)? {
                    0 => end,
                    covered => index + covered,
                };
                self.verify_bases(&bases[index - start..next - start])?;
                for at in index..next {
                    self.admit(&objects, at, &mut prepared)?;
                }
                drop(admission);
                index = next;
            }
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
    /// Rebuilds each object's first stored base that came with a chain,
    /// binding its members (#758), and keeps the bound locators as
    /// authoritative once the rebuild has hashed every element of the chain,
    /// so selection walks those chains with no locate per level. Selection
    /// still sees authoritative locators only: an element whose hash failed
    /// is never kept, the rebuild falls back to authoritative locates (a
    /// stale hint, counted), and a chain with no member is walked as before.
    /// A program page is rebuilt by a pool reader of its own (its counts are
    /// not the Save's): that reader has no rebuild, so a failure a hint may
    /// have caused drops every hint (counted) and selection walks that chain
    /// from authoritative locates, as it did before.
    fn verify_bases(&mut self, bases: &[Vec<ObjectLocation>]) -> StorageResult<()> {
        let fetch = &self.storage.source;
        let mut seen = BTreeSet::new();
        let chained: Vec<ObjectLocation> = bases
            .iter()
            .filter_map(|stored| stored.first().copied())
            .filter(|base| fetch.has_hint(base.object_id) && !self.depths.knows(base.object_id))
            // Pooled leaves bind no member (`Fetch::bind`).
            .filter(|base| base.role != layerfs_content::ObjectRole::InodeLeaf)
            .filter(|base| seen.insert(base.object_id))
            .collect();
        if chained.is_empty() {
            return Ok(());
        }
        let _bind = fetch.binding();
        let _pages = fetch.binding_pages();
        let view = WaveSource {
            fetch,
            packer: &self.packer,
            signatures: &self.signatures,
            values: &self.values,
        };
        let capacities = self.storage.capacities();
        let mut counters = ChainCounters::default();
        let mut pages = PoolReader::new();
        for base in chained {
            // A stale hint on an earlier base drops every hint.
            if !fetch.has_hint(base.object_id) {
                continue;
            }
            if is_program_role(base.role) {
                let caches = PooledCaches {
                    encoded: &mut self.packs,
                    decoded: &mut self.groups,
                };
                let rebuilt = pages.branch_canonical_with_caches(
                    &view,
                    &capacities,
                    i64::MAX,
                    &mut self.decode,
                    base,
                    Some(caches),
                );
                match rebuilt {
                    Ok(_) => {}
                    Err(error) if hint_failure(&error) && fetch.drop_hints(&[])? => continue,
                    Err(error) => return Err(error),
                }
            } else {
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
                    &mut counters,
                )
                .resolve_at(base)?;
            }
            fetch.promote_hints();
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
