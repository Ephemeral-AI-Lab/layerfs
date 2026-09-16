//! Shared external-test helpers for the `layerfs-content` public API.
//!
//! Nothing here is product code: it builds inputs, records what construction
//! emitted and serves those objects back through the public provider trait.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::io::{self, Read};

use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, FinalizedConsumer, FinalizedObject,
    ObjectId, ObjectRole,
};

/// In-memory consumer that keeps every emitted object by identity.
#[derive(Clone, Debug, Default)]
pub struct MemoryStore {
    objects: BTreeMap<ObjectId, (ObjectRole, Vec<u8>)>,
    order: Vec<(ObjectId, ObjectRole)>,
}

impl MemoryStore {
    /// Empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of stored objects.
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// True when nothing was emitted.
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// Emission order as `(id, role)` pairs, including repeats.
    pub fn order(&self) -> &[(ObjectId, ObjectRole)] {
        &self.order
    }

    /// Roles of every emitted object in emission order.
    pub fn roles(&self) -> Vec<ObjectRole> {
        self.order.iter().map(|(_, role)| *role).collect()
    }

    /// Canonical bytes of a stored object.
    pub fn canonical(&self, id: ObjectId) -> Option<&[u8]> {
        self.objects.get(&id).map(|(_, bytes)| bytes.as_slice())
    }

    /// Canonical bytes of the first stored object with `role`.
    pub fn canonical_by_role(&self, role: ObjectRole) -> Option<&[u8]> {
        self.objects
            .iter()
            .find(|(_, (stored, _))| *stored == role)
            .map(|(_, (_, bytes))| bytes.as_slice())
    }

    /// Overwrites a stored object, simulating an externally damaged Store entry.
    pub fn overwrite(&mut self, id: ObjectId, canonical: Vec<u8>) {
        if let Some(entry) = self.objects.get_mut(&id) {
            entry.1 = canonical;
        }
    }

    /// Removes a stored object, simulating a missing dependency.
    pub fn remove(&mut self, id: ObjectId) -> bool {
        self.objects.remove(&id).is_some()
    }

    /// Total canonical bytes held.
    pub fn canonical_bytes(&self) -> u64 {
        self.objects
            .values()
            .map(|(_, bytes)| bytes.len() as u64)
            .sum()
    }
}

impl FinalizedConsumer for MemoryStore {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        let (id, role, bytes, _) = object.into_parts();
        self.order.push((id, role));
        self.objects.insert(id, (role, bytes));
        Ok(())
    }
}

impl AuthenticatedObjects for MemoryStore {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        ids.iter()
            .map(|id| {
                let (_, bytes) = self.objects.get(id).ok_or(ContentError::MissingObject)?;
                if ObjectId::for_bytes(bytes) != *id {
                    return Err(ContentError::IdentityMismatch);
                }
                Ok(bytes.clone())
            })
            .collect()
    }
}

/// Builds a complete file with the frozen policy into a memory store.
pub fn build_file(bytes: &[u8]) -> (MemoryStore, layerfs_content::ConstructedFile) {
    let policy = layerfs_content::ConstructionPolicy::frozen_default();
    let mut store = MemoryStore::new();
    let constructed = disabled_scope(|scope| {
        layerfs_content::construct_bytes(
            policy,
            &policy.capacities(),
            bytes,
            &mut store,
            scope.child("content"),
        )
    })
    .expect("construction succeeds");
    (store, constructed)
}

/// Consumer that records emission order and checks dependency order afterwards.
#[derive(Default)]
pub struct TrackingConsumer {
    /// Objects emitted so far, exactly as the store holds them.
    pub store: MemoryStore,
    /// Positions of every emitted identity, in emission order.
    seen: Vec<ObjectId>,
}

impl TrackingConsumer {
    /// Empty consumer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Fails when any object was emitted before one of its direct references.
    pub fn assert_children_precede_parents(&self) {
        let mut position = std::collections::BTreeMap::new();
        for (index, (id, _)) in self.store.order().iter().enumerate() {
            position.entry(*id).or_insert(index);
        }
        for (id, role) in self.store.order() {
            let Some(canonical) = self.store.canonical(*id) else {
                continue;
            };
            let references = references_of(canonical, *role);
            let parent = position[id];
            for reference in references {
                if let Some(child) = position.get(&reference) {
                    assert!(
                        *child < parent,
                        "{role:?} {id} was emitted before its child {reference}"
                    );
                }
            }
        }
    }
}

impl FinalizedConsumer for TrackingConsumer {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        let (id, _, _, _) = (object.id(), object.role(), object.canonical(), ());
        self.seen.push(id);
        self.store.accept(object)
    }
}

