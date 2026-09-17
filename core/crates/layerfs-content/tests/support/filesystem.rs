//! Shared external helpers for the filesystem-tree test targets.
//!
//! Nothing here is product code: it builds native inputs, collects what the
//! operation emitted and walks the result back through the public API.

#![allow(dead_code)]

use std::collections::BTreeMap;

use layerfs_content::filesystem::references::backing::OrderingBacking;
use layerfs_content::filesystem::{FilesystemObjects, FilesystemResources, PathName};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{
    AuthenticatedObjects, ContentError, ContentResult, FinalizedConsumer, FinalizedObject,
    ObjectId, ObjectRole,
};

/// In-memory provider and consumer for one filesystem operation.
#[derive(Clone, Debug, Default)]
pub struct TreeStore {
    objects: BTreeMap<ObjectId, Vec<u8>>,
    roles: BTreeMap<ObjectId, ObjectRole>,
    order: Vec<(ObjectId, ObjectRole)>,
}

impl TreeStore {
    /// Empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds canonical bytes as an already stored object.
    pub fn insert(&mut self, role: ObjectRole, canonical: Vec<u8>) -> ObjectId {
        let id = ObjectId::for_bytes(&canonical);
        self.objects.insert(id, canonical);
        self.roles.insert(id, role);
        id
    }

    /// Replaces the bytes stored under an existing identity.
    ///
    /// The identity no longer matches the bytes, which is exactly the "the
    /// provider served an object that is not the one demanded" case: a provider
    /// that authenticates its reads must reject it.
    pub fn overwrite(&mut self, id: ObjectId, canonical: Vec<u8>) {
        self.objects.insert(id, canonical);
    }

    /// Number of distinct objects.
    pub fn len(&self) -> usize {
        self.objects.len()
    }

    /// True when nothing is stored.
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    /// Canonical bytes of one object.
    pub fn canonical(&self, id: ObjectId) -> Option<&[u8]> {
        self.objects.get(&id).map(Vec::as_slice)
    }

    /// Role recorded for one object.
    pub fn role(&self, id: ObjectId) -> Option<ObjectRole> {
        self.roles.get(&id).copied()
    }

    /// Emission order.
    pub fn order(&self) -> &[(ObjectId, ObjectRole)] {
        &self.order
    }

    /// Emitted bytes held by the ids in `ids`.
    pub fn canonical_bytes(&self) -> u64 {
        self.objects.values().map(|bytes| bytes.len() as u64).sum()
    }

    /// Merges every object of `other` into this store, keeping emission order.
    pub fn absorb(&mut self, other: &TreeStore) {
        for (id, bytes) in &other.objects {
            self.objects.insert(*id, bytes.clone());
        }
        for (id, role) in &other.roles {
            self.roles.insert(*id, *role);
        }
        self.order.extend(other.order.iter().copied());
    }
}

impl FinalizedConsumer for TreeStore {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        let (id, role, bytes, _) = object.into_parts();
        self.roles.insert(id, role);
        self.objects.insert(id, bytes);
        self.order.push((id, role));
        Ok(())
    }
}

impl AuthenticatedObjects for TreeStore {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        ids.iter()
            .map(|id| {
                let bytes = self.objects.get(id).ok_or(ContentError::MissingObject)?;
                if ObjectId::for_bytes(bytes) != *id {
                    return Err(ContentError::IdentityMismatch);
                }
                Ok(bytes.clone())
            })
            .collect()
    }
}

/// One provider that fails after a declared number of successful reads.
pub struct FailingReader<'a> {
    inner: &'a TreeStore,
    remaining: std::cell::Cell<u64>,
}

impl<'a> FailingReader<'a> {
    /// Fails every read once `successes` reads have happened.
    pub fn new(inner: &'a TreeStore, successes: u64) -> Self {
        Self {
            inner,
            remaining: std::cell::Cell::new(successes),
        }
    }
}

impl AuthenticatedObjects for FailingReader<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        let remaining = self.remaining.get();
        if remaining == 0 {
            return Err(ContentError::MissingObject);
        }
        self.remaining.set(remaining.saturating_sub(1));
        self.inner.read_canonical_batch(ids)
    }
}

/// A directory update under construction.
pub struct DirectoryChange {
    /// Parent serial.
    pub parent: u64,
    /// Final bindings, appended in the order the test supplies.
    pub changes: Vec<(PathName, Option<u64>)>,
}

impl DirectoryChange {
    /// Starts one directory's final bindings.
    pub fn new(parent: u64) -> Self {
        Self {
            parent,
            changes: Vec::new(),
        }
    }

    /// Binds `name` to `serial`.
    pub fn bind(mut self, name: &str, serial: u64) -> Self {
        self.changes.push((name_of(name), Some(serial)));
        self
    }

