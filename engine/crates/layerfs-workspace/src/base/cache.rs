//! Bounded immutable-object cache. Eviction never changes authoritative state.
use layerfs_content::filesystem::attributes::PortableMetadata;
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_content::policy::WHOLE_FILE_CANONICAL_OVERHEAD;
use layerfs_content::{ContentError, ContentResult, FinalizedObject, ObjectId, ObjectRole};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Mutex,
};

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
    /// Decoded pages of objects this cache's clients authenticated, within
    /// their own fixed budget (#364).
    pub(crate) decoded: layerfs_content::filesystem::decoded::DecodedPages,
}
impl CanonicalCache {
    /// Selects one allowance, shared across all clients retaining this owner.
    /// Oversized objects bypass retention instead of refusing valid demand.
    /// Decoded pages get their own budget, never more than this allowance.
    pub fn new(bytes: usize) -> Self {
        let decoded = layerfs_content::filesystem::decoded::DECODED_PAGE_BYTES.min(bytes);
        Self::with_decoded(bytes, decoded)
    }
    /// [`Self::new`] with an explicit decoded-page budget; zero keeps none.
    pub fn with_decoded(bytes: usize, decoded: usize) -> Self {
        Self {
            state: Mutex::new(State {
                cache: Cache::new(bytes),
                counts: Default::default(),
                work: crate::ClientWork::default(),
            }),
            decoded: layerfs_content::filesystem::decoded::DecodedPages::new(decoded),
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
        work.portable_metadata = state.cache.portables();
        work.decoded = self.decoded.work();
        Ok(work)
    }
    /// Places what one Commit constructed in this cache, once that Commit's
    /// root is known published. Each entry is charged and evicted like any
    /// other and moved in without a copy or a second hash: its identity is
    /// the one its `FinalizedObject` computed from these bytes. An object
    /// already cached is left as it is. A seeded entry grants no authority.
    pub fn admit_constructed(&self, constructed: ConstructedObjects) -> ContentResult<()> {
        let ConstructedObjects {
            mut objects, order, ..
        } = constructed;
        let mut state = self
            .state
            .lock()
            .map_err(|_| ContentError::ProviderFailure {
                what: "base cache owner",
            })?;
        // Emission order: the root and the pages above the changed leaves
        // are placed last and are evicted last.
        for id in order {
            let evictions = match objects.remove(&id) {
                Some(Constructed::Object(value)) => {
                    let length = value.len() as u64;
                    let placed = state.cache.place(id, value.len(), || value);
                    if placed.is_some() {
                        state.work.seeded_objects = state.work.seeded_objects.saturating_add(1);
                        state.work.seeded_bytes = state.work.seeded_bytes.saturating_add(length);
                    }
                    placed
                }
                Some(Constructed::Length(length)) => {
                    let placed = state.cache.remember_length(id, length);
                    if placed.is_some() {
                        state.work.seeded_lengths = state.work.seeded_lengths.saturating_add(1);
                    }
                    placed
                }
                None => None,
            };
            state.work.evictions = state.work.evictions.saturating_add(evictions.unwrap_or(0));
        }
        Ok(())
    }
    /// An empty staging set bounded by a fixed share of this allowance.
    pub fn constructed(&self) -> ContentResult<ConstructedObjects> {
        let state = self
            .state
            .lock()
            .map_err(|_| ContentError::ProviderFailure {
                what: "base cache owner",
            })?;
        Ok(ConstructedObjects {
            objects: BTreeMap::new(),
            order: VecDeque::new(),
            charged: 0,
            limit: state.cache.capacity / CONSTRUCTED_SHARE,
        })
    }
}

/// One Commit may place at most this share of the allowance: a large Commit
/// never displaces the whole cache, and its staging set is bounded.
const CONSTRUCTED_SHARE: usize = 8;
/// What an object is charged beyond its bytes.
const OBJECT_CHARGE: usize = 256;
/// A chunk no longer than its own entry's bookkeeping charge is staged: an
/// attribute value's payload is such a chunk and cannot be told apart by role.
const CONSTRUCTED_CHUNK_BYTES: usize = OBJECT_CHARGE;

/// What is staged under one identity.
enum Constructed {
    /// The canonical bytes of a namespace object.
    Object(Vec<u8>),
    /// The logical length of a whole file, under its content root.
    Length(u64),
}
impl Constructed {
    fn charge(&self) -> usize {
        match self {
            Self::Object(value) => value.len() + OBJECT_CHARGE,
            Self::Length(_) => LENGTH_CHARGE,
        }
    }
}

/// Namespace objects one Commit constructed, held apart from every reader
/// until that Commit's root is published. Dropping it leaves no trace.
///
/// It keeps the objects that describe the tree (every role except a file's
/// payload) and the latest of them within its bound: construction emits
/// children before parents, so what remains of an oversized Commit is the
/// root and the pages nearest to it. Of a whole file it keeps the length
/// and not the payload.
pub struct ConstructedObjects {
    objects: BTreeMap<ObjectId, Constructed>,
    order: VecDeque<ObjectId>,
    charged: usize,
    limit: usize,
}
impl ConstructedObjects {
    /// Copies one constructed object's bytes if it is kept. The identity is
    /// the finalized object's own, computed once from the same allocation.
    /// A whole file's length is its canonical length less the format's fixed
    /// overhead: the Store's own answer for that role.
    pub fn offer(&mut self, object: &FinalizedObject) {
        let bytes = object.canonical();
        let length = match object.role() {
            ObjectRole::WholeFile => match bytes.len().checked_sub(WHOLE_FILE_CANONICAL_OVERHEAD) {
                Some(length) if length > 0 => Some(length as u64),
                _ => return,
            },
            ObjectRole::Chunk if bytes.len() > CONSTRUCTED_CHUNK_BYTES => return,
            _ => None,
        };
        let charge = match length {
            Some(_) => LENGTH_CHARGE,
            None => bytes.len().saturating_add(OBJECT_CHARGE),
        };
        if charge > self.limit || self.objects.contains_key(&object.id()) {
            return;
        }
        while self.charged > self.limit - charge {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(dropped) = self.objects.remove(&oldest) {
                self.charged -= dropped.charge();
            }
        }
        self.charged += charge;
        let staged = match length {
            Some(length) => Constructed::Length(length),
            None => Constructed::Object(bytes.to_vec()),
        };
        self.objects.insert(object.id(), staged);
        self.order.push_back(object.id());
    }
    /// Objects and lengths currently staged.
    pub fn len(&self) -> usize {
        self.objects.len()
    }
    /// Whether nothing is staged.
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }
}

