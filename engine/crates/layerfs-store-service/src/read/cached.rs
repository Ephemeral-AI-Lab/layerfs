//! One call's storage read caches over its Store ports (#352 F-035): the
//! pack bodies, decoded groups and decode arena a demand leaves behind serve
//! the call's next demands, instead of each demand starting cold. The caches
//! keep the storage reader's byte bounds and are dropped with the call.
use crate::history::lookup as args;
use crate::store::{PortError, StorePorts};
use layerfs_content::{
    compare::{DirectoryMemo, EntryFacts, TreeTable},
    file::{mapping::FILE_STATE_CANONICAL_BYTES, FileView},
    filesystem::{
        directory::read::lookup, path::PathName, sorted::DirectoryRoot, FilesystemRootId,
    },
    object::inode_leaf::InodeKind,
    policy::WHOLE_FILE_CANONICAL_OVERHEAD,
    AuthenticatedObjects, ContentError, ContentResult, ObjectId,
};
use layerfs_protocol::sdk::SdkRefusal;
use layerfs_storage::ReadCache;
use std::{
    cell::{RefCell, RefMut},
    collections::HashMap,
    sync::Arc,
};

/// The caches are made at the call's first demand, not with the call: a
/// failure to make them is that demand's failure, so it refuses where a
/// demand's failure always did, never ahead of a lookup refusal (#818 R3
/// review).
///
/// An object a per-object read ([`AuthenticatedObjects::read_canonical_each`])
/// found missing, mismatched or undecodable keeps its error for the call
/// (#818 H5): a later demand that names it gets that error back and the
/// object is never read again, so a window read ahead of a walk refuses only
/// where the walk reaches the object, and no failed read is attempted twice.
/// The record holds at most [`MEMO`] objects: a per-object read is made
/// only while the record has room for every object it names to fail
/// ([`Cached::room`]), so every failure it meets is recorded; past that the
/// call reads ahead no more and each object is read where the call needs
/// it, once, as a point read is.
///
/// A call that plans its reads ahead (#818 H6) keeps the lengths it read per
/// file, each a length or that file's own failure, and answers a lengths
/// demand from them when it names only kept files; and it holds the roots it
/// read ahead, at most [`ROOT_BYTES`] until the next plan releases them, and
/// serves a demand from them when it names only held roots. A kept failure
/// is met where the call first asks for that file, as before.
pub struct Cached<'a> {
    ports: &'a StorePorts,
    cache: RefCell<Option<ReadCache>>,
    directories: RefCell<DirectoryMemo>,
    failed: RefCell<HashMap<ObjectId, ContentError>>,
    lengths: RefCell<HashMap<ObjectId, Result<u64, Arc<PortError>>>>,
    held: RefCell<(HashMap<ObjectId, Vec<u8>>, usize)>,
}

/// Names a history call's directory memo keeps at most (#818 H3): lane C's
/// [`DirectoryMemo`], shared by a call's lone walks and its version windows.
/// Also the most objects the call's failure record and kept lengths hold.
pub const MEMO: usize = 4_096;

/// A per-object read not made: the failure record has no room for it.
pub const FULL: ContentError = ContentError::ProviderFailure {
    what: "per-object failure record full",
};

impl<'a> Cached<'a> {
    /// The call's caches over `ports`. Nothing is made yet, so this does
    /// not fail; the `Result` keeps the callers' shape.
    pub fn new(ports: &'a StorePorts) -> Result<Self, SdkRefusal> {
        Ok(Self {
            ports,
            cache: RefCell::new(None),
            directories: RefCell::new(DirectoryMemo::new(MEMO)),
            failed: RefCell::new(HashMap::new()),
            lengths: RefCell::new(HashMap::new()),
            held: RefCell::new((HashMap::new(), 0)),
        })
    }

    /// A file's kept length or kept failure (#818 H6).
    pub fn length(&self, id: ObjectId) -> Option<Result<u64, Arc<PortError>>> {
        self.lengths.borrow().get(&id).cloned()
    }

