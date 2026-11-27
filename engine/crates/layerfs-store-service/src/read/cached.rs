//! One call's storage read caches over its Store ports (#352 F-035): the
//! pack bodies, decoded groups and decode arena a demand leaves behind serve
//! the call's next demands, instead of each demand starting cold. The caches
//! keep the storage reader's byte bounds and are dropped with the call.
use crate::history::lookup as args;
use crate::store::{PortError, StorePorts};
use layerfs_bridge::control::SdkRefusal;
use layerfs_content::{
    compare::{EntryFacts, TreeTable},
    file::{mapping::FILE_STATE_CANONICAL_BYTES, FileView},
    filesystem::{
        directory::read::lookup, path::PathName, sorted::DirectoryRoot, FilesystemRootId,
    },
    object::inode_leaf::InodeKind,
    policy::WHOLE_FILE_CANONICAL_OVERHEAD,
    AuthenticatedObjects, ContentResult, ObjectId,
};
use layerfs_storage::ReadCache;
use std::{cell::RefCell, collections::HashMap, sync::Arc};

/// The caches are made at the call's first demand, not with the call: a
/// failure to make them is that demand's failure, so it refuses where a
/// demand's failure always did, never ahead of a lookup refusal (#818 R3
/// review).
pub struct Cached<'a> {
    ports: &'a StorePorts,
    cache: RefCell<Option<ReadCache>>,
    directories: RefCell<DirectoryMemo>,
}

/// Names a call has bound in saved directories, by the directory's root
/// (#818 H3). A directory root's contents never change, so an equal root
/// binds a name to the same serial in every version that holds it. The shape
/// is lane C's `DirectoryMemo` (`layerfs_content::compare::across`), which
/// replaces it once that lands: `get` answers unknown (`None`), known unbound
/// (`Some(None)`) or bound; `insert` keeps nothing more once full.
pub struct DirectoryMemo {
    entries: HashMap<ObjectId, HashMap<PathName, Option<u64>>>,
    len: usize,
    capacity: usize,
}

impl DirectoryMemo {
    /// Names a history call keeps at most.
    pub const CAPACITY: usize = 4_096;

    /// An empty memo keeping at most `capacity` names (0 keeps none).
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            len: 0,
            capacity,
        }
    }

    /// The binding of `name` in `directory`, when known.
    pub fn get(&self, directory: DirectoryRoot, name: &PathName) -> Option<Option<u64>> {
        self.entries.get(&directory.0)?.get(name).copied()
    }

    /// Keeps a binding; `false` when the memo is full.
    pub fn insert(
        &mut self,
        directory: DirectoryRoot,
        name: PathName,
        binding: Option<u64>,
    ) -> bool {
        if self.len >= self.capacity {
            return false;
        }
        if self
            .entries
            .entry(directory.0)
            .or_default()
            .insert(name, binding)
            .is_none()
        {
            self.len += 1;
        }
        true
    }
}

impl<'a> Cached<'a> {
    /// The call's caches over `ports`. Nothing is made yet, so this does
    /// not fail; the `Result` keeps the callers' shape.
    pub fn new(ports: &'a StorePorts) -> Result<Self, SdkRefusal> {
        Ok(Self {
            ports,
            cache: RefCell::new(None),
            directories: RefCell::new(DirectoryMemo::new(DirectoryMemo::CAPACITY)),
        })
    }

    /// The entry at a checked absolute `path` of the saved tree `root`,
    /// looked up strictly (as [`super::snapshot::lookup`]) through the
    /// call's caches, each directory's names bound through the call's
    /// directory memo (#818 H3): a history call walks the same path in many
    /// versions, and their unchanged directories bind it once.
    pub fn entry(&self, root: ObjectId, path: &str) -> Result<Option<EntryFacts>, SdkRefusal> {
        let tree = TreeTable::open(self, FilesystemRootId(root)).map_err(args::content)?;
        self.lookup(&tree, path)
    }

    /// [`Self::entry`] in a tree already open.
    pub fn lookup(&self, tree: &TreeTable, path: &str) -> Result<Option<EntryFacts>, SdkRefusal> {
        super::snapshot::lookup_bound(self, tree, path, &mut |serial, name| {
            self.bind(tree, serial, name)
        })
    }

