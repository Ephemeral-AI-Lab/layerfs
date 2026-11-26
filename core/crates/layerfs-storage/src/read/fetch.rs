//! Locator caching, batched metadata acquisition and complete-pack integrity.

use super::{locator_cache::LocatorCache, Diagnostics};
use crate::{
    error::{StorageError, StorageResult},
    location::{ObjectLocation, PackDomain, PackInfo, SignatureRow},
    pack::layout,
    policy::{DEPENDENCY_PACK_CACHE_BYTES, READ_OBJECT_LIMIT, SINGLETON_PACK_LIMIT},
    port::PackPersistence,
    source::Source,
};
use layerfs_content::{inode_leaf::INODE_VALUE_BYTES, ObjectId};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};

/// Pack bodies the Saves of this handle published, newest last, so a later
/// Save's own reads and the readers that continue a Save
/// ([`crate::Storage::reader_after_save`]: a Commit's logical count and
/// binding hints) need not fetch them back. Bounded by
/// [`DEPENDENCY_PACK_CACHE_BYTES`], with metadata retained before payloads.
/// The bodies are the published `Arc`s, added after acknowledged publication.
/// A Save that does not finish clears them (#768): the next Save starts from
/// the Store. Any other reader still acquires every body it needs.
#[derive(Default)]
struct Published {
    bodies: VecDeque<(PackInfo, Arc<Vec<u8>>)>,
    bytes: usize,
    payload_bytes: usize,
    /// Pooled inode values it published, oldest first, at most
    /// [`PUBLISHED_VALUES`] of them.
    values: BTreeMap<u32, [u8; INODE_VALUE_BYTES]>,
    order: VecDeque<u32>,
}

/// Published inode values kept: [`crate::policy::POOLED_VALUE_CACHE_BYTES`]
/// of value bytes.
const PUBLISHED_VALUES: usize = crate::policy::POOLED_VALUE_CACHE_BYTES / INODE_VALUE_BYTES;

impl Published {
    fn value(&mut self, ordinal: u32, value: [u8; INODE_VALUE_BYTES]) {
        if self.values.insert(ordinal, value).is_none() {
            self.order.push_back(ordinal);
        }
        while self.order.len() > PUBLISHED_VALUES {
            if let Some(old) = self.order.pop_front() {
                self.values.remove(&old);
            }
        }
    }
    fn get(&self, id: i64) -> Option<&(PackInfo, Arc<Vec<u8>>)> {
        self.bodies.iter().find(|(info, _)| info.pack_id == id)
    }
    fn insert(&mut self, info: PackInfo, body: Arc<Vec<u8>>) {
        // A body is kept once: a held tail's bodies live only in `Held`
        // until its publication is acknowledged (#768), and a later
        // acknowledgement of the same pack adds no second copy.
        let payload = info.domain == PackDomain::Payload;
        let reserved = if payload {
            self.bytes - self.payload_bytes
        } else {
            0
        };
        if body.len() > DEPENDENCY_PACK_CACHE_BYTES - reserved || self.get(info.pack_id).is_some() {
            return;
        }
        while self.bytes + body.len() > DEPENDENCY_PACK_CACHE_BYTES {
            let at = self
                .bodies
                .iter()
                .position(|(info, _)| info.domain == PackDomain::Payload)
                .unwrap_or(0);
            let Some((removed, old)) = self.bodies.remove(at) else {
                break;
            };
            self.bytes -= old.len();
            if removed.domain == PackDomain::Payload {
                self.payload_bytes -= old.len();
            }
        }
        self.bytes += body.len();
        if payload {
            self.payload_bytes += body.len();
        }
        self.bodies.push_back((info, body));
    }
}

/// Restores a flag's earlier value when its scope ends: the serving state
/// of a continuing reader's call, or the Save mode of a Save's own reads.
pub(crate) struct Restore<'a>(&'a Cell<bool>, bool);
impl Drop for Restore<'_> {
    fn drop(&mut self) {
        self.0.set(self.1);
    }
}

