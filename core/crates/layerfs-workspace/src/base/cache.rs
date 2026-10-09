//! Bounded immutable-object cache. Eviction never changes authoritative state.
use layerfs_content::{ContentError, ContentResult, ObjectId};
use std::{collections::BTreeMap, sync::Mutex};

pub(crate) struct State {
    pub(crate) cache: Cache,
    /// Derived child-directory counts of base directories, by content root.
    pub(crate) counts: crate::base::links::DirectoryCounts,
    pub(crate) work: crate::ClientWork,
}

/// One bounded immutable cache allowance shared by operation-scoped providers.
///
/// This owner contains authenticated bytes and cumulative observations, not
/// authority or provider errors. Its caller must restrict every sharing client
/// to one declared authorization context; an object ID does not grant access.
pub struct CanonicalCache {
    pub(crate) state: Mutex<State>,
}
impl CanonicalCache {
    /// Selects one allowance, shared across all clients retaining this owner.
    /// Oversized objects bypass retention instead of refusing valid demand.
    pub fn new(bytes: usize) -> Self {
        Self {
            state: Mutex::new(State {
                cache: Cache::new(bytes),
                counts: Default::default(),
                work: crate::ClientWork::default(),
            }),
        }
    }
    /// Cumulative shared-owner counters and current logical cache charge.
    /// Charges and counters are not a measured allocation or residency bound.
    pub fn diagnostics(&self) -> ContentResult<crate::ClientWork> {
        let state = self
            .state
            .lock()
            .map_err(|_| ContentError::ProviderFailure {
                what: "base cache owner",
            })?;
        let mut work = state.work;
        work.charged_cache_bytes = state.cache.charged();
        work.cached_objects = state.cache.entries();
        work.directory_counts = state.counts.len();
        work.file_lengths = state.cache.lengths();
        Ok(work)
    }
}

/// What one remembered file length is charged against the allowance: its
/// value and the same bookkeeping allowance as an object.
const LENGTH_CHARGE: usize = 8 + 256;

pub(crate) struct Cache {
    objects: BTreeMap<ObjectId, (Vec<u8>, u64)>,
    /// Logical lengths of regular files by content root: the Store's own
    /// answer, a pure function of that immutable root. They are charged to
    /// the same allowance and evicted in the same order as the objects.
    lengths: BTreeMap<ObjectId, (u64, u64)>,
    /// Age, whether the entry is a length, and its identity.
    ages: BTreeMap<(u64, bool, ObjectId), ()>,
    capacity: usize,
    charged: usize,
    epoch: u64,
}
impl Cache {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            objects: BTreeMap::new(),
            lengths: BTreeMap::new(),
            ages: BTreeMap::new(),
            capacity,
            charged: 0,
            epoch: 0,
        }
    }
    /// Evicts the least recently used entries, of either kind, until
    /// `charge` fits. The caller has checked that it fits the allowance.
    fn make_room(&mut self, charge: usize) -> u64 {
        let mut evicted = 0_u64;
        while self.charged > self.capacity - charge {
            let Some(((_, length, id), ())) = self.ages.pop_first() else {
                break;
            };
            let freed = if length {
                self.lengths.remove(&id).map(|_| LENGTH_CHARGE)
            } else {
                self.objects.remove(&id).map(|(old, _)| old.len() + 256)
            };
            if let Some(freed) = freed {
                self.charged -= freed;
                evicted = evicted.saturating_add(1);
            }
        }
        evicted
    }
    /// The remembered length of one file content root.
    pub(crate) fn length(&mut self, id: ObjectId) -> Option<u64> {
        let (length, age) = self.lengths.get_mut(&id)?;
        self.epoch = self.epoch.saturating_add(1);
        self.ages.remove(&(*age, true, id));
        *age = self.epoch;
        self.ages.insert((*age, true, id), ());
        Some(*length)
    }
    /// Remembers one length the provider answered. An allowance too small
    /// for one entry remembers nothing; the provider is then asked each time.
    pub(crate) fn remember_length(&mut self, id: ObjectId, length: u64) -> u64 {
        if LENGTH_CHARGE > self.capacity || self.lengths.contains_key(&id) {
            return 0;
        }
        let evicted = self.make_room(LENGTH_CHARGE);
        self.epoch = self.epoch.saturating_add(1);
        self.charged += LENGTH_CHARGE;
        self.lengths.insert(id, (length, self.epoch));
        self.ages.insert((self.epoch, true, id), ());
        evicted
    }
    pub(crate) fn lengths(&self) -> usize {
        self.lengths.len()
    }
    pub(crate) fn get(&mut self, id: ObjectId, remaining: usize) -> ContentResult<Option<Vec<u8>>> {
        let Some((value, age)) = self.objects.get_mut(&id) else {
            return Ok(None);
        };
        if value.len() > remaining {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "base cached demand bytes",
                limit: remaining as u64,
                actual: value.len() as u64,
            });
        }
        // Cache policy saturation can lose a hit, never reject filesystem work.
        self.epoch = self.epoch.saturating_add(1);
        self.ages.remove(&(*age, false, id));
        *age = self.epoch;
        self.ages.insert((*age, false, id), ());
        Ok(Some(value.clone()))
    }
    /// Whether the object is cached and no longer than `limit`.
    pub(crate) fn within(&self, id: ObjectId, limit: usize) -> bool {
        self.objects
            .get(&id)
            .is_some_and(|(value, _)| value.len() <= limit)
    }
    pub(crate) fn insert(&mut self, id: ObjectId, value: &[u8]) -> u64 {
        // Charge canonical bytes and a conservative bookkeeping allowance.
        // This is logical accounting, not a measured allocator/RSS bound.
        let Some(charge) = value.len().checked_add(256) else {
            return 0;
        };
        if charge > self.capacity || self.objects.contains_key(&id) {
            return 0;
        }
        let evicted = self.make_room(charge);
        self.epoch = self.epoch.saturating_add(1);
        self.charged += charge;
        self.objects.insert(id, (value.to_vec(), self.epoch));
        self.ages.insert((self.epoch, false, id), ());
        evicted
    }
    pub(crate) fn charged(&self) -> usize {
        self.charged
    }
    pub(crate) fn entries(&self) -> usize {
        self.objects.len()
    }
}
