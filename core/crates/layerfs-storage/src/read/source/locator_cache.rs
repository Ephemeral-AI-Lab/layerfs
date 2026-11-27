//! Positive locator retention with bounded second-chance victim selection.
use crate::location::LocatedObject;
use layerfs_content::{ObjectId, ObjectRole};
use std::{cell::Cell, collections::BTreeMap, ops::Bound};

const SECOND_CHANCE: u8 = 1;
const RING_WINNER_CHANCES: u8 = 2;
/// Pooled inode reads can rediscover the same ancestors in several waves.
const INODE_CHANCES: u8 = 3;

struct Entry {
    row: LocatedObject,
    chances: Cell<u8>,
    /// Bound from a chain member (#758), not located authoritatively.
    hinted: bool,
}
#[derive(Default)]
pub(crate) struct LocatorCache {
    entries: BTreeMap<ObjectId, Entry>,
    hand: Option<ObjectId>,
    prefer_inodes: bool,
    /// Entries that are hinted, so a demand with none skips the scan.
    hinted: usize,
}
#[derive(Default)]
pub(crate) struct EvictionWork {
    pub(crate) removed: u64,
    pub(crate) probes: u64,
    pub(crate) second_chances: u64,
}
impl LocatorCache {
    pub(super) fn retain_inodes(&mut self) {
        self.prefer_inodes = true;
    }
    pub(super) fn keeps_ring_winners(&self) -> bool {
        self.prefer_inodes
    }
    /// One initial extra pass for a newly acknowledged ring base. Ordinary
    /// lookup credit can sustain but never renew this bonus.
    pub(super) fn keep_ring_winner(&self, location: crate::location::ObjectLocation) {
        if let Some(entry) = self.entries.get(&location.object_id) {
            if !entry.hinted && entry.row.location == location {
                entry.chances.set(RING_WINNER_CHANCES);
            }
        }
    }
    fn chances(&self, role: ObjectRole, hinted: bool) -> u8 {
        if self.prefer_inodes
            && !hinted
            && matches!(role, ObjectRole::InodeLeaf | ObjectRole::InodeBranch)
        {
            INODE_CHANCES
        } else {
            SECOND_CHANCE
        }
    }
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
    pub(crate) fn get(&self, id: &ObjectId) -> Option<&LocatedObject> {
        let entry = self.entries.get(id)?;
        let minimum = self.chances(entry.row.location.role, entry.hinted);
        entry.chances.set(entry.chances.get().max(minimum));
        Some(&entry.row)
    }
    pub(crate) fn contains_key(&self, id: &ObjectId) -> bool {
        self.get(id).is_some()
    }
    /// Presence only: unlike [`Self::contains_key`], sets no referenced bit,
    /// so the second-chance order is left as it was.
    pub(crate) fn holds(&self, id: &ObjectId) -> bool {
        self.entries.contains_key(id)
    }
    pub(crate) fn remove(&mut self, id: &ObjectId) {
        if let Some(entry) = self.entries.remove(id) {
            self.hinted -= usize::from(entry.hinted);
        }
    }
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.hand = None;
        self.hinted = 0;
    }
    pub(crate) fn insert_as(&mut self, id: ObjectId, row: LocatedObject, hinted: bool) {
        let chances = Cell::new(self.chances(row.location.role, hinted));
        let entry = Entry {
            row,
            chances,
            hinted,
        };
        if let Some(old) = self.entries.insert(id, entry) {
            self.hinted -= usize::from(old.hinted);
        }
        self.hinted += usize::from(hinted);
    }
    /// A locator located or published authoritatively, without touching recency.
    pub(crate) fn authoritative(&self, id: &ObjectId) -> Option<LocatedObject> {
        self.entries
            .get(id)
            .filter(|entry| !entry.hinted)
            .map(|entry| entry.row)
    }
    pub(crate) fn is_hinted(&self, id: &ObjectId) -> bool {
        self.entries.get(id).is_some_and(|entry| entry.hinted)
    }
    /// Removes every hinted locator; returns how many there were.
    pub(crate) fn remove_hinted(&mut self) -> usize {
        if self.hinted == 0 {
            return 0;
        }
        let before = self.entries.len();
        self.entries.retain(|_, entry| !entry.hinted);
        self.hinted = 0;
        before - self.entries.len()
    }
    /// Keeps every hinted locator as authoritative; returns how many.
    pub(crate) fn promote_hinted(&mut self) -> usize {
        for entry in self.entries.values_mut() {
            entry.hinted = false;
        }
        std::mem::take(&mut self.hinted)
    }
    /// Fixed/live field bytes beyond the original map and LocatedObject values.
    /// BTree node capacity and allocator overhead are not included in this count.
    pub(crate) fn bookkeeping_bytes(&self) -> usize {
        std::mem::size_of::<Self>() - std::mem::size_of::<BTreeMap<ObjectId, LocatedObject>>()
            + self.len() * (std::mem::size_of::<Entry>() - std::mem::size_of::<LocatedObject>())
    }
    /// Evicts until `needed` more entries fit. `protected` is sorted: each
    /// probe searches it, so a demand larger than the cache costs a probe a
    /// logarithmic search, not a scan of the whole demand.
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
        // Every unprotected entry can be evicted within this many revolutions.
        // If all entries are protected, the existing oversized-frontier guard acts.
        let chances = if self.prefer_inodes {
            INODE_CHANCES
        } else {
            SECOND_CHANCE
        };
        for _ in 0..self.len().saturating_mul(usize::from(chances) + 1) {
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
            if protected.binary_search(&id).is_ok() {
                continue;
            }
            let credit = &self.entries[&id].chances;
            let old = credit.get();
            if old != 0 {
                credit.set(old - 1);
                work.second_chances += 1;
                continue;
            }
            self.remove(&id);
            work.removed += 1;
            return true;
        }
        false
    }
}