    /// Reads the lengths of the `ids` not kept yet in one per-object demand
    /// and keeps each, a length or the file's own failure, while the record
    /// has room ([`MEMO`]); a failure of the demand itself refuses.
    pub fn lengths_each(&self, ids: &[ObjectId]) -> Result<(), SdkRefusal> {
        let wanted = {
            let kept = self.lengths.borrow();
            let mut wanted: Vec<ObjectId> = Vec::new();
            for id in ids {
                if !kept.contains_key(id) && !wanted.contains(id) {
                    wanted.push(*id);
                }
            }
            wanted.truncate(MEMO.saturating_sub(kept.len()));
            wanted
        };
        if wanted.is_empty() {
            return Ok(());
        }
        let each = self
            .with(|cache| self.ports.lengths_each_with(cache, &wanted))
            .map_err(args::failed)?;
        if each.len() != wanted.len() {
            return Err(args::failed("file length cardinality"));
        }
        self.lengths
            .borrow_mut()
            .extend(wanted.into_iter().zip(each));
        Ok(())
    }

    /// Releases the held roots, then reads the `ids` not held, not failed,
    /// in one per-object demand and holds each one read while the held bytes
    /// stay within [`ROOT_BYTES`]; an object's own failure is recorded as
    /// every per-object read records it.
    pub fn hold_each(&self, ids: &[ObjectId]) -> ContentResult<()> {
        *self.held.borrow_mut() = (HashMap::new(), 0);
        let mut wanted: Vec<ObjectId> = Vec::new();
        for id in ids {
            if !wanted.contains(id) {
                wanted.push(*id);
            }
        }
        // Nothing is read ahead once the failure record could fill.
        if wanted.is_empty() || !self.room(wanted.len()) {
            return Ok(());
        }
        let each = self.read_canonical_each(&wanted)?;
        let mut held = self.held.borrow_mut();
        for (id, read) in wanted.into_iter().zip(each) {
            if let Ok(bytes) = read {
                if held.1.saturating_add(bytes.len()) <= ROOT_BYTES {
                    held.1 += bytes.len();
                    held.0.insert(id, bytes);
                }
            }
        }
        Ok(())
    }

    /// The held roots `ids` name, when every one is held.
    fn held(&self, ids: &[ObjectId]) -> Option<Vec<Vec<u8>>> {
        let held = self.held.borrow();
        if held.0.is_empty() {
            return None;
        }
        ids.iter().map(|id| held.0.get(id).cloned()).collect()
    }

    /// Whether the Store pays a round trip per demand (PostgreSQL), where
    /// `fs.grep` and `fs.glob` take wide windows (#818 R6).
    pub fn wide(&self) -> bool {
        self.ports.batched_reads()
    }

    /// The call's directory memo, for a window of versions (#818 H5).
    pub fn memo(&self) -> RefMut<'_, DirectoryMemo> {
        self.directories.borrow_mut()
    }

    /// Whether the failure record has room for `count` more objects, so a
    /// per-object read of that many can record every failure it meets.
    pub fn room(&self, count: usize) -> bool {
        self.failed.borrow().len().saturating_add(count) <= MEMO
    }

    /// The recorded failure of the first of `ids` a per-object read found
    /// failed, in request order.
    fn recorded(&self, ids: &[ObjectId]) -> Option<ContentError> {
        let failed = self.failed.borrow();
        if failed.is_empty() {
            return None;
        }
        ids.iter().find_map(|id| failed.get(id).cloned())
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
    pub fn roots(
        &self,
        ids: &[ObjectId],
        count: usize,
    ) -> Result<HashMap<ObjectId, FileView>, SdkRefusal> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let lengths = self.lengths(ids)?;
        if lengths.len() != ids.len() {
            return Err(args::failed("file length cardinality"));
        }
        let mut views = HashMap::with_capacity(ids.len());
        for batch in batches_of(&lengths, count) {
            self.open(&ids[batch], &mut views)?;
        }
        Ok(views)
    }

    /// Regular files' lengths through the call's caches (#818 R4).
    /// Answered from the kept lengths when every file is kept (#818 H6),
    /// the first kept failure in request order refusing as the demand's
    /// failure would.
    pub fn lengths(&self, ids: &[ObjectId]) -> Result<Vec<u64>, SdkRefusal> {
        let kept: Option<Vec<_>> = {
            let lengths = self.lengths.borrow();
            ids.iter().map(|id| lengths.get(id).cloned()).collect()
        };
        match kept {
            Some(kept) if !ids.is_empty() => kept
                .into_iter()
                .map(|kept| kept.map_err(args::failed))
                .collect(),
            _ => self
                .with(|cache| self.ports.lengths_with(cache, ids))
                .map_err(args::failed),
        }
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
/// A wide window's files and roots a demand at most (#818 R6): where the
/// Store pays a round trip per demand (PostgreSQL), `fs.grep` lets
/// [`ROOT_BYTES`] decide, up to this many.
pub const WIDE_ROOTS: usize = 256;

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
    batches_of(lengths, ROOTS)
}

