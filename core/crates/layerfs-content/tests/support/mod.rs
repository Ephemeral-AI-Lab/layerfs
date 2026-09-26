//! Shared external-test helpers for the `layerfs-content` public API.
//!
//! Nothing here is product code: it builds inputs, records what construction
//! emitted and serves those objects back through the public provider trait.

#![allow(dead_code)]

pub mod edits;
pub mod filesystem;

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
        let parts = object.into_parts();
        let (id, role, bytes) = (parts.id, parts.role, parts.canonical);
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

impl MemoryStore {
    /// Logical length of a stored file root, without an external policy.
    pub fn logical_len_probe(&self, root: ObjectId) -> u64 {
        let canonical = self.canonical(root).expect("root bytes");
        match layerfs_content::whole_file_payload(canonical).expect("framing") {
            Some(bytes) => bytes.len() as u64,
            None => {
                layerfs_content::file::mapping::decode_file_state(canonical)
                    .expect("file state")
                    .logical_len
            }
        }
    }

    /// Adds every object held by `other` to this store.
    pub fn absorb(&mut self, other: &MemoryStore) {
        for (id, role) in other.order() {
            let bytes = other.canonical(*id).expect("held object has bytes");
            let object = FinalizedObject::new(*role, bytes.to_vec()).expect("canonical object");
            self.accept(object).expect("absorb accepts");
        }
    }

    /// Copies every held object into a fresh store, preserving emission order.
    pub fn merged_clone(&self) -> MemoryStore {
        let mut target = MemoryStore::new();
        for (id, role) in self.order() {
            let bytes = self.canonical(*id).expect("held object has bytes");
            let object = FinalizedObject::new(*role, bytes.to_vec()).expect("canonical object");
            assert_eq!(object.id(), *id);
            target.accept(object).expect("copy accepts");
        }
        target
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
        ObjectRole::WholeFile | ObjectRole::Chunk | ObjectRole::InodeLeaf => Vec::new(),
        ObjectRole::ExtentLeaf | ObjectRole::ExtentBranch => decode_node(canonical)
            .map(|node| node.references())
            .unwrap_or_default(),
        ObjectRole::FileState => decode_file_state(canonical)
            .map(|state| vec![state.mapping_root])
            .unwrap_or_default(),
        ObjectRole::DirectoryLeaf | ObjectRole::Symlink => Vec::new(),
        ObjectRole::DirectoryBranch => {
            layerfs_content::filesystem::directory::codec::decode_directory_page(canonical)
                .map(|page| match page {
                    layerfs_content::filesystem::directory::codec::DirectoryPage::Leaf {
                        ..
                    } => Vec::new(),
                    layerfs_content::filesystem::directory::codec::DirectoryPage::Branch {
                        children,
                        ..
                    } => children.into_iter().map(|(_, id)| id).collect(),
                })
                .unwrap_or_default()
        }
        ObjectRole::InodeBranch => {
            layerfs_content::filesystem::inode::codec::decode_inode_page(canonical)
                .map(|page| match page {
                    layerfs_content::filesystem::inode::codec::InodePage::Leaf { .. } => Vec::new(),
                    layerfs_content::filesystem::inode::codec::InodePage::Branch {
                        children,
                        ..
                    } => children.into_iter().map(|(_, id)| id).collect(),
                })
                .unwrap_or_default()
        }
        ObjectRole::FilesystemRoot => {
            layerfs_content::filesystem::FilesystemRoot::decode(canonical)
                .map(|root| vec![root.inode_table()])
                .unwrap_or_default()
        }
        ObjectRole::AttributeLeaf | ObjectRole::AttributeBranch => Vec::new(),
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
    bytes: u64,
}

impl<R: Read> CountingSource<R> {
    /// Wraps `inner`.
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            requests: 0,
            largest: 0,
            bytes: 0,
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

