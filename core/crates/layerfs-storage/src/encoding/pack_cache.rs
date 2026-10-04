//! Operation-owned immutable bodies with selective byte/count-bounded eviction.
use crate::{policy, StorageError, StorageResult};
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
};

#[derive(Debug)]
struct Entry {
    body: Vec<u8>,
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
    entries: BTreeMap<i64, Entry>,
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
        self.entries.contains_key(id)
    }
    /// Borrows an immutable retained body and records its recency.
    pub fn get(&self, id: &i64) -> Option<&Vec<u8>> {
        let entry = self.entries.get(id)?;
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
        let entry = self.entries.remove(id)?;
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
            while self.entries.keys().any(|id| !ids.contains(id)) {
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
            id,
            Entry {
                body,
                touched: Cell::new(next),
            },
        );
        Ok(())
    }
    fn evict(&mut self, protected: &BTreeSet<i64>) -> StorageResult<()> {
        let id = self
            .entries
            .iter()
            .filter(|(id, _)| !protected.contains(id))
            .min_by_key(|(id, entry)| (entry.touched.get(), **id))
            .map(|(id, _)| *id)
            .ok_or(StorageError::Integrity("pack cache protected capacity"))?;
        let body = self
            .remove(&id)
            .ok_or(StorageError::Integrity("pack cache victim"))?;
        self.work.evictions += 1;
        self.work.evicted_bytes += body.len() as u64;
        Ok(())
    }
}