    /// Ensures `name` is absent.
    pub fn unbind(mut self, name: &str) -> Self {
        self.changes.push((name_of(name), None));
        self
    }

    /// Sorts the bindings so the update satisfies the native input contract.
    pub fn sorted(mut self) -> Self {
        self.changes.sort_by(|left, right| left.0.cmp(&right.0));
        self
    }

    /// The checked bindings.
    pub fn into_changes(self) -> Vec<(PathName, Option<u64>)> {
        self.changes
    }
}

/// Checks and wraps one name.
pub fn name_of(value: &str) -> PathName {
    PathName::new(value).expect("test name")
}

/// One typed inode value with a synthetic content root.
pub fn value(kind: InodeKind, content_root: ObjectId, metadata_root: ObjectId) -> InodeValue {
    InodeValue {
        kind,
        namespace_ref_count: 0,
        content_root,
        metadata_root,
    }
}

/// A synthetic identity used as a content or attribute root placeholder.
pub fn synthetic(label: &str) -> ObjectId {
    ObjectId::for_bytes(format!("layerfs/stage5-fixture/{label}").as_bytes())
}

/// Default resource ceilings for a test operation.
pub fn resources() -> FilesystemResources {
    FilesystemResources::default()
}

/// Consumer that writes into a store shared with the operation's reader.
struct SharedConsumer(std::rc::Rc<std::cell::RefCell<TreeStore>>);

impl FinalizedConsumer for SharedConsumer {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.0.borrow_mut().accept(object)
    }
}

/// Reader that resolves an object the operation just emitted, or the base store.
struct Overlay<'a> {
    base: &'a TreeStore,
    emitted: std::rc::Rc<std::cell::RefCell<TreeStore>>,
}

impl AuthenticatedObjects for Overlay<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.emitted
            .borrow()
            .read_canonical_batch(ids)
            .or_else(|_| self.base.read_canonical_batch(ids))
    }
}

/// Runs one operation closure and appends only the objects it newly emitted.
pub fn with_objects<T>(
    store: &mut TreeStore,
    body: impl FnOnce(&mut FilesystemObjects<'_>) -> ContentResult<T>,
) -> ContentResult<T> {
    let shared = std::rc::Rc::new(std::cell::RefCell::new(TreeStore::new()));
    let mut consumer = SharedConsumer(shared.clone());
    let result = {
        let reader = Overlay {
            base: store,
            emitted: shared.clone(),
        };
        let mut objects = FilesystemObjects::new(&reader, &mut consumer);
        body(&mut objects)
    };
    let emitted = shared.borrow().clone();
    store.absorb(&emitted);
    result
}

/// One in-memory filesystem built by the public operations, for reuse in tests.
pub struct Session {
    /// Objects emitted and read so far.
    pub store: TreeStore,
    /// Current root identity.
    pub root: ObjectId,
    /// Current decoded root.
    pub value: layerfs_content::filesystem::FilesystemRoot,
    /// Allocation scope of every identity in this session.
    pub scope: layerfs_content::filesystem::InodeScope,
    /// Root directory serial.
    pub root_serial: u64,
    /// Next serial a test's allocator hands out.
    next_serial: u64,
    /// Serials already allocated in this session.
    pub allocated: Vec<u64>,
}

impl Session {
    /// Builds an empty filesystem whose root directory is emitted for real.
    pub fn new(root_serial: u64) -> ContentResult<Self> {
        let scope = layerfs_content::filesystem::scope_for_seed([0x11; 32]);
        let mut store = TreeStore::new();
        let directories = [layerfs_content::filesystem::DirectoryUpdate {
            parent: root_serial,
            changes: Vec::new(),
        }];
        let inodes = [layerfs_content::filesystem::InodeUpdate {
            serial: root_serial,
            value: value(
                InodeKind::Directory,
                synthetic("session/root-content"),
                synthetic("session/root-metadata"),
            ),
        }];
        let new_inodes = [root_serial];
        let input = layerfs_content::filesystem::FilesystemInput {
            base: None,
            scope,
            root_serial,
            directories: &directories,
            inodes: &inodes,
            new_inodes: &new_inodes,
            resources: resources(),
        };
        let result = with_objects(&mut store, |objects| {
            layerfs_content::filesystem::build_filesystem(objects, &input, None)
        })?;
        Ok(Self {
            store,
            root: result.root.0,
            value: result.value,
            scope,
            root_serial,
            next_serial: root_serial + 1,
            allocated: vec![root_serial],
        })
    }

    /// Allocates the next identity from this session's test allocator.
    pub fn allocate(&mut self) -> u64 {
        let serial = self.next_serial;
        self.next_serial += 1;
        self.allocated.push(serial);
        serial
    }

    /// Applies one complete final-state update.
    pub fn apply(
        &mut self,
        directories: &[layerfs_content::filesystem::DirectoryUpdate],
        inodes: &[layerfs_content::filesystem::InodeUpdate],
        new_inodes: &[u64],
    ) -> ContentResult<layerfs_content::filesystem::FilesystemResult> {
        let input = layerfs_content::filesystem::FilesystemInput {
            base: Some(layerfs_content::filesystem::FilesystemRootId(self.root)),
            scope: self.scope,
            root_serial: self.root_serial,
            directories,
            inodes,
            new_inodes,
            resources: resources(),
        };
        let result = with_objects(&mut self.store, |objects| {
            layerfs_content::filesystem::update_filesystem(objects, &input, None)
        })?;
        self.root = result.root.0;
        self.value = result.value;
        Ok(result)
    }

    /// Reads one path inside the current root.
    pub fn read(&self) -> ContentResult<layerfs_content::filesystem::FilesystemRead<'_>> {
        layerfs_content::filesystem::FilesystemRead::new(
            &self.store,
            layerfs_content::filesystem::FilesystemRootId(self.root),
        )
    }
}