    /// The serial `name` binds in directory `serial` of `tree`, through the
    /// memo; `None` when unbound or when `serial` is no directory.
    fn bind(&self, tree: &TreeTable, serial: u64, name: &PathName) -> ContentResult<Option<u64>> {
        let directory = match tree.value(self, serial)? {
            Some(value) if value.kind == InodeKind::Directory => DirectoryRoot(value.content_root),
            _ => return Ok(None),
        };
        if let Some(binding) = self.directories.borrow().get(directory, name) {
            return Ok(binding);
        }
        let binding = lookup(self, directory, name)?;
        self.directories
            .borrow_mut()
            .insert(directory, name.clone(), binding);
        Ok(binding)
    }

    /// Runs `demand` through the call's caches, made on first use.
    fn with<T>(
        &self,
        demand: impl FnOnce(&mut ReadCache) -> Result<T, Arc<PortError>>,
    ) -> Result<T, Arc<PortError>> {
        let mut slot = self.cache.borrow_mut();
        let cache = match &mut *slot {
            Some(cache) => cache,
            empty => {
                empty.insert(ReadCache::new().map_err(|error| Arc::new(PortError::Storage(error)))?)
            }
        };
        demand(cache)
    }

    /// The regular files `ids` opened as views, their roots read [`ROOTS`] at
    /// a time in demands of at most [`ROOT_BYTES`], so their delta chains are
    /// resolved level by level together instead of file by file. A root's
    /// canonical size is bounded from its file length, read first: a
    /// whole-file root is its payload plus a fixed overhead, a chunked or
    /// empty file's root is its fixed-size state.
    pub fn roots(&self, ids: &[ObjectId]) -> Result<HashMap<ObjectId, FileView>, SdkRefusal> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let lengths = self.lengths(ids)?;
        if lengths.len() != ids.len() {
            return Err(args::failed("file length cardinality"));
        }
        let mut views = HashMap::with_capacity(ids.len());
        for batch in batches(&lengths) {
            self.open(&ids[batch], &mut views)?;
        }
        Ok(views)
    }

    /// Regular files' lengths through the call's caches (#818 R4).
    pub fn lengths(&self, ids: &[ObjectId]) -> Result<Vec<u64>, SdkRefusal> {
        self.with(|cache| self.ports.lengths_with(cache, ids))
            .map_err(args::failed)
    }

    fn open(
        &self,
        ids: &[ObjectId],
        views: &mut HashMap<ObjectId, FileView>,
    ) -> Result<(), SdkRefusal> {
        let canonical = crate::telemetry::timed("sdk.lines", |timing| {
            timing
                .child("content.read")
                .run(|_| self.read_canonical_batch(ids))
        })
        .map_err(args::content)?;
        if canonical.len() != ids.len() {
            return Err(args::failed("root cardinality"));
        }
        for (id, canonical) in ids.iter().zip(canonical) {
            views.insert(
                *id,
                FileView::from_canonical(*id, canonical).map_err(args::content)?,
            );
        }
        Ok(())
    }
}

/// Roots read in one demand, and their canonical bytes at most: an eighth
/// of the storage reader's per-demand limit, so a batch holds little.
pub const ROOTS: usize = 64;
pub const ROOT_BYTES: usize = 4 * 1024 * 1024;

/// The most canonical bytes a regular file's root of `length` has.
pub fn root_bound(length: u64) -> usize {
    usize::try_from(length)
        .unwrap_or(usize::MAX)
        .saturating_add(WHOLE_FILE_CANONICAL_OVERHEAD)
        .max(FILE_STATE_CANONICAL_BYTES)
}

/// Consecutive runs of files, by their lengths, read one demand each: at
/// most [`ROOTS`] files and [`ROOT_BYTES`] of bounded root bytes, except a
/// single file over that bound, which goes alone.
pub fn batches(lengths: &[u64]) -> Vec<std::ops::Range<usize>> {
    let (mut runs, mut from, mut bytes) = (Vec::new(), 0, 0usize);
    for (at, &length) in lengths.iter().enumerate() {
        let size = root_bound(length);
        if at > from && (at - from == ROOTS || bytes.saturating_add(size) > ROOT_BYTES) {
            runs.push(from..at);
            (from, bytes) = (at, 0);
        }
        bytes = bytes.saturating_add(size);
    }
    if from < lengths.len() {
        runs.push(from..lengths.len());
    }
    runs
}

impl AuthenticatedObjects for Cached<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.with(|cache| self.ports.objects_with(cache, ids))
            .map_err(|error| error.object_demand())
    }
}