/// Direct logical references of a canonical object, derived from its bytes.
pub fn references_of(canonical: &[u8], role: ObjectRole) -> Vec<ObjectId> {
    use layerfs_content::file::mapping::{decode_file_state, decode_node};
    match role {
        ObjectRole::WholeFile | ObjectRole::Chunk => Vec::new(),
        ObjectRole::ExtentLeaf | ObjectRole::ExtentBranch => decode_node(canonical)
            .map(|node| node.references())
            .unwrap_or_default(),
        ObjectRole::FileState => decode_file_state(canonical)
            .map(|state| vec![state.mapping_root])
            .unwrap_or_default(),
    }
}

/// Provider that tracks the peak number of distinct payloads handed out per batch.
pub struct CountingStore<'a> {
    inner: &'a MemoryStore,
    peak: std::cell::Cell<usize>,
}

impl<'a> CountingStore<'a> {
    /// Wraps `inner`.
    pub fn new(inner: &'a MemoryStore) -> Self {
        Self {
            inner,
            peak: std::cell::Cell::new(0),
        }
    }

    /// Largest batch this provider was asked for.
    pub fn peak_live_payloads(&self) -> usize {
        self.peak.get()
    }
}

impl AuthenticatedObjects for CountingStore<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.peak.set(self.peak.get().max(ids.len()));
        self.inner.read_canonical_batch(ids)
    }
}

/// Provider that fails a batch the moment it is asked for `poison`.
pub struct PoisonedStore<'a> {
    inner: &'a MemoryStore,
    poison: ObjectId,
}

impl<'a> PoisonedStore<'a> {
    /// Wraps `inner` so that any batch containing `poison` fails.
    pub fn new(inner: &'a MemoryStore, poison: ObjectId) -> Self {
        Self { inner, poison }
    }
}

impl AuthenticatedObjects for PoisonedStore<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        if ids.contains(&self.poison) {
            return Err(ContentError::MissingObject);
        }
        self.inner.read_canonical_batch(ids)
    }
}

/// Records how many byte requests a source received and their sizes.
pub struct CountingSource<R> {
    inner: R,
    requests: u64,
    largest: usize,
}

impl<R: Read> CountingSource<R> {
    /// Wraps `inner`.
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            requests: 0,
            largest: 0,
        }
    }

    /// Requests observed so far.
    pub fn requests(&self) -> u64 {
        self.requests
    }

    /// Largest requested read size.
    pub fn largest_request(&self) -> usize {
        self.largest
    }
}

impl<R: Read> Read for CountingSource<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.requests += 1;
        self.largest = self.largest.max(output.len());
        self.inner.read(output)
    }
}

/// Reader that yields `limit` bytes and then fails, exercising a late input error.
pub struct FailingAfter {
    remaining: Vec<u8>,
    position: usize,
    fail_at: usize,
}

impl FailingAfter {
    /// Serves `bytes` but reports an I/O failure once `fail_at` bytes were served.
    pub fn new(bytes: Vec<u8>, fail_at: usize) -> Self {
        Self {
            remaining: bytes,
            position: 0,
            fail_at,
        }
    }
}

impl Read for FailingAfter {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.position >= self.fail_at {
            return Err(io::Error::other("late input failure"));
        }
        let available = (self.fail_at - self.position).min(output.len());
        let end = (self.position + available).min(self.remaining.len());
        let count = end - self.position;
        output[..count].copy_from_slice(&self.remaining[self.position..end]);
        self.position += count;
        Ok(count)
    }
}

/// Deterministic incompressible input: a fixed xorshift stream, not file data.
pub fn noise(len: usize) -> Vec<u8> {
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    (0..len)
        .map(|_| {
            state ^= state.wrapping_shl(7);
            state ^= state.wrapping_shr(9);
            state ^= state.wrapping_shl(8);
            state as u8
        })
        .collect()
}

/// Deterministic highly compressible input.
pub fn repeat(len: usize, byte: u8) -> Vec<u8> {
    vec![byte; len]
}

/// Deterministic structured input whose chunk boundaries were frozen upstream.
pub fn patterned(len: usize) -> Vec<u8> {
    (0..len)
        .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
        .collect()
}

/// Alias kept small: the recording store is the same memory store.
pub type RecordingStore = MemoryStore;

/// A root scope that records nothing, for callers that do not want timings.
pub fn disabled_scope<T, E>(
    body: impl FnOnce(
        &layerfs_telemetry::timer::TimingScope<'_, layerfs_telemetry::timer::Active>,
    ) -> Result<T, E>,
) -> Result<T, E> {
    layerfs_telemetry::timer::Timing::disabled("test.operation", body).0
}

/// Reassembles a file through the public read API.
pub fn read_back(store: &MemoryStore, root: ObjectId) -> ContentResult<Vec<u8>> {
    let mut out = Vec::new();
    disabled_scope(|scope| {
        layerfs_content::read_all(store, root, &mut out, scope.child("test.read"))?;
        Ok(out)
    })
}
