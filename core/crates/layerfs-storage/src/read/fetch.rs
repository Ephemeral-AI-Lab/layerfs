//! Locator caching, batched metadata acquisition and complete-pack integrity.

use super::Diagnostics;
use crate::{
    error::{StorageError, StorageResult},
    location::{LocatedObject, ObjectLocation, PackDomain, PackInfo, SignatureRow, ValueGroupRow},
    pack::layout,
    policy::{DEPENDENCY_PACK_CACHE_BYTES, READ_OBJECT_LIMIT, SINGLETON_PACK_LIMIT},
    port::{PackPersistence, ValueGroupQuery, ValueGroups},
    source::Source,
};
use layerfs_content::ObjectId;
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub(crate) struct Fetch {
    pub(crate) metadata: Arc<dyn PackPersistence>,
    locators: RefCell<BTreeMap<ObjectId, LocatedObject>>,
    absent: RefCell<BTreeSet<ObjectId>>,
    descriptors: RefCell<BTreeMap<i64, PackInfo>>,
    groups: RefCell<BTreeMap<u32, ValueGroupRow>>,
    missing_ordinals: RefCell<BTreeSet<u32>>,
    window: Cell<Option<u32>>,
    signatures: RefCell<Option<Vec<SignatureRow>>>,
    counters: Cell<Diagnostics>,
}
impl Fetch {
    pub(crate) fn new(metadata: Arc<dyn PackPersistence>) -> Self {
        Self {
            metadata,
            locators: RefCell::new(BTreeMap::new()),
            absent: RefCell::new(BTreeSet::new()),
            descriptors: RefCell::new(BTreeMap::new()),
            groups: RefCell::new(BTreeMap::new()),
            missing_ordinals: RefCell::new(BTreeSet::new()),
            window: Cell::new(None),
            signatures: RefCell::new(None),
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
    pub(crate) fn refresh_window(&self) {
        self.window.set(None);
    }
    pub(crate) fn invalidate_signatures(&self) {
        *self.signatures.borrow_mut() = None;
    }
    pub(crate) fn begin_demand(&self) {
        self.absent.borrow_mut().clear();
        self.missing_ordinals.borrow_mut().clear();
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
                    // Keep hits needed by this demand before admitting its misses.
                    // The existing full-cache guard still bounds oversized frontiers.
                    cache.retain(|id, _| ids.contains(id));
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
                    cache.clear();
                }
                cache.insert(location.object_id, row);
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
    pub(crate) fn prefetch_values(&self, ordinals: &[u32]) -> StorageResult<()> {
        let wanted: Vec<_> = ordinals
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|ordinal| {
                self.covering(*ordinal).is_none()
                    && !self.missing_ordinals.borrow().contains(ordinal)
            })
            .collect();
        for page in wanted.chunks(READ_OBJECT_LIMIT) {
            let reply = self.catalogue(ValueGroupQuery::Ordinals(page))?;
            if reply.next.is_some() || reply.rows.len() > page.len() {
                return Err(StorageError::Integrity("catalogue set cardinality"));
            }
            for ordinal in page {
                if self.covering(*ordinal).is_none() {
                    let mut missing = self.missing_ordinals.borrow_mut();
                    if missing.len() >= READ_OBJECT_LIMIT {
                        missing.clear();
                    }
                    missing.insert(*ordinal);
                }
            }
            for row in &reply.rows {
                if !page.iter().any(|ordinal| covers(*row, *ordinal)) {
                    return Err(StorageError::Integrity("unrequested value group"));
                }
            }
        }
        Ok(())
    }
    fn covering(&self, ordinal: u32) -> Option<ValueGroupRow> {
        self.groups
            .borrow()
            .range(..=ordinal)
            .next_back()
            .map(|(_, row)| *row)
            .filter(|row| covers(*row, ordinal))
    }
    fn catalogue(&self, query: ValueGroupQuery<'_>) -> StorageResult<ValueGroups> {
        self.note(|c| c.value_groups += 1);
        let reply = self.metadata.value_groups(query)?;
        if reply.window_start == 0 || reply.rows.len() > READ_OBJECT_LIMIT {
            return Err(StorageError::Integrity("catalogue bound/window"));
        }
        let mut end = 0;
        for row in &reply.rows {
            if row.first_ordinal == 0
                || u64::from(row.first_ordinal) < end
                || row.count == 0
                || row.count > crate::policy::VALUES_PER_GROUP
                || row.pack_id <= 0
                || row.group_number >= crate::policy::GROUP_COUNT_LIMIT
            {
                return Err(StorageError::Integrity("port value group"));
            }
            end = u64::from(row.first_ordinal) + row.count as u64;
            if end > u64::from(u32::MAX) + 1 {
                return Err(StorageError::Integrity("ordinal range"));
            }
            let mut cache = self.groups.borrow_mut();
            if cache.len() >= READ_OBJECT_LIMIT {
                cache.clear();
            }
            if cache.get(&row.first_ordinal).is_some_and(|old| old != row) {
                return Err(StorageError::Integrity("inconsistent value group"));
            }
            cache.insert(row.first_ordinal, *row);
        }
        self.window.set(Some(reply.window_start));
        Ok(reply)
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
    fn pack_bytes(&self, id: i64) -> StorageResult<Vec<u8>> {
        let info = self.descriptors.borrow().get(&id).copied();
        if let Some(info) = info {
            if info.domain == PackDomain::Payload {
                return self.payload(info);
            }
        } else if !self.groups.borrow().values().any(|row| row.pack_id == id) {
            return Err(StorageError::Integrity("pack descriptor missing"));
        }
        let mut packs = crate::encoding::PackCache::new();
        self.metadata_packs(&[id], &mut packs)?;
        packs
            .remove(&id)
            .ok_or(StorageError::Integrity("metadata pack missing"))
    }
    fn prepare_value_groups(&self, ordinals: &[u32]) -> StorageResult<()> {
        self.prefetch_values(ordinals)
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
        self.note(|c| {
            c.read_pack_selections += 1;
            c.pack_misses += 1;
        });
        let mut plan = super::units::GroupPlan::new(id, groups, whole_for_reuse);
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
    fn value_group(&self, ordinal: u32) -> StorageResult<Option<ValueGroupRow>> {
        if ordinal == 0 {
            return Err(StorageError::Integrity("metadata ordinal"));
        }
        if let Some(row) = self.covering(ordinal) {
            return Ok(Some(row));
        }
        self.prefetch_values(&[ordinal])?;
        Ok(self.covering(ordinal))
    }
    fn value_groups(
        &self,
        from: Option<u32>,
        visit: &mut dyn FnMut(ValueGroupRow) -> StorageResult<()>,
    ) -> StorageResult<()> {
        let mut from = from.unwrap_or(1);
        loop {
            let page = self.catalogue(ValueGroupQuery::Page {
                from,
                limit: READ_OBJECT_LIMIT,
            })?;
            if page
                .rows
                .first()
                .is_some_and(|row| row.first_ordinal < from)
            {
                return Err(StorageError::Integrity("catalogue chronology"));
            }
            let last = page
                .rows
                .last()
                .map(|row| row.first_ordinal)
                .unwrap_or(from);
            for row in page.rows {
                visit(row)?;
            }
            match page.next {
                Some(next) if next > last => from = next,
                Some(_) => return Err(StorageError::Integrity("catalogue page progress")),
                None => return Ok(()),
            }
        }
    }
    fn window_start(&self) -> StorageResult<u32> {
        if let Some(start) = self.window.get() {
            return Ok(start);
        }
        Ok(self
            .catalogue(ValueGroupQuery::Page { from: 1, limit: 0 })?
            .window_start)
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
fn covers(row: ValueGroupRow, ordinal: u32) -> bool {
    ordinal >= row.first_ordinal
        && u64::from(ordinal) < u64::from(row.first_ordinal) + row.count as u64
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
