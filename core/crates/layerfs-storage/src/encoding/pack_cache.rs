//! Operation-owned immutable bodies with selective byte/count-bounded eviction.
use crate::{policy, StorageError, StorageResult};
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
};

#[derive(Debug)]
struct Entry {
    body: Vec<u8>,
    info: Option<crate::location::PackInfo>,
    unit: Option<(
        crate::pack::layout::PackHeader,
        crate::pack::layout::GroupView,
    )>,
    touched: Cell<u64>,
}
/// Actual body-cache work over one owning reader/save lifetime.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PackCacheWork {
    /// Entries selectively evicted for capacity.
    pub evictions: u64,
    /// Body bytes selectively evicted for capacity.
    pub evicted_bytes: u64,
    /// Highest retained body-byte count, including the isolated singleton exception.
    pub peak_bytes: usize,
}
/// One reader/save's owned bodies. Ordinary retention is at most 2 MiB and
/// READ_OBJECT_LIMIT entries. An oversized singleton is isolated and bounded by
/// SINGLETON_PACK_LIMIT. No body is shared with another operation or setup.
#[derive(Debug, Default)]
pub struct PackCache {
    entries: BTreeMap<(i64, Option<usize>), Entry>,
    retained: usize,
    clock: Cell<u64>,
    work: PackCacheWork,
}
impl PackCache {
    /// Empty operation-owned cache.
    pub fn new() -> Self {
        Self::default()
    }
    /// Presence only; a body consult through `get` updates recency.
    pub fn contains_key(&self, id: &i64) -> bool {
        self.entries.contains_key(&(*id, None))
    }
    /// Borrows an immutable retained body and records its recency.
    pub fn get(&self, id: &i64) -> Option<&Vec<u8>> {
        let entry = self.entries.get(&(*id, None))?;
        let next = self.clock.get().saturating_add(1);
        self.clock.set(next);
        entry.touched.set(next);
        Some(&entry.body)
    }
    /// Current owned body bytes, separately from metadata and decode buffers.
    pub fn retained_bytes(&self) -> usize {
        self.retained
    }
    /// Lifetime work counts; never a phase-local memory claim.
    pub fn work(&self) -> PackCacheWork {
        self.work
    }
    /// Releases bodies at an explicit private-write or owner boundary.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.retained = 0;
    }
    /// Transfers one body out, preserving exact retained-byte accounting.
    pub fn remove(&mut self, id: &i64) -> Option<Vec<u8>> {
        let entry = self.entries.remove(&(*id, None))?;
        self.retained -= entry.body.len();
        Some(entry.body)
    }
    /// Evicts unrelated bodies before acquiring a bounded physical cohort.
    /// Requested hits cannot be displaced by that cohort's misses. Duplicate
    /// identities or invalid declared widths are refused before acquisition.
    pub fn reserve(&mut self, wanted: &[(i64, usize)]) -> StorageResult<()> {
        let mut ids = BTreeSet::new();
        let mut missing = 0usize;
        for &(id, length) in wanted {
            if id <= 0 || length == 0 || length > policy::SINGLETON_PACK_LIMIT || !ids.insert(id) {
                return Err(StorageError::Integrity("pack cache demand"));
            }
            if let Some(body) = self.get(&id) {
                if body.len() != length {
                    return Err(StorageError::Integrity("cached pack length"));
                }
            } else {
                missing = missing
                    .checked_add(length)
                    .ok_or(StorageError::Integrity("pack cache bytes"))?;
            }
        }
        let total = wanted
            .iter()
            .try_fold(0usize, |sum, (_, length)| sum.checked_add(*length))
            .ok_or(StorageError::Integrity("pack cache bytes"))?;
        let limit = if wanted.len() == 1 && total > policy::DEPENDENCY_PACK_CACHE_BYTES {
            policy::SINGLETON_PACK_LIMIT
        } else {
            policy::DEPENDENCY_PACK_CACHE_BYTES
        };
        if total > limit || wanted.len() > policy::READ_OBJECT_LIMIT {
            return Err(StorageError::Integrity("pack cache cohort bound"));
        }
        // Oversized bodies never coexist with unrelated ordinary bodies.
        if total > policy::DEPENDENCY_PACK_CACHE_BYTES {
            while self.entries.keys().any(|(id, _)| !ids.contains(id)) {
                self.evict(&ids)?;
            }
        }
        while self.retained.saturating_add(missing) > limit
            || self.entries.len()
                + wanted
                    .iter()
                    .filter(|(id, _)| !self.contains_key(id))
                    .count()
                > policy::READ_OBJECT_LIMIT
        {
            self.evict(&ids)?;
        }
        Ok(())
    }
    /// Admits a real acquired immutable body, releasing only needed victims.
    pub fn insert(&mut self, id: i64, body: Vec<u8>) -> StorageResult<()> {
        if id <= 0 || body.is_empty() || body.len() > policy::SINGLETON_PACK_LIMIT {
            return Err(StorageError::Integrity("pack cache body"));
        }
        if let Some(prior) = self.get(&id) {
            if prior != &body {
                return Err(StorageError::Integrity("immutable cached pack changed"));
            }
            return Ok(());
        }
        let oversized = body.len() > policy::DEPENDENCY_PACK_CACHE_BYTES;
        while !self.entries.is_empty()
            && (oversized
                || self.retained.saturating_add(body.len()) > policy::DEPENDENCY_PACK_CACHE_BYTES
                || self.entries.len() >= policy::READ_OBJECT_LIMIT)
        {
            self.evict(&BTreeSet::new())?;
        }
        let next = self.clock.get().saturating_add(1);
        self.clock.set(next);
        self.retained += body.len();
        self.work.peak_bytes = self.work.peak_bytes.max(self.retained);
        self.entries.insert(
            (id, None),
            Entry {
                body,
                info: None,
                unit: None,
                touched: Cell::new(next),
            },
        );
        Ok(())
    }
    fn remove_entry(&mut self, key: (i64, Option<usize>)) -> Option<Vec<u8>> {
        let entry = self.entries.remove(&key)?;
        self.retained -= entry.body.len();
        Some(entry.body)
    }
    /// Borrows a complete encoded group from a whole body or an authenticated
    /// selected unit. Descriptor/eligibility checks precede every unit hit.
    pub fn group<'a>(
        &'a self,
        source: &dyn crate::source::Source,
        id: i64,
        number: usize,
    ) -> StorageResult<Option<super::GroupSlice<'a>>> {
        let entry = self
            .entries
            .get(&(id, None))
            .or_else(|| self.entries.get(&(id, Some(number))));
        let Some(entry) = entry else {
            return Ok(None);
        };
        if let Some(info) = entry.info {
            source.validate_cached_pack(info)?;
        }
        let next = self.clock.get().saturating_add(1);
        self.clock.set(next);
        entry.touched.set(next);
        if let Some((header, view)) = entry.unit {
            Ok(Some(super::GroupSlice {
                header,
                view,
                bytes: &entry.body,
            }))
        } else {
            let header = crate::pack::layout::parse_header(&entry.body)?;
            let view = crate::pack::layout::group_view(&entry.body, header, number)?;
            let bytes = entry
                .body
                .get(view.start..view.end)
                .ok_or(StorageError::Integrity("group body range"))?;
            Ok(Some(super::GroupSlice {
                header,
                view,
                bytes,
            }))
        }
    }
    /// Acquires one deduplicated physical pack's missing groups into the same
    /// byte/count allowance as whole bodies. Reports real acquired source bytes,
    /// including the complete authentication scan for selected results.
    pub fn acquire_groups(
        &mut self,
        source: &dyn crate::source::Source,
        id: i64,
        groups: &[usize],
    ) -> StorageResult<(bool, usize)> {
        if id <= 0 || groups.is_empty() || groups.len() > policy::READ_OBJECT_LIMIT {
            return Err(StorageError::Integrity("encoded group demand"));
        }
        if groups.len() == 1 && self.group(source, id, groups[0])?.is_some() {
            source.note_pack_cache_hit();
            return Ok((false, 0));
        }
        let wanted: BTreeSet<_> = groups.iter().copied().collect();
        let mut missing = Vec::new();
        for &group in &wanted {
            if self.group(source, id, group)?.is_some() {
                source.note_pack_cache_hit();
            } else {
                missing.push(group);
            }
        }
        if missing.is_empty() {
            return Ok((false, 0));
        }
        // A selected read already pays a complete digest scan. On a new group
        // miss for the same retained pack, pay that scan once more for its whole
        // body, then reuse it. No extra history/cache or error-driven retry.
        let whole_for_reuse = self
            .entries
            .range((id, Some(0))..=(id, Some(usize::MAX)))
            .any(|(_, entry)| {
                entry
                    .info
                    .is_some_and(|info| info.length <= policy::DEPENDENCY_PACK_CACHE_BYTES)
            });
        let acquired = source.acquire_groups(id, &missing, whole_for_reuse)?;
        if whole_for_reuse && !matches!(&acquired, super::PackAcquisition::Whole { .. }) {
            return Err(StorageError::Integrity("whole reuse selection reply"));
        }
        let before = self.work;
        let bytes = match acquired {
            super::PackAcquisition::Whole { info, body } => {
                if let Some(info) = info {
                    if info.pack_id != id || info.length != body.len() {
                        return Err(StorageError::Integrity("acquired pack binding"));
                    }
                    self.check_descriptor(info)?;
                }
                let header = crate::pack::layout::parse_header(&body)?;
                if info.is_some_and(|info| {
                    crate::location::PackDomain::for_lane(header.lane) != info.domain
                }) {
                    return Err(StorageError::Integrity("cached whole domain"));
                }
                for &group in &wanted {
                    crate::pack::layout::group_view(&body, header, group)?;
                }
                let bytes = body.len();
                let keys: Vec<_> = self
                    .entries
                    .keys()
                    .filter(|(pack, group)| *pack == id && group.is_some())
                    .copied()
                    .collect();
                for key in keys {
                    let entry = &self.entries[&key];
                    let (_, view) = entry
                        .unit
                        .ok_or(StorageError::Integrity("cached group view"))?;
                    if body.get(view.start..view.end) != Some(entry.body.as_slice()) {
                        return Err(StorageError::Integrity("immutable cached group changed"));
                    }
                    self.remove_entry(key);
                }
                self.reserve(&[(id, bytes)])?;
                self.insert(id, body)?;
                self.entries
                    .get_mut(&(id, None))
                    .ok_or(StorageError::Integrity("whole pack admission"))?
                    .info = info;
                bytes
            }
            super::PackAcquisition::Units(units) => {
                let info = units
                    .first()
                    .ok_or(StorageError::Integrity("empty selected reply"))?
                    .info;
                self.check_descriptor(info)?;
                let got: BTreeSet<_> = units.iter().map(|unit| unit.number).collect();
                if info.pack_id != id
                    || got.len() != units.len()
                    || got != missing.iter().copied().collect()
                    || units.iter().any(|unit| unit.info != info)
                {
                    return Err(StorageError::Integrity("selected group reply"));
                }
                for unit in units {
                    let length = unit.body.len();
                    if length > policy::DEPENDENCY_PACK_CACHE_BYTES {
                        return Err(StorageError::Integrity("selected cache bound"));
                    }
                    while !self.entries.is_empty()
                        && (self.retained.saturating_add(length)
                            > policy::DEPENDENCY_PACK_CACHE_BYTES
                            || self.entries.len() >= policy::READ_OBJECT_LIMIT)
                    {
                        self.evict(&BTreeSet::new())?;
                    }
                    let next = self.clock.get().saturating_add(1);
                    self.clock.set(next);
                    self.retained += length;
                    self.work.peak_bytes = self.work.peak_bytes.max(self.retained);
                    self.entries.insert(
                        (id, Some(unit.number)),
                        Entry {
                            body: unit.body,
                            info: Some(info),
                            unit: Some((unit.header, unit.view)),
                            touched: Cell::new(next),
                        },
                    );
                }
                info.length
            }
        };
        source.note_pack_evictions(
            self.work.evictions - before.evictions,
            self.work.evicted_bytes - before.evicted_bytes,
        );
        for group in wanted {
            if self.group(source, id, group)?.is_none() {
                return Err(StorageError::Integrity("cohort evicted required group"));
            }
        }
        Ok((true, bytes))
    }
    fn check_descriptor(&self, info: crate::location::PackInfo) -> StorageResult<()> {
        if self
            .entries
            .range((info.pack_id, None)..=(info.pack_id, Some(usize::MAX)))
            .any(|(_, entry)| entry.info.is_some_and(|old| old != info))
        {
            return Err(StorageError::Integrity(
                "immutable cached descriptor changed",
            ));
        }
        Ok(())
    }
    fn evict(&mut self, protected: &BTreeSet<i64>) -> StorageResult<()> {
        let id = self
            .entries
            .iter()
            .filter(|(id, _)| !protected.contains(&id.0))
            .min_by_key(|(id, entry)| (entry.touched.get(), **id))
            .map(|(id, _)| *id)
            .ok_or(StorageError::Integrity("pack cache protected capacity"))?;
        let body = self
            .remove_entry(id)
            .ok_or(StorageError::Integrity("pack cache victim"))?;
        self.work.evictions += 1;
        self.work.evicted_bytes += body.len() as u64;
        Ok(())
    }
}
