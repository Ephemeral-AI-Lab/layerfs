//! Bounded immutable-object cache. Eviction never changes authoritative state.
use layerfs_content::{ContentError, ContentResult, ObjectId};
use std::{collections::BTreeMap, sync::Mutex};

pub(crate) struct State {
    pub(crate) cache: Cache,
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
        Ok(work)
    }
}

pub(crate) struct Cache {
    objects: BTreeMap<ObjectId, (Vec<u8>, u64)>,
    ages: BTreeMap<(u64, ObjectId), ()>,
    capacity: usize,
    charged: usize,
    epoch: u64,
}
impl Cache {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            objects: BTreeMap::new(),
            ages: BTreeMap::new(),
            capacity,
            charged: 0,
            epoch: 0,
        }
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
        self.ages.remove(&(*age, id));
        *age = self.epoch;
        self.ages.insert((*age, id), ());
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
        let mut evicted = 0_u64;
        while self.charged > self.capacity - charge {
            let Some(((age, id), ())) = self.ages.pop_first() else {
                break;
            };
            if let Some((old, _)) = self.objects.remove(&id) {
                self.charged -= old.len() + 256;
                evicted = evicted.saturating_add(1);
            }
            let _ = age;
        }
        self.epoch = self.epoch.saturating_add(1);
        self.charged += charge;
        self.objects.insert(id, (value.to_vec(), self.epoch));
        self.ages.insert((self.epoch, id), ());
        evicted
    }
    pub(crate) fn charged(&self) -> usize {
        self.charged
    }
    pub(crate) fn entries(&self) -> usize {
        self.objects.len()
    }
}