/// Disposable directory removed when it is dropped.
pub struct TempDir {
    path: std::path::PathBuf,
}

impl TempDir {
    /// Creates a fresh unique directory under the system temporary directory.
    pub fn new(label: &str) -> Self {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "layerfs-stage5-ordering-{label}-{}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("temporary directory");
        Self { path }
    }

    /// Path of the directory.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Counters one recording backing keeps for external assertions.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BackingCounters {
    /// Runs the operation asked for.
    pub creates: u64,
    /// Appends issued.
    pub appends: u64,
    /// Bytes appended.
    pub append_bytes: u64,
    /// Random reads issued.
    pub reads: u64,
    /// Flushes issued.
    pub flushes: u64,
    /// Runs currently owned.
    pub live_runs: u64,
    /// Largest number of runs owned at once.
    pub peak_live_runs: u64,
    /// Release calls the operation made.
    pub releases: u64,
}

/// Caller-supplied ordering backing that records what the operation asked of it.
///
/// Failures are injected per capability so a test can hold one exact step at a
/// time; nothing here is product code and the product has no test hooks.
pub struct RecordingBacking {
    inner: layerfs_content::filesystem::references::backing::FileBacking,
    shared: std::rc::Rc<std::cell::RefCell<BackingCounters>>,
    /// Fail the append whose one-based ordinal is this value.
    pub fail_append_at: Option<u64>,
    /// Fail the read whose one-based ordinal is this value.
    pub fail_read_at: Option<u64>,
    /// Fail the flush whose one-based ordinal is this value.
    pub fail_flush_at: Option<u64>,
    /// Refuse the finishing release.
    pub refuse_release: bool,
}

impl RecordingBacking {
    /// A recording backing over a fresh local directory.
    pub fn new(directory: impl AsRef<std::path::Path>) -> Self {
        Self {
            inner: layerfs_content::filesystem::references::backing::FileBacking::new(directory),
            shared: std::rc::Rc::new(std::cell::RefCell::new(BackingCounters::default())),
            fail_append_at: None,
            fail_read_at: None,
            fail_flush_at: None,
            refuse_release: false,
        }
    }

    /// A recording backing with a fixed physical ceiling.
    pub fn with_capacity(directory: impl AsRef<std::path::Path>, bytes: u64) -> Self {
        Self {
            inner: layerfs_content::filesystem::references::backing::FileBacking::with_capacity(
                directory, bytes,
            ),
            shared: std::rc::Rc::new(std::cell::RefCell::new(BackingCounters::default())),
            fail_append_at: None,
            fail_read_at: None,
            fail_flush_at: None,
            refuse_release: false,
        }
    }

    /// Counters observed so far.
    pub fn counters(&self) -> BackingCounters {
        *self.shared.borrow()
    }

    /// True when the backing still owns storage.
    pub fn owns_storage(&self) -> bool {
        self.inner.owns_storage()
    }

    /// Runs the backing created, as the backing itself counted them.
    pub fn runs_created(&self) -> u64 {
        self.inner.runs()
    }

    /// Bytes the backing currently owns.
    pub fn held_bytes(&self) -> u64 {
        self.inner.held_bytes()
    }

    /// Largest simultaneous bytes the backing owned.
    pub fn peak_bytes(&self) -> u64 {
        self.inner.peak_bytes()
    }

    /// True when a removal this backing attempted failed.
    pub fn cleanup_failed(&self) -> bool {
        self.inner.cleanup_failed()
    }
}