/// [`batches`] of at most `count` files each.
pub fn batches_of(lengths: &[u64], count: usize) -> Vec<std::ops::Range<usize>> {
    let (mut runs, mut from, mut bytes) = (Vec::new(), 0, 0usize);
    for (at, &length) in lengths.iter().enumerate() {
        let size = root_bound(length);
        if at > from && (at - from == count || bytes.saturating_add(size) > ROOT_BYTES) {
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
        if let Some(error) = self.recorded(ids) {
            return Err(error);
        }
        if let Some(held) = self.held(ids) {
            return Ok(held);
        }
        self.with(|cache| self.ports.objects_with(cache, ids))
            .map_err(|error| error.object_demand())
    }

    /// Each object's own result (#818 H5), through the call's caches: an
    /// object's own failure is recorded and returned for it; a failure of the
    /// demand itself (the session, the transport) fails the call. An object
    /// recorded failed is not read again.
    ///
    /// Each object is read once however often `ids` names it. A demand that
    /// could overfill the failure record ([`Cached::room`]) is not made: it
    /// fails as a whole with [`FULL`], which a caller reading ahead checks
    /// for first and never meets.
    fn read_canonical_each(&self, ids: &[ObjectId]) -> ContentResult<Vec<ContentResult<Vec<u8>>>> {
        let wanted: Vec<ObjectId> = {
            let failed = self.failed.borrow();
            let mut wanted: Vec<ObjectId> = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for id in ids {
                if !failed.contains_key(id) && seen.insert(*id) {
                    wanted.push(*id);
                }
            }
            wanted
        };
        if !self.room(wanted.len()) {
            return Err(FULL);
        }
        let read = if wanted.is_empty() {
            Vec::new()
        } else {
            self.with(|cache| self.ports.objects_each_with(cache, &wanted))
                .map_err(|error| error.object_demand())?
        };
        if read.len() != wanted.len() {
            return Err(ContentError::BatchCardinality {
                requested: wanted.len(),
                returned: read.len(),
            });
        }
        let mut found: HashMap<ObjectId, Vec<u8>> = HashMap::with_capacity(wanted.len());
        {
            let mut failed = self.failed.borrow_mut();
            for (id, read) in wanted.into_iter().zip(read) {
                match read {
                    Ok(bytes) => {
                        found.insert(id, bytes);
                    }
                    Err(error) => {
                        failed.insert(id, error.object_demand());
                    }
                }
            }
        }
        Ok(ids
            .iter()
            .map(|id| match self.recorded(std::slice::from_ref(id)) {
                Some(error) => Err(error),
                None => found.get(id).cloned().ok_or(ContentError::MissingObject),
            })
            .collect())
    }

    fn reads_each_at_once(&self) -> bool {
        self.ports.reads_each_at_once()
    }
}