pub(crate) struct Fetch {
    pub(crate) metadata: Arc<dyn PackPersistence>,
    pub(super) locators: RefCell<LocatorCache>,
    pub(super) absent: RefCell<BTreeSet<ObjectId>>,
    pub(super) descriptors: RefCell<BTreeMap<i64, PackInfo>>,
    signatures: RefCell<Option<Vec<SignatureRow>>>,
    published: RefCell<Published>,
    /// A Save's last publication held for its caller's transaction
    /// (`held.rs`), served to every read before the caches and the Store.
    pub(super) held: RefCell<Option<super::held::Held>>,
    /// Set while a Save, or a reader that continues one, reads.
    pub(super) serve_published: Cell<bool>,
    counters: Cell<Diagnostics>,
    /// Chain hints from chain locates (#758).
    pub(super) hints: RefCell<super::hints::Hints>,
    /// The handle's chain depth bounds, once its policy is read.
    pub(crate) capacities: Cell<Option<crate::policy::StorageCapacities>>,
    /// Tests and benches: a stale hint fails as `delta chain hint`.
    pub(crate) strict_hints: Cell<bool>,
    /// A read binds members to locators; a Save only prefetches their groups.
    pub(super) bind_hints: Cell<bool>,
    /// Program pages bind members too: set only while a Save rebuilds and
    /// hashes its bases (`save/wave.rs`), never for a read.
    pub(super) bind_pages: Cell<bool>,
    /// Set while a Save reads through a reader (its lost registrations'
    /// winners): those demands bind nothing either.
    saving: Cell<bool>,
    /// Cleared by a failure a hint may have caused, until the next demand.
    pub(super) trust_hints: Cell<bool>,
}
impl Fetch {
    pub(crate) fn retain_inode_locators(&mut self) {
        if self.metadata.batched_chain_reads() {
            self.locators.get_mut().retain_inodes();
        }
    }

    /// Gives non-lost FULL files just admitted to the similarity ring one
    /// extra clock pass without retaining new entries or private locators.
    pub(crate) fn keep_captured_ring_winners(
        &self,
        batch: &crate::port::Publication,
        lost: &BTreeSet<ObjectId>,
    ) {
        let cache = self.locators.borrow();
        if !cache.keeps_ring_winners() || batch.signatures.is_empty() {
            return;
        }
        let indexed: BTreeSet<_> = batch.signatures.iter().map(|row| row.object_id).collect();
        for row in &batch.objects {
            if row.role == layerfs_content::ObjectRole::WholeFile
                && row.base.is_none()
                && indexed.contains(&row.object_id)
                && !lost.contains(&row.object_id)
            {
                cache.keep_ring_winner(*row);
            }
        }
    }