impl layerfs_content::filesystem::references::backing::OrderingBacking for RecordingBacking {
    fn create_run(
        &mut self,
    ) -> layerfs_content::ContentResult<
        Box<dyn layerfs_content::filesystem::references::backing::OrderingRun>,
    > {
        let inner = self.inner.create_run()?;
        let counters = self.shared.borrow_mut();
        let mut counters = counters;
        counters.creates = counters.creates.saturating_add(1);
        counters.live_runs = counters.live_runs.saturating_add(1);
        counters.peak_live_runs = counters.peak_live_runs.max(counters.live_runs);
        drop(counters);
        Ok(Box::new(RecordingRun {
            inner,
            shared: self.shared.clone(),
            fail_append_at: self.fail_append_at,
            fail_read_at: self.fail_read_at,
            fail_flush_at: self.fail_flush_at,
        }))
    }

    fn held_bytes(&self) -> u64 {
        self.inner.held_bytes()
    }

    fn peak_bytes(&self) -> u64 {
        self.inner.peak_bytes()
    }

    fn capacity_bytes(&self) -> Option<u64> {
        self.inner.capacity_bytes()
    }

    fn cleanup_failed(&self) -> bool {
        self.inner.cleanup_failed()
    }

    fn release(&mut self) -> layerfs_content::ContentResult<()> {
        {
            let mut counters = self.shared.borrow_mut();
            counters.releases = counters.releases.saturating_add(1);
        }
        if self.refuse_release {
            return Err(ContentError::ResourceUnavailable {
                what: "ordering run cleanup",
            });
        }
        self.inner.release()
    }
}

/// One recorded run.
struct RecordingRun {
    inner: Box<dyn layerfs_content::filesystem::references::backing::OrderingRun>,
    shared: std::rc::Rc<std::cell::RefCell<BackingCounters>>,
    fail_append_at: Option<u64>,
    fail_read_at: Option<u64>,
    fail_flush_at: Option<u64>,
}

impl layerfs_content::filesystem::references::backing::OrderingRun for RecordingRun {
    fn append(&mut self, bytes: &[u8]) -> ContentResult<()> {
        let ordinal = {
            let mut counters = self.shared.borrow_mut();
            counters.appends = counters.appends.saturating_add(1);
            counters.append_bytes = counters.append_bytes.saturating_add(bytes.len() as u64);
            counters.appends
        };
        if self.fail_append_at == Some(ordinal) {
            return Err(ContentError::Io);
        }
        self.inner.append(bytes)
    }

    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> ContentResult<()> {
        let ordinal = {
            let mut counters = self.shared.borrow_mut();
            counters.reads = counters.reads.saturating_add(1);
            counters.reads
        };
        if self.fail_read_at == Some(ordinal) {
            return Err(ContentError::Io);
        }
        self.inner.read_at(offset, buffer)
    }

    fn flush(&mut self) -> ContentResult<()> {
        let ordinal = {
            let mut counters = self.shared.borrow_mut();
            counters.flushes = counters.flushes.saturating_add(1);
            counters.flushes
        };
        if self.fail_flush_at == Some(ordinal) {
            return Err(ContentError::Io);
        }
        self.inner.flush()
    }

    fn len(&self) -> u64 {
        self.inner.len()
    }
}

impl Drop for RecordingRun {
    fn drop(&mut self) {
        let mut counters = self.shared.borrow_mut();
        counters.live_runs = counters.live_runs.saturating_sub(1);
    }
}

/// Number of objects the sink emitted with `role`.
pub fn count_role(store: &TreeStore, role: ObjectRole) -> usize {
    store
        .order()
        .iter()
        .filter(|(_, seen)| *seen == role)
        .count()
}

/// Provider that records every demanded identity and the size of each wave.
pub struct CountingProvider<'a> {
    inner: &'a TreeStore,
    demanded: std::cell::RefCell<std::collections::BTreeSet<ObjectId>>,
    waves: std::cell::RefCell<Vec<usize>>,
}

impl<'a> CountingProvider<'a> {
    /// Wraps one store.
    pub fn new(inner: &'a TreeStore) -> Self {
        Self {
            inner,
            demanded: std::cell::RefCell::new(std::collections::BTreeSet::new()),
            waves: std::cell::RefCell::new(Vec::new()),
        }
    }

    /// Every identity this provider was asked for.
    pub fn demanded(&self) -> std::collections::BTreeSet<ObjectId> {
        self.demanded.borrow().clone()
    }

    /// Sizes of the demand waves, in order.
    pub fn waves(&self) -> Vec<usize> {
        self.waves.borrow().clone()
    }

    /// Largest single wave.
    pub fn peak_wave(&self) -> usize {
        self.waves.borrow().iter().copied().max().unwrap_or(0)
    }

    /// Total objects demanded.
    pub fn demands(&self) -> usize {
        self.waves.borrow().iter().sum()
    }
}

impl AuthenticatedObjects for CountingProvider<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.waves.borrow_mut().push(ids.len());
        let mut demanded = self.demanded.borrow_mut();
        for id in ids {
            demanded.insert(*id);
        }
        drop(demanded);
        self.inner.read_canonical_batch(ids)
    }
}
