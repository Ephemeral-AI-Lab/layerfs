//! Membership, exact reuse and one preparation wave.
use super::{super::state::State, source::WaveSource};
use crate::{
    encoding::{
        delta::read::{BodyCaches, ChainCounters, Resolver},
        pool::{program_page::is_program_role, read::PooledCaches, PoolReader},
    },
    error::{StorageError, StorageResult},
    location::ObjectLocation,
    policy::READ_OBJECT_LIMIT,
    read::hint_failure,
    source::Source,
};
use layerfs_content::{FinalizedObject, ObjectId, ObjectRole};
use std::collections::{BTreeMap, BTreeSet};
impl State<'_> {
    pub(crate) fn reserve_packs(&mut self, required: usize) -> StorageResult<()> {
        if required as i64 <= self.pack_end.saturating_sub(self.next_pack) {
            return Ok(());
        }
        let _work = self.storage.work.span(super::super::Stage::PackReserve);
        self.storage.source.note(|c| {
            c.reserve += 1;
            c.reservation_refills += 1;
        });
        let count = required.max(self.storage.reservations.packs);
        let reserved = self.storage.source.metadata.reserve(crate::port::Reserve {
            packs: count,
            ordinals: 0,
        });
        let reserved = self.written(reserved)?;
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
    pub(crate) fn wave(&mut self, objects: Vec<FinalizedObject>) -> StorageResult<()> {
        if objects.is_empty() {
            return Ok(());
        }
        self.prepare(objects)?;
        self.publish_wave()
    }

    /// Publishes ready packs and the acknowledged bases needed to place held objects.
    pub(crate) fn publish_wave(&mut self) -> StorageResult<()> {
        let flush = self.storage.work.span(super::super::Stage::Flush);
        // Keep the existing bounded partial queues until pressure, dependency
        // closure or finish requires them. Ready packs still publish this wave,
        // with the bases the held objects wait for (#810).
        self.seal_deferred_bases()?;
        self.flush_signatures()?;
        drop(flush);
        self.register_ready(false)?;
        self.readmit_deferred()
    }
    /// Seals the lanes that hold the bases of the held objects, so the next
    /// publication publishes them (`save/deferred.rs`).
    fn seal_deferred_bases(&mut self) -> StorageResult<()> {
        if self.deferred.is_empty() {
            return Ok(());
        }
        let bases = self.deferred.bases();
        let required = self.packer.finish_pack_bound();
        self.reserve_packs(required)?;
        self.packer.seal_pending(
            &bases,
            &mut self.compression,
            &mut self.next_pack,
            self.pack_end,
        )?;
        Ok(())
    }
    /// Selects every held object, once a publication published its base:
    /// located together first, then each admitted as `admit` would.
    fn readmit_deferred(&mut self) -> StorageResult<()> {
        if self.deferred.is_empty() {
            return Ok(());
        }
        let held = self.deferred.take();
        let ids: BTreeSet<ObjectId> = held
            .iter()
            .flat_map(|(object, base)| [object.id(), *base])
            .collect();
        let ids: Vec<ObjectId> = ids.into_iter().collect();
        for slice in ids.chunks(READ_OBJECT_LIMIT / 2) {
            self.storage.source.locate(slice)?;
        }
        for (object, base) in &held {
            self.place(object, Some(*base))?;
        }
        Ok(())
    }
    /// One wave's membership, exact reuse and selection, without publishing
    /// it: `Save::finish` publishes its last wave with the sealed tail, in
    /// one publication rather than two (#768).
    pub(crate) fn prepare(&mut self, objects: Vec<FinalizedObject>) -> StorageResult<()> {
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
            let membership = self.storage.work.span(super::super::Stage::Membership);
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
                let admission = self.storage.work.span(super::super::Stage::Admission);
                let pages = bases[index - start..].iter().map(Vec::as_slice);
                let next = match self.storage.source.page(pages, &mut self.packs)? {
                    0 => end,
                    covered => index + covered,
                };
                let page = &bases[index - start..next - start];
                self.verify_bases(&objects[index..next], page)?;
                for at in index..next {
                    self.prepare_bases(&objects[at..next], &page[at - index..])?;
                    self.admit(&objects, at, &mut prepared)?;
                }
                drop(admission);
                index = next;
            }
            start = end;
        }
        Ok(())
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
        // A held object is this Save's, as a pending one is (#810).
        if let Some(held) = self.deferred.canonical(object.id()) {
            if held != object.canonical() {
                return Err(StorageError::Collision(object.id()));
            }
            self.outcome.reused += 1;
            return Ok(());
        }
        // Keep the allocation block, but check the operation that can consume
        // IDs rather than replacing a tail for an entire hypothetical wave.
        self.place(object, None)
    }
    /// Places `object`, not seen before in its wave: an exact reuse, or a new
    /// object offered for selection (with `base`, a held one: `offer`).
    fn place(&mut self, object: &FinalizedObject, base: Option<ObjectId>) -> StorageResult<()> {
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
            // A held root has no pack yet. Place it before admitting a
            // dependent or selecting against it as a declared predecessor.
            if !self.deferred.is_empty()
                && object
                    .references()
                    .iter()
                    .copied()
                    .chain(object.predecessors().ids())
                    .any(|id| self.deferred.holds(id))
            {
                self.publish_wave()?;
            }
            // One new object group plus every currently unfinished group/queue.
            let required = self.packer.finish_pack_bound() + 1;
            self.reserve_packs(required)?;
            self.offer(object, base)?;
        }
        Ok(())
    }
    /// Rebuilds the stored base of each object whose chain selection may walk
    /// and that came with a chain, binding its members (#758), and keeps the
    /// bound locators as authoritative once the rebuild has hashed every
    /// element of the chain, so selection walks those chains with no locate
    /// per level. Selection still sees authoritative locators only: an
    /// element whose hash failed is never kept, the rebuild falls back to
    /// authoritative locates (a stale hint, counted), and a chain with no
    /// member is walked as before. A program page is rebuilt by a pool
    /// reader of its own (its counts are not the Save's): that reader has no
    /// rebuild, so a failure at an element a hint located drops every hint
    /// (counted) and selection walks that chain from authoritative locates,
    /// as it did before; any other failure is the Save's.
    fn verify_bases(
        &mut self,
        objects: &[FinalizedObject],
        bases: &[Vec<ObjectLocation>],
    ) -> StorageResult<()> {
        let fetch = &self.storage.source;
        let mut seen = BTreeSet::new();
        let chained: Vec<ObjectLocation> = objects
            .iter()
            .zip(bases)
            .filter_map(|(object, stored)| self.walked_base(object, stored))
            .filter(|base| fetch.has_hint(base.object_id))
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
                let stale_before = fetch.counters().chain_hint_stale;
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
                    Err(error) if hint_failure(&error) && pages.hint_blamed() => {
                        // One stale hint, counted once.
                        let counted = fetch.counters().chain_hint_stale > stale_before;
                        fetch.hint_stale(counted)?;
                        continue;
                    }
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
    /// The stored base whose chain selection may walk for `object`, among
    /// `stored` (its stored predecessors, none private, in order), if it is
    /// one a Save checks. Wider than selection: `object` may still turn out
    /// private to this Save or repeated in the wave (never selected), and an
    /// ordinary role whose selection walks no base is checked too. A program
    /// page's is its first advisory predecessor when stored with the page's
    /// role (`encoding::delta::branch::trial`);
    /// any other object's is its first stored predecessor of an ordinary
    /// role, unless the Save already knows its depth. Pooled leaves bind no
    /// member (`Fetch::bind`).
    fn walked_base(
        &self,
        object: &FinalizedObject,
        stored: &[ObjectLocation],
    ) -> Option<ObjectLocation> {
        let base = stored.first().copied()?;
        if is_program_role(object.role()) {
            let first = object.predecessors().ids().next()?;
            return (base.object_id == first && base.role == object.role()).then_some(base);
        }
        (base.role != ObjectRole::InodeLeaf
            && !is_program_role(base.role)
            && !self.depths.knows(base.object_id))
        .then_some(base)
    }
    pub(crate) fn flush_signatures(&mut self) -> StorageResult<()> {
        let view = WaveSource {
            fetch: &self.storage.source,
            packer: &self.packer,
            signatures: &self.signatures,
            values: &self.values,
        };
        self.candidates.flush(&view)?;
        Ok(())
    }
    /// Before `objects[0]` is admitted: when it is an inode leaf whose stored
    /// base (`bases[0]`, its located predecessors) is not rebuilt ahead yet,
    /// rebuilds ahead the stored bases of it and of the inode leaves after it
    /// in `objects`, with their values in one lookup a window at a time
    /// ([`Self::prepare_leaves`]): selection (`pooled_bases`) then reads each
    /// base without a lookup of its own (#810 F4). Reads only; selection reads
    /// and checks the same bytes either way.
    fn prepare_bases(
        &mut self,
        objects: &[FinalizedObject],
        bases: &[Vec<ObjectLocation>],
    ) -> StorageResult<()> {
        let leaf = |object: &FinalizedObject| object.role() == ObjectRole::InodeLeaf;
        match (
            objects.first(),
            bases.first().and_then(|bases| bases.first()),
        ) {
            (Some(object), Some(base))
                if leaf(object) && !self.pool.is_prepared(&base.object_id) =>
            {
                let ahead: Vec<ObjectId> = objects
                    .iter()
                    .zip(bases)
                    .filter(|(object, _)| leaf(object))
                    .flat_map(|(_, bases)| bases.iter().map(|base| base.object_id))
                    .collect();
                self.prepare_leaves(&ahead, false)
            }
            _ => Ok(()),
        }
    }
    /// Before `ids` are read: rebuilds ahead the pooled inode leaves among
    /// them (when `resolving` their canonical bytes, those this Save has not
    /// resolved yet; selection reads the pooled bodies, which that cache does
    /// not keep) and looks up their stored values together, a bounded window at a time
    /// (`PoolReader::prepare_leaves`, as a Store reader's demand does, #95):
    /// one value lookup per window instead of one per leaf (#810 F4). Each
    /// leaf's own resolution still authenticates it.
    pub(super) fn prepare_leaves(
        &mut self,
        ids: &[ObjectId],
        resolving: bool,
    ) -> StorageResult<()> {
        let view = WaveSource {
            fetch: &self.storage.source,
            packer: &self.packer,
            signatures: &self.signatures,
            values: &self.values,
        };
        let mut leaves = Vec::new();
        for id in ids {
            if resolving && self.resolved.holds(*id) {
                continue;
            }
            // Only where it is known already: sealed here, or located in the
            // Store. A pending object is not looked up, so preparing asks
            // the Store nothing the demand itself would not.
            if let Some(location) = self.leaf_location(*id) {
                if !leaves.contains(&location) {
                    leaves.push(location);
                }
            }
        }
        if leaves.len() < 2 {
            return Ok(());
        }
        // A Save read has already located its stored chains in read_inner.
        // For selection, group the stored leaf bases here. The wave starts
        // with binding disabled, and its guarded base verification has
        // finished before selection, so a walk failure is authoritative.
        if !resolving {
            let stored: Vec<ObjectLocation> = leaves
                .iter()
                .filter(|leaf| self.packer.location(leaf.object_id).is_none())
                .copied()
                .collect();
            if stored.len() > 1 {
                crate::read::prefetch_chains(
                    &self.storage.source,
                    &stored,
                    &mut self.packs,
                    &mut self.decode,
                    &mut self.groups,
                )?;
            }
        }
        let capacities = self.storage.capacities();
        self.pool.prepare_leaves(
            &view,
            &capacities,
            i64::MAX,
            &mut self.decode,
            &leaves,
            Some(crate::encoding::pool::read::PooledCaches {
                encoded: &mut self.packs,
                decoded: &mut self.groups,
            }),
        )
    }
    fn leaf_location(&self, id: ObjectId) -> Option<ObjectLocation> {
        self.packer
            .location(id)
            .or_else(|| self.storage.source.located(std::iter::once(id)).pop())
            .filter(|location| location.role == ObjectRole::InodeLeaf)
    }

    /// Prepares the next value window as its first unresolved leaf is read.
    pub(crate) fn resolve_demand(
        &mut self,
        id: ObjectId,
        remaining: &[ObjectId],
    ) -> StorageResult<Vec<u8>> {
        if !self.resolved.holds(id)
            && !self.pool.is_prepared(&id)
            && self.leaf_location(id).is_some()
        {
            self.prepare_leaves(remaining, true)?;
        }
        self.resolve(id)
    }

    pub(crate) fn resolve(&mut self, id: ObjectId) -> StorageResult<Vec<u8>> {
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
