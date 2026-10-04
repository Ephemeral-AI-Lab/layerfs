//! Positive locator retention with bounded second-chance victim selection.
use crate::location::LocatedObject;
use layerfs_content::ObjectId;
use std::{cell::Cell, collections::BTreeMap, ops::Bound};

struct Entry {
    row: LocatedObject,
    referenced: Cell<bool>,
}
#[derive(Default)]
pub(crate) struct LocatorCache {
    entries: BTreeMap<ObjectId, Entry>,
    hand: Option<ObjectId>,
}
#[derive(Default)]
pub(crate) struct EvictionWork {
    pub(crate) removed: u64,
    pub(crate) probes: u64,
    pub(crate) second_chances: u64,
}
impl LocatorCache {
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
    pub(crate) fn get(&self, id: &ObjectId) -> Option<&LocatedObject> {
        let entry = self.entries.get(id)?;
        entry.referenced.set(true);
        Some(&entry.row)
    }
    pub(crate) fn contains_key(&self, id: &ObjectId) -> bool {
        self.get(id).is_some()
    }
    pub(crate) fn remove(&mut self, id: &ObjectId) {
        self.entries.remove(id);
    }
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.hand = None;
    }
    pub(crate) fn insert(&mut self, id: ObjectId, row: LocatedObject) {
        self.entries.insert(
            id,
            Entry {
                row,
                referenced: Cell::new(true),
            },
        );
    }
    /// Fixed/live field bytes beyond the original map and LocatedObject values.
    /// BTree node capacity and allocator overhead are not included in this count.
    pub(crate) fn bookkeeping_bytes(&self) -> usize {
        std::mem::size_of::<Self>() - std::mem::size_of::<BTreeMap<ObjectId, LocatedObject>>()
            + self.len() * (std::mem::size_of::<Entry>() - std::mem::size_of::<LocatedObject>())
    }
    pub(crate) fn make_room(&mut self, needed: usize, protected: &[ObjectId]) -> EvictionWork {
        let mut work = EvictionWork::default();
        while self.len().saturating_add(needed) > crate::policy::READ_OBJECT_LIMIT {
            if !self.evict_one(protected, &mut work) {
                break;
            }
        }
        work
    }
    fn evict_one(&mut self, protected: &[ObjectId], work: &mut EvictionWork) -> bool {
        // Each unprotected entry gets at most one second chance during this call.
        // If all entries are protected, the existing oversized-frontier guard acts.
        for _ in 0..self.len().saturating_mul(2) {
            let next = self
                .hand
                .and_then(|hand| {
                    self.entries
                        .range((Bound::Excluded(hand), Bound::Unbounded))
                        .next()
                        .map(|(id, _)| *id)
                })
                .or_else(|| self.entries.first_key_value().map(|(id, _)| *id));
            let Some(id) = next else {
                return false;
            };
            self.hand = Some(id);
            work.probes += 1;
            if protected.contains(&id) {
                continue;
            }
            if self.entries[&id].referenced.replace(false) {
                work.second_chances += 1;
                continue;
            }
            self.entries.remove(&id);
            work.removed += 1;
            return true;
        }
        false
    }
}