    pub(crate) fn new(metadata: Arc<dyn PackPersistence>) -> Self {
        Self {
            metadata,
            locators: RefCell::new(LocatorCache::default()),
            absent: RefCell::new(BTreeSet::new()),
            descriptors: RefCell::new(BTreeMap::new()),
            signatures: RefCell::new(None),
            published: RefCell::new(Published::default()),
            held: RefCell::new(None),
            serve_published: Cell::new(false),
            counters: Cell::new(Diagnostics::default()),
            hints: RefCell::default(),
            capacities: Cell::new(None),
            strict_hints: Cell::new(false),
            bind_hints: Cell::new(true),
            bind_pages: Cell::new(false),
            saving: Cell::new(false),
            trust_hints: Cell::new(true),
        }
    }
    pub(crate) fn note(&self, change: impl FnOnce(&mut Diagnostics)) {
        let mut counters = self.counters.get();
        change(&mut counters);
        self.counters.set(counters);
    }
    pub(crate) fn counters(&self) -> Diagnostics {
        self.counters.get()
    }
    pub(crate) fn invalidate(&self, ids: &[ObjectId]) {
        let mut cache = self.locators.borrow_mut();
        let mut absent = self.absent.borrow_mut();
        for id in ids {
            cache.remove(id);
            absent.remove(id);
        }
    }
    /// After an acknowledged publication: the locations this Save won are
    /// the Store's winners, so they are kept instead of located again; lost
    /// ones were invalidated by the caller and are located again. Its
    /// metadata pack bodies are kept for its own later reads. A remote
    /// captured Save may also keep payload packs with acknowledged winners.
    pub(crate) fn published(
        &self,
        batch: &crate::port::Publication,
        lost: &BTreeSet<ObjectId>,
        keep_payloads: bool,
    ) -> StorageResult<()> {
        let infos: BTreeMap<i64, PackInfo> = batch
            .packs
            .iter()
            .map(|pack| (pack.info().pack_id, pack.info()))
            .collect();
        let mut winner_packs = BTreeSet::new();
        for row in &batch.objects {
            let id = row.object_id;
            self.absent.borrow_mut().remove(&id);
            let Some(pack) = infos
                .get(&row.pack_id)
                .copied()
                .filter(|_| !lost.contains(&id))
            else {
                self.locators.borrow_mut().remove(&id);
                continue;
            };
            check_info(pack)?;
            self.remember_pack(pack)?;
            if keep_payloads && pack.domain == PackDomain::Payload {
                winner_packs.insert(pack.pack_id);
            }
            let location = *row;
            self.keep_locator(crate::location::LocatedObject { location, pack }, false);
        }
        self.published_directories(&batch.packs);
        let mut published = self.published.borrow_mut();
        for pack in &batch.packs {
            let info = pack.info();
            if info.domain == PackDomain::Metadata || winner_packs.contains(&info.pack_id) {
                published.insert(info, pack.body().clone());
            }
        }
        for row in &batch.inode_values {
            published.value(row.ordinal, row.value);
        }
        Ok(())
    }
    /// Forgets every published body and value, and a held publication: a
    /// Save on this handle did not finish.
    pub(crate) fn forget_published(&self) {
        *self.published.borrow_mut() = Published::default();
        self.release_held();
    }
    /// Serves the published bodies and values until the guard drops.
    pub(crate) fn serving_published(&self) -> Restore<'_> {
        Restore(&self.serve_published, self.serve_published.replace(true))
    }
    /// Holds every demand to a Save's rule (no binding) until the guard drops.
    pub(crate) fn save_reads(&self) -> Restore<'_> {
        Restore(&self.saving, self.saving.replace(true))
    }
    /// Binds chain members, for a Save's check of its bases, until the guard
    /// drops.
    pub(crate) fn binding(&self) -> Restore<'_> {
        Restore(&self.bind_hints, self.bind_hints.replace(true))
    }
    /// Lets program pages bind members too, for a Save's check of its
    /// program-page bases, until the guard drops.
    pub(crate) fn binding_pages(&self) -> Restore<'_> {
        Restore(&self.bind_pages, self.bind_pages.replace(true))
    }
    /// A body a Save of this handle published, to a Save or a reader that
    /// continues one, validated as a fetched one would be.
    /// A held body (`held.rs`) is served to every read.
    fn published_body(&self, id: i64) -> StorageResult<Option<(PackInfo, Vec<u8>)>> {
        let found = match self.held_body(id) {
            Some(held) => Some(held),
            None if self.serve_published.get() => self.published.borrow().get(id).cloned(),
            None => None,
        };
        let Some((info, body)) = found else {
            return Ok(None);
        };
        validate_frame(info, &body)?;
        self.remember_pack(info)?;
        self.note(|c| c.pack_hits += 1);
        Ok(Some((info, body.as_ref().clone())))
    }
    pub(crate) fn invalidate_signatures(&self) {
        *self.signatures.borrow_mut() = None;
    }
    /// A read demand binds chain members to locators; a Save's (`bind`
    /// false), and any demand while [`Self::save_reads`] holds, uses chains
    /// only to prefetch groups, so its selection walks see authoritative
    /// locators only. A hinted locator serves only the demand that bound it.
    pub(crate) fn begin_demand(&self, bind: bool) {
        self.absent.borrow_mut().clear();
        self.trust_hints.set(true);
        self.bind_hints.set(bind && !self.saving.get());
        self.locators.borrow_mut().remove_hinted();
        self.forget_member_packs();
    }
    /// Keeps one checked locator within the cache bound.
    pub(super) fn keep_locator(&self, row: crate::location::LocatedObject, hinted: bool) {
        let mut cache = self.locators.borrow_mut();
        if cache.len() >= READ_OBJECT_LIMIT {
            let work = cache.make_room(1, &[]);
            self.note(|c| {
                c.locator_evictions += work.removed;
                c.locator_eviction_probes += work.probes;
                c.locator_second_chances += work.second_chances;
            });
        }
        self.insert_locator(&mut cache, row, hinted);
    }
    /// Inserts one checked locator, clearing a cache with no room left.
    fn insert_locator(
        &self,
        cache: &mut LocatorCache,
        row: crate::location::LocatedObject,
        hinted: bool,
    ) {
        if cache.len() >= READ_OBJECT_LIMIT {
            self.note(|c| c.locator_evictions += cache.len() as u64);
            cache.clear();
        }
        cache.insert_as(row.location.object_id, row, hinted);
    }
    /// Whether [`Self::published_body`] would serve pack `id`.
    pub(super) fn published_has(&self, id: i64) -> bool {
        self.held_body(id).is_some()
            || self.serve_published.get() && self.published.borrow().get(id).is_some()
    }
    pub(crate) fn locate(&self, ids: &[ObjectId]) -> StorageResult<()> {
        self.locate_inner(ids, None)
    }
    pub(crate) fn locate_for_save(
        &self,
        ids: &[ObjectId],
        packs: &mut crate::encoding::PackCache,
    ) -> StorageResult<()> {
        let packs = (ids.len() > 1 && self.metadata.batched_chain_reads()).then_some(packs);
        self.locate_inner(ids, packs)
    }
    fn locate_inner(
        &self,
        ids: &[ObjectId],
        mut packs: Option<&mut crate::encoding::PackCache>,
    ) -> StorageResult<()> {
        let mut missing = BTreeSet::new();
        for id in ids {
            if self.locators.borrow().contains_key(id) || self.absent.borrow().contains(id) {
                self.note(|c| c.locator_hits += 1);
            } else {
                self.note(|c| c.locator_misses += 1);
                missing.insert(*id);
            }
        }
        // Held locations answer as the Store will once they are published.
        let held = self.held_locations(&mut missing);
        let missing: Vec<_> = missing.into_iter().collect();
        // The whole demand is protected from eviction, searched sorted.
        let mut protected = Vec::new();
        if !missing.is_empty() || !held.is_empty() {
            protected = ids.to_vec();
            protected.sort_unstable();
        }
        for page in held.chunks(READ_OBJECT_LIMIT) {
            {
                let mut cache = self.locators.borrow_mut();
                if cache.len().saturating_add(page.len()) > READ_OBJECT_LIMIT {
                    let work = cache.make_room(page.len(), &protected);
                    self.note(|c| {
                        c.locator_evictions += work.removed;
                        c.locator_eviction_probes += work.probes;
                        c.locator_second_chances += work.second_chances;
                    });
                }
            }
            for row in page {
                self.remember_pack(row.pack)?;
                let mut cache = self.locators.borrow_mut();
                self.insert_locator(&mut cache, *row, false);
            }
        }
        for page in missing.chunks(READ_OBJECT_LIMIT) {
            let mut rows = Vec::new();
            let mut chains = crate::location::LocatedChains::default();
            self.note(|c| c.locate += 1);
            let mut bodies = Vec::new();
            if let Some(packs) = packs.as_deref().filter(|_| page.len() > 1) {
                let published = self.published.borrow();
                let resident: Vec<_> = packs
                    .resident_ids()
                    .chain(published.bodies.iter().map(|(info, _)| info.pack_id))
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect();
                self.metadata.locate_chains_with_packs(
                    page,
                    &resident,
                    &mut rows,
                    &mut chains,
                    &mut bodies,
                )?;
                if bodies
                    .iter()
                    .any(|body| resident.binary_search(&body.info().pack_id).is_ok())
                {
                    return Err(StorageError::Integrity(
                        "joined read returned resident pack",
                    ));
                }
                self.note(|c| c.joined_pack_locates += 1);
            } else {
                self.metadata.locate_chains(page, &mut rows, &mut chains)?;
            }
            if rows.len() > page.len() {
                return Err(StorageError::Integrity("locator cardinality"));
            }
            {
                let mut cache = self.locators.borrow_mut();
                if cache.len().saturating_add(rows.len()) > READ_OBJECT_LIMIT {
                    let work = cache.make_room(rows.len(), &protected);
                    self.note(|c| {
                        c.locator_evictions += work.removed;
                        c.locator_eviction_probes += work.probes;
                        c.locator_second_chances += work.second_chances;
                    });
                }
            }
            // The whole answer, rows and chains, is checked before any of it
            // is kept.
            let mut seen = BTreeMap::new();
            for row in &rows {
                let location = row.location;
                if page.binary_search(&location.object_id).is_err()
                    || seen.insert(location.object_id, location.role).is_some()
                    || location.pack_id != row.pack.pack_id
                    || location.pack_id <= 0
                    || bad_position(
                        location.group_number,
                        location.record_number,
                        location.canonical_length,
                    )
                {
                    return Err(StorageError::Integrity("port locator"));
                }
                check_info(row.pack)?;
            }
            if !bodies.is_empty() {
                let packs = packs
                    .as_deref_mut()
                    .ok_or(StorageError::Integrity("unsolicited packs"))?;
                self.admit_joined_packs(packs, &rows, bodies)?;
            }
            self.install_chains(&seen, chains)?;
            for row in rows {
                self.remember_pack(row.pack)?;
                let mut cache = self.locators.borrow_mut();
                self.insert_locator(&mut cache, row, false);
                self.note(|c| {
                    c.locator_bookkeeping_live_peak_bytes = c
                        .locator_bookkeeping_live_peak_bytes
                        .max(cache.bookkeeping_bytes() as u64);
                });
            }
            let mut absent = self.absent.borrow_mut();
            for id in page.iter().filter(|id| !seen.contains_key(id)) {
                if absent.len() >= READ_OBJECT_LIMIT {
                    absent.clear();
                }
                absent.insert(*id);
            }
        }
        Ok(())
    }
    /// How many of `packs` (pack ids in physical order, repeated per object)
    /// form the next cohort: at least one, and while their distinct located
    /// lengths fit [`DEPENDENCY_PACK_CACHE_BYTES`] together. A pack whose
    /// length is not located counts as the whole bound.
    pub(crate) fn cohort(&self, packs: impl Iterator<Item = i64>) -> usize {
        let descriptors = self.descriptors.borrow();
        let (mut count, mut bytes, mut last) = (0, 0_usize, None);
        for id in packs {
            if last != Some(id) {
                let length = descriptors
                    .get(&id)
                    .map_or(DEPENDENCY_PACK_CACHE_BYTES, |info| info.length);
                if count > 0 && bytes.saturating_add(length) > DEPENDENCY_PACK_CACHE_BYTES {
                    break;
                }
                bytes = bytes.saturating_add(length);
                last = Some(id);
            }
            count += 1;
        }
        count
    }
    /// Keeps a descriptor a source gave, and confirms one only a chain member
    /// named (#758, `remember_member_pack`). Every caller confirms: locate
    /// rows, whole-pack replies (length and digest authenticated), but also
    /// a group read's ranged admission and a pack-cache hit, whose descriptor
    /// may itself be a member's, so a member's descriptor whose group was
    /// read counts as authoritative from then on. A descriptor that
    /// contradicts a still-marked one makes the hint stale (counted, strict
    /// refuses) and drops every hint at once, so a later failure of that
    /// walk at a hinted element is no longer rebuilt on a read (the failure
    /// stands) and is not counted again by a Save's check; one
    /// that contradicts an unmarked descriptor fails as
    /// `inconsistent pack descriptor`. Pack ids are never reused, so either
    /// contradiction means a provider answered a chain unlike its own pack
    /// row.
    pub(super) fn remember_pack(&self, info: PackInfo) -> StorageResult<()> {
        let from_member = self.confirm_member_pack(info.pack_id);
        let old = self.descriptors.borrow().get(&info.pack_id).copied();
        if old.is_some_and(|old| old != info) {
            if !from_member {
                return Err(StorageError::Integrity("inconsistent pack descriptor"));
            }
            self.descriptors.borrow_mut().remove(&info.pack_id);
            self.forget_hints();
            self.stale()?;
        }
        let mut cache = self.descriptors.borrow_mut();
        if cache.len() >= READ_OBJECT_LIMIT && !cache.contains_key(&info.pack_id) {
            cache.clear();
            self.forget_member_marks();
        }
        cache.insert(info.pack_id, info);
        Ok(())
    }
    fn metadata_packs(
        &self,
        ids: &[i64],
        packs: &mut crate::encoding::PackCache,
    ) -> StorageResult<()> {
        let mut fetched = Vec::with_capacity(ids.len());
        for id in ids {
            match self.published_body(*id)? {
                Some((info, body)) => packs.insert(info.pack_id, body)?,
                None => fetched.push(*id),
            }
        }
        let ids = fetched.as_slice();
        if ids.is_empty() {
            return Ok(());
        }
        let mut rows = Vec::new();
        self.note(|c| {
            c.read_packs += 1;
            c.pack_misses += ids.len() as u64;
        });
        self.metadata.read_packs(ids, &mut rows)?;
        let bytes: usize = rows.iter().map(|row| row.body().len()).sum();
        if rows.len() != ids.len()
            || bytes > DEPENDENCY_PACK_CACHE_BYTES
                && !(rows.len() == 1 && bytes <= SINGLETON_PACK_LIMIT)
        {
            return Err(StorageError::Integrity("metadata pack cardinality/bytes"));
        }
        let mut seen = BTreeSet::new();
        for row in rows {
            let (info, body) = row.into_parts();
            if !ids.contains(&info.pack_id) || !seen.insert(info.pack_id) {
                return Err(StorageError::Integrity("metadata pack descriptor"));
            }
            validate_frame(info, &body)?;
            self.remember_pack(info)?;
            self.note(|c| c.pack_read_bytes += body.len() as u64);
            packs.insert(info.pack_id, body)?;
        }
        Ok(())
    }
    /// Takes the uncached packs of one demand in one `read_packs` call when
    /// the provider takes packs whole ([`PackPersistence::whole_pack_bytes`]
    /// above the control prefix) and there are at least two of them; one
    /// pack goes through its own acquisition at the same cost. Only packs
    /// whose descriptors the demand located and that the provider would take
    /// whole are read, within the pack cache's byte bound, so the cache keeps
    /// every one; the others are acquired as before.
    pub(crate) fn whole_packs(
        &self,
        ids: &[i64],
        packs: &mut crate::encoding::PackCache,
    ) -> StorageResult<()> {
        let limit = self.metadata.whole_pack_bytes();
        if limit <= crate::port::PACK_READ_PREFIX_BYTES {
            return Ok(());
        }
        // The demand's whole packs, kept or wanted, within the cache bound;
        // reserving them first keeps a kept one from being evicted by the read.
        let mut cohort = Vec::new();
        let mut wanted = Vec::new();
        let mut bytes = 0usize;
        {
            let descriptors = self.descriptors.borrow();
            for id in ids.iter().copied().collect::<BTreeSet<_>>() {
                let Some(info) = descriptors.get(&id) else {
                    continue;
                };
                if info.length > limit.min(crate::policy::PACK_LIMIT)
                    || bytes + info.length > DEPENDENCY_PACK_CACHE_BYTES
                {
                    continue;
                }
                bytes += info.length;
                cohort.push((id, info.length));
                if !packs.contains_key(&id) {
                    wanted.push(id);
                }
            }
        }
        if wanted.len() < 2 {
            return Ok(());
        }
        packs.reserve(&cohort)?;
        self.metadata_packs(&wanted, packs)
    }
    fn payload(&self, info: PackInfo) -> StorageResult<Vec<u8>> {
        if let Some((held, body)) = self.held_body(info.pack_id) {
            if held != info {
                return Err(StorageError::Integrity("payload descriptor"));
            }
            validate_frame(info, &body)?;
            self.note(|c| c.pack_hits += 1);
            return Ok(body.as_ref().clone());
        }
        self.note(|c| {
            c.payload_reads += 1;
            c.pack_misses += 1;
        });
        let mut rows = Vec::new();
        self.metadata.read_packs(&[info.pack_id], &mut rows)?;
        let row = rows
            .pop()
            .filter(|row| rows.is_empty() && row.info() == info)
            .ok_or(StorageError::Integrity("payload descriptor"))?;
        let (_, body) = row.into_parts();
        self.note(|c| c.payload_read_bytes += body.len() as u64);
        validate_frame(info, &body)?;
        Ok(body)
    }
    /// A whole pack a reply returned: refused as `reason` unless it is the
    /// one asked for (`bound`), else its descriptor is kept and its frame
    /// checked, and the pack cache may admit it.
    pub(super) fn whole_reply(
        &self,
        bound: bool,
        reason: &'static str,
        info: PackInfo,
        body: Vec<u8>,
    ) -> StorageResult<crate::encoding::PackAcquisition> {
        if !bound {
            return Err(StorageError::Integrity(reason));
        }
        self.remember_pack(info)?;
        validate_frame(info, &body)?;
        Ok(crate::encoding::PackAcquisition::Whole {
            info: Some(info),
            body,
        })
    }
}
impl Source for Fetch {
    fn ordered_dependencies(&self) -> bool {
        false
    }
    fn location(&self, id: ObjectId, _ceiling: i64) -> StorageResult<Option<ObjectLocation>> {
        self.locate(&[id])?;
        Ok(self.locators.borrow().get(&id).map(|row| row.location))
    }
    fn base_location(&self, id: ObjectId, ceiling: i64) -> StorageResult<Option<ObjectLocation>> {
        // A negative mark may be older than the record naming this base: the
        // other writer published both after it was taken (F-036).
        self.absent.borrow_mut().remove(&id);
        self.location(id, ceiling)
    }
    fn pack_bytes(&self, id: i64) -> StorageResult<Vec<u8>> {
        let info = self.descriptors.borrow().get(&id).copied();
        if let Some(info) = info {
            if info.domain == PackDomain::Payload {
                return self.payload(info);
            }
        } else {
            return Err(StorageError::Integrity("pack descriptor missing"));
        }
        let mut packs = crate::encoding::PackCache::new();
        self.metadata_packs(&[id], &mut packs)?;
        packs
            .remove(&id)
            .ok_or(StorageError::Integrity("metadata pack missing"))
    }
    fn validate_cached_pack(&self, info: PackInfo) -> StorageResult<()> {
        self.remember_pack(info)
    }
    fn acquire_groups(
        &self,
        id: i64,
        groups: &[usize],
        whole_for_reuse: bool,
    ) -> StorageResult<crate::encoding::PackAcquisition> {
        if let Some((info, body)) = self.published_body(id)? {
            return Ok(crate::encoding::PackAcquisition::Whole {
                info: Some(info),
                body,
            });
        }
        self.note(|c| {
            c.read_pack_selections += 1;
            c.pack_misses += 1;
        });
        let mut plan = super::units::GroupPlan::new(
            id,
            groups,
            whole_for_reuse,
            self.metadata.whole_pack_bytes(),
        );
        let reply = self.metadata.read_scoped_pack(id, &mut plan);
        if let Some(error) = plan.error {
            return Err(error);
        }
        let reply = reply?;
        let info = match &reply {
            crate::port::AcquiredPackRead::Whole(row) => row.info(),
            crate::port::AcquiredPackRead::Units(row) => row.info(),
        };
        if plan.selected_info != Some(info) {
            return Err(StorageError::Integrity("selected descriptor binding"));
        }
        match (&plan.selected_choice, &reply) {
            (Some(crate::port::PackReadChoice::Whole), crate::port::AcquiredPackRead::Whole(_)) => {
            }
            (
                Some(crate::port::PackReadChoice::Ranges(wanted)),
                crate::port::AcquiredPackRead::Units(row),
            ) if wanted
                .iter()
                .copied()
                .eq(row.ranges().iter().map(|(range, _)| *range)) => {}
            _ => return Err(StorageError::Integrity("selected strategy/extent reply")),
        }
        match reply {
            crate::port::AcquiredPackRead::Whole(row) => {
                let (info, body) = row.into_parts();
                let length = body.len() as u64;
                let whole =
                    self.whole_reply(info.pack_id == id, "selected whole binding", info, body)?;
                self.note(|c| {
                    c.whole_selected += 1;
                    match plan.whole_reason {
                        Some("singleton") => c.whole_due_singleton += 1,
                        Some("density") => c.whole_due_density += 1,
                        Some("small") => c.whole_due_small += 1,
                        Some("reuse") => c.whole_due_reuse += 1,
                        _ => {}
                    }
                    if info.domain == PackDomain::Payload {
                        c.payload_reads += 1;
                        c.payload_read_bytes += length;
                    } else {
                        c.pack_read_bytes += length;
                    }
                });
                Ok(whole)
            }
            crate::port::AcquiredPackRead::Units(row) => {
                if row.info().pack_id != id {
                    return Err(StorageError::Integrity("selected pack binding"));
                }
                self.remember_pack(row.info())?;
                self.note(|c| {
                    c.range_selected += 1;
                    c.range_acquired_bytes += row.acquired_bytes() as u64;
                    c.range_materialized_bytes += row.prefix().len() as u64
                        + row
                            .ranges()
                            .iter()
                            .map(|(_, body)| body.len() as u64)
                            .sum::<u64>();
                });
                Ok(crate::encoding::PackAcquisition::Units(
                    crate::encoding::PackUnit::from_acquired(row)?,
                ))
            }
        }
    }
    fn inode_values(&self, ordinals: &[u32]) -> StorageResult<Vec<[u8; INODE_VALUE_BYTES]>> {
        if ordinals.contains(&0) {
            return Err(StorageError::Integrity("metadata ordinal"));
        }
        let wanted: Vec<u32> = ordinals
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut found = BTreeMap::new();
        // Held values first: the Store does not have them yet.
        let wanted: Vec<u32> = wanted
            .into_iter()
            .filter(|ordinal| match self.held_value(*ordinal) {
                Some(value) => {
                    found.insert(*ordinal, value);
                    false
                }
                None => true,
            })
            .collect();
        let wanted: Vec<u32> = if self.serve_published.get() {
            let published = self.published.borrow();
            wanted
                .into_iter()
                .filter(|ordinal| match published.values.get(ordinal) {
                    Some(value) => {
                        found.insert(*ordinal, *value);
                        false
                    }
                    None => true,
                })
                .collect()
        } else {
            wanted
        };
        let mut rows = Vec::new();
        for page in wanted.chunks(READ_OBJECT_LIMIT) {
            self.note(|c| c.value_groups += 1);
            self.metadata.inode_values(page, &mut rows)?;
            // The reply may omit an ordinal but never add, repeat or reorder one.
            let mut asked = page.iter();
            for row in &rows {
                if !asked.any(|ordinal| *ordinal == row.ordinal) {
                    return Err(StorageError::Integrity("unrequested inode value"));
                }
                found.insert(row.ordinal, row.value);
            }
            // A stored ordinal's value never changes: values a Save (or a
            // reader continuing one) read are kept beside the ones it
            // published, within the same bound, so the next Save of this
            // handle and a Commit's logical count read the base leaf's
            // values once (#768 stage 2).
            if self.serve_published.get() {
                let mut published = self.published.borrow_mut();
                for row in &rows {
                    published.value(row.ordinal, row.value);
                }
            }
        }
        ordinals
            .iter()
            .map(|ordinal| {
                found
                    .get(ordinal)
                    .copied()
                    .ok_or(StorageError::Integrity("metadata ordinal missing"))
            })
            .collect()
    }
    fn signatures(&self) -> StorageResult<Vec<SignatureRow>> {
        if let Some(rows) = self.signatures.borrow().as_ref() {
            return Ok(rows.clone());
        }
        let mut rows = Vec::new();
        self.note(|c| c.signatures += 1);
        self.metadata.signatures(&mut rows)?;
        if rows.len() > 8192 {
            return Err(StorageError::Integrity("content index over slot bound"));
        }
        *self.signatures.borrow_mut() = Some(rows.clone());
        Ok(rows)
    }
    fn write_signatures(&self, _rows: &[SignatureRow]) -> StorageResult<usize> {
        Err(StorageError::UnsupportedPolicy {
            field: "signature changes require atomic registration",
        })
    }
    fn note_pack_evictions(&self, entries: u64, bytes: u64) {
        self.note(|c| {
            c.pack_evictions += entries;
            c.pack_evicted_bytes += bytes;
        });
    }
    fn note_pack_directory_validation(&self, groups: usize, entries: usize) {
        self.note(|c| {
            c.directory_validations += 1;
            c.directory_unshared_walks += groups as u64;
            c.directory_entry_bounds += entries as u64;
        });
    }
    fn note_pack_cache_hit(&self) {
        self.note(|c| c.pack_hits += 1);
    }
    fn prefetch_chain(
        &self,
        location: &ObjectLocation,
        packs: &mut crate::encoding::PackCache,
    ) -> StorageResult<()> {
        self.prefetch_groups(std::slice::from_ref(location), packs)
    }
    fn locate_ahead(
        &self,
        child: &ObjectLocation,
        base: ObjectId,
        packs: &crate::encoding::PackCache,
        workspace: &mut crate::encoding::codec::DecompressionWorkspace,
    ) -> StorageResult<()> {
        Fetch::locate_ahead(self, child, base, packs, workspace)
    }
    fn bind_base(&self, child: &ObjectLocation, base: Option<ObjectId>) -> StorageResult<()> {
        self.bind(child, base)
    }
    fn hinted(&self, id: ObjectId) -> bool {
        self.locators.borrow().is_hinted(&id)
    }
    fn drop_hints(&self, chain: &[ObjectId]) -> StorageResult<bool> {
        Fetch::drop_hints(self, chain)
    }
}
/// Whether a located position is out of bounds: a row's and a chain
/// member's alike (a member's length must also be at least one).
pub(super) fn bad_position(group: usize, record: usize, canonical_length: usize) -> bool {
    canonical_length > crate::policy::CANONICAL_LIMIT
        || group >= crate::policy::GROUP_COUNT_LIMIT
        || record >= crate::policy::RECORD_COUNT_LIMIT
}
pub(super) fn check_info(info: PackInfo) -> StorageResult<()> {
    if info.pack_id <= 0 || !(32..=SINGLETON_PACK_LIMIT).contains(&info.length) {
        return Err(StorageError::Integrity("port pack descriptor"));
    }
    if info.domain == PackDomain::Metadata && info.length > crate::policy::PACK_LIMIT {
        return Err(StorageError::Integrity("metadata pack bound"));
    }
    Ok(())
}
pub(super) fn validate_frame(info: PackInfo, body: &[u8]) -> StorageResult<()> {
    check_info(info)?;
    let header = layout::parse_header(body)?;
    if layout::declared_length(body)? != body.len()
        || PackDomain::for_lane(header.lane) != info.domain
    {
        return Err(StorageError::Integrity("sealed pack length/domain"));
    }
    Ok(())
}