    /// Bytes the source actually delivered.
    pub fn bytes_read(&self) -> u64 {
        self.bytes
    }
}

impl<R: Read> Read for CountingSource<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        self.requests += 1;
        self.largest = self.largest.max(output.len());
        let read = self.inner.read(output)?;
        self.bytes += read as u64;
        Ok(read)
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

/// Entry count of every mapping page reachable from `root`, root first.
///
/// Level-by-level walk over the real decoded nodes: a page that is not canonical
/// fails during decoding, so this is an independent structural check.
pub fn mapping_page_sizes(store: &MemoryStore, root: ObjectId) -> Vec<(u8, bool, usize)> {
    use layerfs_content::file::mapping::{decode_file_state, decode_node_with_context};
    let canonical = store.canonical(root).expect("root bytes");
    let state = match layerfs_content::whole_file_payload(canonical).expect("framing") {
        Some(_) => return Vec::new(),
        None => decode_file_state(canonical).expect("file state"),
    };
    let mut pages = Vec::new();
    let mut level = vec![state.mapping_root];
    let mut depth = state.tree_level;
    let mut is_root = true;
    while !level.is_empty() {
        let mut next = Vec::new();
        for id in &level {
            let bytes = store.canonical(*id).expect("node bytes");
            let node = decode_node_with_context(bytes, is_root).expect("canonical page");
            pages.push((depth, is_root, node.entry_count()));
            if let layerfs_content::file::mapping::ExtentNode::Branch { children, .. } = node {
                for child in children {
                    next.push(child.child_object_id);
                }
            }
        }
        is_root = false;
        depth = depth.saturating_sub(1);
        level = next;
    }
    pages
}

/// Number of extent slices in the mapping tree below `root`.
pub fn extent_count(store: &MemoryStore, root: ObjectId) -> u64 {
    use layerfs_content::file::mapping::{decode_node_with_context, ExtentNode};
    let canonical = store.canonical(root).expect("root bytes");
    if layerfs_content::whole_file_payload(canonical)
        .expect("framing")
        .is_some()
    {
        return 1;
    }
    let state = layerfs_content::file::mapping::decode_file_state(canonical).expect("state");
    let mut total = 0_u64;
    let mut stack = vec![(state.mapping_root, true, state.tree_level)];
    while let Some((id, root, level)) = stack.pop() {
        let bytes = store.canonical(id).expect("node bytes");
        match decode_node_with_context(bytes, root).expect("canonical page") {
            ExtentNode::Leaf { extents, .. } => total += extents.len() as u64,
            ExtentNode::Branch { children, .. } => {
                for child in children {
                    stack.push((child.child_object_id, false, level - 1));
                }
            }
        }
    }
    total
}

/// True when `target` is reachable from `root` through decoded references.
pub fn reachable(store: &MemoryStore, root: ObjectId, target: ObjectId) -> bool {
    use layerfs_content::file::mapping::{decode_file_state, decode_node_with_context, ExtentNode};
    if root == target {
        return true;
    }
    let canonical = store.canonical(root).expect("root bytes");
    if layerfs_content::whole_file_payload(canonical)
        .expect("framing")
        .is_some()
    {
        return false;
    }
    let state = decode_file_state(canonical).expect("state");
    let mut stack = vec![(state.mapping_root, true, state.tree_level)];
    let mut payload_seen = false;
    while let Some((id, is_root, level)) = stack.pop() {
        if id == target {
            return true;
        }
        let bytes = store.canonical(id).expect("node bytes");
        match decode_node_with_context(bytes, is_root).expect("page") {
            ExtentNode::Leaf { extents, .. } => {
                if extents
                    .iter()
                    .any(|extent| extent.payload_object_id() == target)
                {
                    payload_seen = true;
                }
            }
            ExtentNode::Branch { children, .. } => {
                for child in children {
                    stack.push((child.child_object_id, false, level - 1));
                }
            }
        }
    }
    payload_seen || state.mapping_root == target
}

/// Provider wrapper that counts grouped demands and objects handed out.
#[derive(Default)]
pub struct CountingProvider {
    /// Objects handed out, in demand order.
    pub ids: std::cell::RefCell<Vec<ObjectId>>,
    /// Batch calls observed.
    pub batches: std::cell::Cell<u64>,
}

impl CountingProvider {
    /// Empty counter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Objects handed out so far.
    pub fn objects(&self) -> u64 {
        self.ids.borrow().len() as u64
    }

    /// Batch calls observed so far.
    pub fn batch_calls(&self) -> u64 {
        self.batches.get()
    }

    /// Objects handed out so far that are not `excluded`.
    pub fn objects_excluding(&self, excluded: &[ObjectId]) -> u64 {
        self.ids
            .borrow()
            .iter()
            .filter(|id| !excluded.contains(id))
            .count() as u64
    }
}

/// Provider that counts every batch it serves from `store`.
pub struct Counted<'a> {
    /// Wrapped store.
    pub store: &'a MemoryStore,
    /// Counters updated by every demand.
    pub counts: &'a CountingProvider,
}

impl layerfs_content::AuthenticatedObjects for Counted<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.counts.batches.set(self.counts.batches.get() + 1);
        self.counts.ids.borrow_mut().extend_from_slice(ids);
        self.store.read_canonical_batch(ids)
    }
}
