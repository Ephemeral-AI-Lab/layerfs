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

/// Metadata pack bodies the latest Save published, newest last, so the
/// readers that continue its operation ([`crate::Storage::reader_after_save`]:
/// a Commit's logical count and binding hints) need not fetch them back.
/// Bounded by [`DEPENDENCY_PACK_CACHE_BYTES`]; the bodies are the published
/// `Arc`s themselves, and a new Save clears them. Any other reader still
/// acquires every body it needs.
#[derive(Default)]
struct Published {
    bodies: VecDeque<(PackInfo, Arc<Vec<u8>>)>,
    bytes: usize,
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
        if body.len() > DEPENDENCY_PACK_CACHE_BYTES {
            return;
        }
        while self.bytes + body.len() > DEPENDENCY_PACK_CACHE_BYTES {
            let Some((_, old)) = self.bodies.pop_front() else {
                break;
            };
            self.bytes -= old.len();
        }
        self.bytes += body.len();
        self.bodies.push_back((info, body));
    }
}

/// Restores the earlier serving state when a continuing reader's call ends.
pub(crate) struct ServePublished<'a>(&'a Cell<bool>, bool);
impl Drop for ServePublished<'_> {
    fn drop(&mut self) {
        self.0.set(self.1);
    }
}

pub(crate) struct Fetch {
    pub(crate) metadata: Arc<dyn PackPersistence>,
    locators: RefCell<LocatorCache>,
    absent: RefCell<BTreeSet<ObjectId>>,
    descriptors: RefCell<BTreeMap<i64, PackInfo>>,
    signatures: RefCell<Option<Vec<SignatureRow>>>,
    published: RefCell<Published>,
    /// Set while a reader that continues the latest Save reads.
    serve_published: Cell<bool>,
    counters: Cell<Diagnostics>,
}
impl Fetch {
    pub(crate) fn new(metadata: Arc<dyn PackPersistence>) -> Self {
        Self {
            metadata,
            locators: RefCell::new(LocatorCache::default()),
            absent: RefCell::new(BTreeSet::new()),
            descriptors: RefCell::new(BTreeMap::new()),
            signatures: RefCell::new(None),
            published: RefCell::new(Published::default()),
            serve_published: Cell::new(false),
            counters: Cell::new(Diagnostics::default()),
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
    /// metadata pack bodies are kept for its own later reads (bounded).
    pub(crate) fn published(
        &self,
        batch: &crate::port::Publication,
        lost: &BTreeSet<ObjectId>,
    ) -> StorageResult<()> {
        let infos: BTreeMap<i64, PackInfo> = batch
            .packs
            .iter()
            .map(|pack| (pack.info().pack_id, pack.info()))
            .collect();
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
            let mut cache = self.locators.borrow_mut();
            if cache.len() >= READ_OBJECT_LIMIT {
                let work = cache.make_room(1, &[]);
                self.note(|c| {
                    c.locator_evictions += work.removed;
                    c.locator_eviction_probes += work.probes;
                    c.locator_second_chances += work.second_chances;
                });
            }
            if cache.len() >= READ_OBJECT_LIMIT {
                self.note(|c| c.locator_evictions += cache.len() as u64);
                cache.clear();
            }
            cache.insert(
                id,
                crate::location::LocatedObject {
                    location: *row,
                    pack,
                },
            );
        }
        let mut published = self.published.borrow_mut();
        for pack in &batch.packs {
            if pack.info().domain == PackDomain::Metadata {
                published.insert(pack.info(), pack.body().clone());
            }
        }
        for row in &batch.inode_values {
            published.value(row.ordinal, row.value);
        }
        Ok(())
    }
    /// A new Save starts without an earlier Save's published bodies.
    pub(crate) fn forget_published(&self) {
        *self.published.borrow_mut() = Published::default();
    }
    /// Serves the latest Save's published bodies until the guard drops.
    pub(crate) fn serving_published(&self) -> ServePublished<'_> {
        ServePublished(&self.serve_published, self.serve_published.replace(true))
    }
    /// A body the latest Save published, to a reader that continues it,
    /// validated as a fetched one would be.
    fn published_body(&self, id: i64) -> StorageResult<Option<(PackInfo, Vec<u8>)>> {
        if !self.serve_published.get() {
            return Ok(None);
        }
        let found = self.published.borrow().get(id).cloned();
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
    pub(crate) fn begin_demand(&self) {
        self.absent.borrow_mut().clear();
    }
    pub(crate) fn locate(&self, ids: &[ObjectId]) -> StorageResult<()> {
        let mut missing = BTreeSet::new();
        for id in ids {
            if self.locators.borrow().contains_key(id) || self.absent.borrow().contains(id) {
                self.note(|c| c.locator_hits += 1);
            } else {
                self.note(|c| c.locator_misses += 1);
                missing.insert(*id);
            }
        }
        let missing: Vec<_> = missing.into_iter().collect();
        for page in missing.chunks(READ_OBJECT_LIMIT) {
            let mut rows = Vec::new();
            self.note(|c| c.locate += 1);
            self.metadata.locate(page, &mut rows)?;
            if rows.len() > page.len() {
                return Err(StorageError::Integrity("locator cardinality"));
            }
            {
                let mut cache = self.locators.borrow_mut();
                if cache.len().saturating_add(rows.len()) > READ_OBJECT_LIMIT {
                    let work = cache.make_room(rows.len(), ids);
                    self.note(|c| {
                        c.locator_evictions += work.removed;
                        c.locator_eviction_probes += work.probes;
                        c.locator_second_chances += work.second_chances;
                    });
                }
            }
            let mut seen = BTreeSet::new();
            for row in rows {
                let location = row.location;
                if page.binary_search(&location.object_id).is_err()
                    || !seen.insert(location.object_id)
                    || location.pack_id != row.pack.pack_id
                    || location.pack_id <= 0
                    || location.canonical_length > crate::policy::CANONICAL_LIMIT
                    || location.group_number >= crate::policy::GROUP_COUNT_LIMIT
                    || location.record_number >= crate::policy::RECORD_COUNT_LIMIT
                {
                    return Err(StorageError::Integrity("port locator"));
                }
                check_info(row.pack)?;
                self.remember_pack(row.pack)?;
                let mut cache = self.locators.borrow_mut();
                if cache.len() >= READ_OBJECT_LIMIT {
                    self.note(|c| c.locator_evictions += cache.len() as u64);
                    cache.clear();
                }
                cache.insert(location.object_id, row);
                self.note(|c| {
                    c.locator_bookkeeping_live_peak_bytes = c
                        .locator_bookkeeping_live_peak_bytes
                        .max(cache.bookkeeping_bytes() as u64);
                });
            }
            let mut absent = self.absent.borrow_mut();
            for id in page.iter().filter(|id| !seen.contains(id)) {
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
    fn remember_pack(&self, info: PackInfo) -> StorageResult<()> {
        let mut cache = self.descriptors.borrow_mut();
        if cache.get(&info.pack_id).is_some_and(|old| *old != info) {
            return Err(StorageError::Integrity("inconsistent pack descriptor"));
        }
        if cache.len() >= READ_OBJECT_LIMIT && !cache.contains_key(&info.pack_id) {
            cache.clear();
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
                if info.pack_id != id {
                    return Err(StorageError::Integrity("selected whole binding"));
                }
                self.remember_pack(info)?;
                validate_frame(info, &body)?;
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
                        c.payload_read_bytes += body.len() as u64;
                    } else {
                        c.pack_read_bytes += body.len() as u64;
                    }
                });
                Ok(crate::encoding::PackAcquisition::Whole {
                    info: Some(info),
                    body,
                })
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
}
fn check_info(info: PackInfo) -> StorageResult<()> {
    if info.pack_id <= 0 || !(32..=SINGLETON_PACK_LIMIT).contains(&info.length) {
        return Err(StorageError::Integrity("port pack descriptor"));
    }
    if info.domain == PackDomain::Metadata && info.length > crate::policy::PACK_LIMIT {
        return Err(StorageError::Integrity("metadata pack bound"));
    }
    Ok(())
}
fn validate_frame(info: PackInfo, body: &[u8]) -> StorageResult<()> {
    check_info(info)?;
    let header = layout::parse_header(body)?;
    if layout::declared_length(body)? != body.len()
        || PackDomain::for_lane(header.lane) != info.domain
    {
        return Err(StorageError::Integrity("sealed pack length/domain"));
    }
    Ok(())
}