/// What one remembered file length is charged against the allowance: its
/// value and the same bookkeeping allowance as an object.
const LENGTH_CHARGE: usize = 8 + 256;

/// What one remembered mode and mtime is charged: its sixteen bytes of
/// value and the same bookkeeping allowance as an object.
const PORTABLE_CHARGE: usize = 16 + 256;
/// What an age names: an object, a length, or the portable metadata of an
/// inode of one kind.
const OBJECT: u8 = 0;
const LENGTH: u8 = 1;
const fn portable_tag(kind: InodeKind) -> u8 {
    match kind {
        InodeKind::RegularFile => 2,
        InodeKind::Directory => 3,
        InodeKind::Symlink => 4,
    }
}

pub(crate) struct Cache {
    objects: BTreeMap<ObjectId, (Vec<u8>, u64)>,
    /// Logical lengths of regular files by content root: the Store's own
    /// answer, a pure function of that immutable root. They are charged to
    /// the same allowance and evicted in the same order as the objects.
    lengths: BTreeMap<ObjectId, (u64, u64)>,
    /// Checked portable mode and mtime by metadata root and inode kind: a
    /// pure function of that immutable root, decoded once from objects this
    /// cache's client authenticated. Charged and evicted like a length.
    portable: BTreeMap<(ObjectId, u8), (PortableMetadata, u64)>,
    /// Age, what the entry is, and its identity.
    ages: BTreeMap<(u64, u8, ObjectId), ()>,
    capacity: usize,
    charged: usize,
    epoch: u64,
}
impl Cache {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            objects: BTreeMap::new(),
            lengths: BTreeMap::new(),
            portable: BTreeMap::new(),
            ages: BTreeMap::new(),
            capacity,
            charged: 0,
            epoch: 0,
        }
    }
    /// Evicts the least recently used entries, of any kind, until `charge`
    /// fits. The caller has checked that it fits the allowance.
    fn make_room(&mut self, charge: usize) -> u64 {
        let mut evicted = 0_u64;
        while self.charged > self.capacity - charge {
            let Some(((_, tag, id), ())) = self.ages.pop_first() else {
                break;
            };
            let freed = match tag {
                OBJECT => self
                    .objects
                    .remove(&id)
                    .map(|(old, _)| old.len() + OBJECT_CHARGE),
                LENGTH => self.lengths.remove(&id).map(|_| LENGTH_CHARGE),
                _ => self.portable.remove(&(id, tag)).map(|_| PORTABLE_CHARGE),
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
        self.ages.remove(&(*age, LENGTH, id));
        *age = self.epoch;
        self.ages.insert((*age, LENGTH, id), ());
        Some(*length)
    }
    /// Remembers one length the provider answered. An allowance too small
    /// for one entry remembers nothing; the provider is then asked each time.
    pub(crate) fn remember_length(&mut self, id: ObjectId, length: u64) -> Option<u64> {
        if LENGTH_CHARGE > self.capacity || self.lengths.contains_key(&id) {
            return None;
        }
        let evicted = self.make_room(LENGTH_CHARGE);
        self.epoch = self.epoch.saturating_add(1);
        self.charged += LENGTH_CHARGE;
        self.lengths.insert(id, (length, self.epoch));
        self.ages.insert((self.epoch, LENGTH, id), ());
        Some(evicted)
    }
    pub(crate) fn lengths(&self) -> usize {
        self.lengths.len()
    }
    /// The remembered portable metadata of one metadata root and kind.
    pub(crate) fn portable(&mut self, root: ObjectId, kind: InodeKind) -> Option<PortableMetadata> {
        let tag = portable_tag(kind);
        let (metadata, age) = self.portable.get_mut(&(root, tag))?;
        self.epoch = self.epoch.saturating_add(1);
        self.ages.remove(&(*age, tag, root));
        *age = self.epoch;
        self.ages.insert((*age, tag, root), ());
        Some(*metadata)
    }
    /// Remembers one decoded, checked answer. An allowance too small for one
    /// entry remembers nothing; the objects are then read each time.
    pub(crate) fn remember_portable(
        &mut self,
        root: ObjectId,
        kind: InodeKind,
        metadata: PortableMetadata,
    ) -> u64 {
        let tag = portable_tag(kind);
        if PORTABLE_CHARGE > self.capacity || self.portable.contains_key(&(root, tag)) {
            return 0;
        }
        let evicted = self.make_room(PORTABLE_CHARGE);
        self.epoch = self.epoch.saturating_add(1);
        self.charged += PORTABLE_CHARGE;
        self.portable.insert((root, tag), (metadata, self.epoch));
        self.ages.insert((self.epoch, tag, root), ());
        evicted
    }
    pub(crate) fn portables(&self) -> usize {
        self.portable.len()
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
        self.ages.remove(&(*age, OBJECT, id));
        *age = self.epoch;
        self.ages.insert((*age, OBJECT, id), ());
        Ok(Some(value.clone()))
    }
    /// Whether the object is cached and no longer than `limit`.
    pub(crate) fn within(&self, id: ObjectId, limit: usize) -> bool {
        self.objects
            .get(&id)
            .is_some_and(|(value, _)| value.len() <= limit)
    }
    pub(crate) fn insert(&mut self, id: ObjectId, value: &[u8]) -> u64 {
        self.place(id, value.len(), || value.to_vec()).unwrap_or(0)
    }
    /// Retains one object of `length` bytes and returns what it evicted, or
    /// nothing when it is already cached or larger than the allowance.
    /// `value` is taken only when the object is retained.
    fn place(
        &mut self,
        id: ObjectId,
        length: usize,
        value: impl FnOnce() -> Vec<u8>,
    ) -> Option<u64> {
        // Charge canonical bytes and a conservative bookkeeping allowance.
        // This is logical accounting, not a measured allocator/RSS bound.
        let charge = length.checked_add(OBJECT_CHARGE)?;
        if charge > self.capacity || self.objects.contains_key(&id) {
            return None;
        }
        let evicted = self.make_room(charge);
        self.epoch = self.epoch.saturating_add(1);
        self.charged += charge;
        self.objects.insert(id, (value(), self.epoch));
        self.ages.insert((self.epoch, OBJECT, id), ());
        Some(evicted)
    }
    pub(crate) fn charged(&self) -> usize {
        self.charged
    }
    pub(crate) fn entries(&self) -> usize {
        self.objects.len()
    }
}
