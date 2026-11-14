//! Authenticated canonical acquisition and exact immutable cache keys.
use crate::cache::CanonicalCache;
use layerfs_content::file::cdc::MAXIMUM_CHUNK_BYTES;
use layerfs_content::file::mapping::{
    chunk_canonical_len, decode_node_with_context, ExtentNode, FileState, READ_NAVIGATION_WAVE,
};
use layerfs_content::filesystem::attributes::{read_portable, AttributeReadWork, PortableMetadata};
use layerfs_content::object::inode_leaf::{InodeKind, InodeMetadata};
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult, ObjectId};
use std::ops::Range;
use std::sync::Arc;

const DEMAND_IDS: usize = 4096;
const DEMAND_BYTES: usize = 32 * 1024 * 1024;
const CANONICAL_BYTES: usize = 16 * 1024 * 1024;
/// Largest object a memory-only read copies. Such a read runs inside an owner
/// job; a larger object is left to the ordinary demand outside it.
const RESIDENT_BYTES: usize = 64 * 1024;

/// Bytes a sequential base read fetches past its window (layerfs-project
/// #97, owner-approved 2026-11-06), so the following windows find their
/// objects in the cache. The chunks a window lacks are demanded in one
/// upstream batch, not one read wave at a time.
const READ_AHEAD: u64 = 8 * 1024 * 1024;
/// Smallest cache allowance that reads ahead: two windows, the one being
/// read and the one fetched ahead, so a fetch never evicts the window it
/// fetched. Read-ahead fetches only when the reader has passed the previous
/// window, so it never holds more than these two windows per reader (owner
/// cap: 16 MiB); an allowance follows at most one stream per two windows.
const READ_AHEAD_CACHE: usize = 2 * READ_AHEAD as usize;
/// Most chunks one read-ahead demand names: within [`DEMAND_IDS`] and, at
/// the largest chunk, within [`DEMAND_BYTES`]. A window with more extents is
/// fetched up to this many; the rest is read on demand.
const AHEAD_IDS: usize = {
    let by_bytes = DEMAND_BYTES / chunk_canonical_len(MAXIMUM_CHUNK_BYTES);
    if by_bytes < DEMAND_IDS {
        by_bytes
    } else {
        DEMAND_IDS
    }
};
/// Mapping pages one level of a read-ahead walk demands at most; the window
/// is cut at the first page past them.
const AHEAD_PAGES: usize = READ_NAVIGATION_WAVE;

/// Prototype switches (FUSE profile probe, not a product contract), from
/// `LAYERFS_PROBE_READ`, a comma list: `noverify` skips the client's BLAKE3
/// re-check of objects the in-process Store already authenticated; `single`
/// makes a demand for objects another demand is fetching wait for that
/// fetch; `async` runs read-ahead on its own thread ahead of the reader
/// (implies `single`). Unset, none applies.
#[derive(Clone, Copy, Default)]
pub(crate) struct Probe {
    pub(crate) noverify: bool,
    pub(crate) single: bool,
    pub(crate) ahead: bool,
}
pub(crate) fn probe() -> Probe {
    static PROBE: std::sync::OnceLock<Probe> = std::sync::OnceLock::new();
    *PROBE.get_or_init(|| {
        let value = std::env::var("LAYERFS_PROBE_READ").unwrap_or_default();
        let has = |name: &str| value.split(',').any(|part| part.trim() == name);
        let ahead = has("async");
        Probe {
            noverify: has("noverify"),
            single: has("single") || ahead,
            ahead,
        }
    })
}
/// Prototype: the ids a demand marked in flight, released (and waiters
/// woken) when it ends, whatever its outcome.
struct Flight<'c> {
    cache: &'c CanonicalCache,
    ids: Vec<ObjectId>,
}
impl Drop for Flight<'_> {
    fn drop(&mut self) {
        if self.ids.is_empty() {
            return;
        }
        if let Ok(mut state) = self.cache.state.lock() {
            for id in &self.ids {
                state.inflight.remove(id);
            }
        }
        self.cache.landed.notify_all();
    }
}

/// One mapping page a read-ahead walk reaches: its identity, whether it is
/// the root, where it starts and the summary its parent recorded.
struct Page {
    id: ObjectId,
    root: bool,
    base: u64,
    bytes: u64,
    extents: u64,
    level: u8,
}

/// One followed stream: a read of content `root` that starts at `next`
/// continues it. `until` is how far it has fetched.
#[derive(Clone, Copy)]
struct Ahead {
    root: ObjectId,
    next: u64,
    until: u64,
}

/// The sequential streams one cache allowance follows, least recently read
/// first. They live with the cache, not with a client: the daemon gives each
/// kernel request its own operation client, so a stream must outlive the
/// client that read it (layerfs-project #97). At most allowance / two
/// windows are followed, so every followed stream's two windows fit the
/// allowance together; another read takes the least recently read slot.
#[derive(Default)]
pub(crate) struct Streams(Vec<Ahead>, Vec<ObjectId>);
impl Streams {
    /// Records a read of `start..end` of `root` and returns how far its
    /// stream had fetched when it continues one.
    fn follow(&mut self, root: ObjectId, start: u64, end: u64, most: usize) -> Option<u64> {
        let found = self
            .0
            .iter()
            .position(|last| last.root == root && last.next == start);
        let until = found.map(|index| self.0.remove(index).until);
        self.0.push(Ahead {
            root,
            next: end,
            until: until.unwrap_or(end).max(end),
        });
        let excess = self.0.len().saturating_sub(most.max(1));
        self.0.drain(..excess);
        until
    }
    /// Prototype (`async`): whether a background read-ahead of `root` runs.
    fn busy(&self, root: ObjectId) -> bool {
        self.1.contains(&root)
    }
    pub(crate) fn finish(&mut self, root: ObjectId) {
        self.1.retain(|busy| *busy != root);
    }
    /// The latest read's stream has fetched up to `until`.
    fn fetched(&mut self, until: u64) {
        if let Some(last) = self.0.last_mut() {
            last.until = last.until.max(until);
        }
    }
}

/// Bounded cumulative object observations; no hidden namespace inventory.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ClientWork {
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub upstream_batches: u64,
    pub authenticated_bytes: u64,
    pub evictions: u64,
    pub charged_cache_bytes: usize,
    pub cached_objects: usize,
    /// Base directories whose child-directory count was derived by listing.
    pub directory_count_scans: u64,
    /// Child-directory counts currently remembered; at most
    /// [`crate::DIRECTORY_COUNT_CAPACITY`].
    pub directory_counts: usize,
    /// File lengths currently remembered. Each is charged to the cache
    /// allowance and counted in `charged_cache_bytes`, not in `cached_objects`.
    pub file_lengths: usize,
    /// Metadata roots whose portable mode and mtime were decoded from their
    /// objects: a remembered answer was absent.
    pub portable_metadata_reads: u64,
    /// Portable mode and mtime answers currently remembered. Each is charged
    /// to the cache allowance like a file length.
    pub portable_metadata: usize,
    /// Objects a published Commit constructed that were placed in the cache
    /// without a Store read. They are not misses and not authenticated bytes.
    pub seeded_objects: u64,
    /// Canonical bytes of those objects.
    pub seeded_bytes: u64,
    /// Whole-file lengths a published Commit constructed that were
    /// remembered without a Store demand.
    pub seeded_lengths: u64,
    /// The shared decoded-page cache's hits, misses, evictions and charge.
    pub decoded: layerfs_content::filesystem::decoded::DecodedWork,
}
/// A runtime-backed authenticated object provider with bounded immutable caching.
/// Upstream implements its public demand-window/authority contract; allocation
/// before upstream returns remains that provider's responsibility.
pub struct CanonicalClient {
    source: Arc<dyn AuthenticatedObjects + Send + Sync>,
    cache: Arc<CanonicalCache>,
    lengths: Option<Arc<dyn crate::FileLengths + Send + Sync>>,
    /// Answers only from the cache: no upstream demand is ever made.
    resident: bool,
    /// Prototype (FUSE profile probe, `async`): a fresh demand custody over
    /// the same reader, for a read-ahead that runs beside this operation.
    fresh: Option<Fresh>,
}
/// See [`CanonicalClient::with_fresh`].
pub type Fresh = Arc<dyn Fn() -> Arc<dyn AuthenticatedObjects + Send + Sync> + Send + Sync>;
/// The upstream of a memory-only client. It is never asked.
struct NoUpstream;
impl AuthenticatedObjects for NoUpstream {
    fn read_canonical_batch(&self, _: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        Err(NOT_RESIDENT)
    }
}
const NOT_RESIDENT: ContentError = ContentError::ProviderFailure {
    what: "base object not resident",
};
impl CanonicalClient {
    /// A client over objects already in `cache`, with no provider and no
    /// length provider: it answers a file length only when the cache
    /// remembers it. A demand for anything else fails without I/O, and
    /// without counting a miss: the ordinary demand that follows counts it.
    pub fn resident(cache: Arc<CanonicalCache>) -> Self {
        Self {
            source: Arc::new(NoUpstream),
            cache,
            lengths: None,
            resident: true,
            fresh: None,
        }
    }
    /// Selects a cache allowance. A large object can bypass cache without refusal.
    pub fn new(source: Arc<dyn AuthenticatedObjects + Send + Sync>, cache_bytes: usize) -> Self {
        Self::with_cache(source, None, Arc::new(CanonicalCache::new(cache_bytes)))
    }
    /// Adds the owning trusted Store-length port without payload classification.
    /// Missing capability fails explicitly; no FileView/stat fallback is used.
    pub fn with_lengths(
        source: Arc<dyn AuthenticatedObjects + Send + Sync>,
        lengths: Arc<dyn crate::FileLengths + Send + Sync>,
        cache_bytes: usize,
    ) -> Self {
        Self::with_cache(
            source,
            Some(lengths),
            Arc::new(CanonicalCache::new(cache_bytes)),
        )
    }
    /// Uses one existing cache allowance with this operation's owning providers.
    ///
    /// Provider failure/partial-result custody remains on the supplied source;
    /// it is never placed in the shared cache. The caller must authorize the
    /// operation and restrict cache sharing to its declared authorization context.
    /// Cached immutable identity alone supplies no authority or revocation fence.
    pub fn with_cache(
        source: Arc<dyn AuthenticatedObjects + Send + Sync>,
        lengths: Option<Arc<dyn crate::FileLengths + Send + Sync>>,
        cache: Arc<CanonicalCache>,
    ) -> Self {
        Self {
            source,
            cache,
            lengths,
            resident: false,
            fresh: None,
        }
    }
    /// Prototype (FUSE profile probe): how to make a fresh demand custody.
    pub fn with_fresh(mut self, fresh: Fresh) -> Self {
        self.fresh = Some(fresh);
        self
    }
    /// Prototype (`async`): a client over a fresh custody and the same cache.
    pub(crate) fn forked(&self) -> Option<Arc<Self>> {
        let fresh = self.fresh.as_ref()?;
        Some(Arc::new(Self {
            source: fresh(),
            cache: self.cache.clone(),
            lengths: None,
            resident: false,
            fresh: Some(fresh.clone()),
        }))
    }
    /// The length provider's own answer, asked every time: nothing is
    /// remembered and nothing remembered is used. Commit construction checks
    /// a captured file against it.
    pub(crate) fn provided_length(&self, id: ObjectId) -> crate::WorkspaceResult<u64> {
        self.lengths
            .as_ref()
            .ok_or(crate::WorkspaceError::MissingLengthProvider)?
            .file_length(id)
    }
    /// Whether the length of `id` is remembered, without a demand.
    pub(crate) fn remembers_length(&self, id: ObjectId) -> bool {
        self.state()
            .is_ok_and(|mut state| state.cache.length(id).is_some())
    }
    /// Remembers the lengths of those `ids` not remembered yet, asked of the
    /// provider in one demand (#356 F-032). A memory-only client, or one
    /// without a length provider, asks nothing.
    pub(crate) fn provide_lengths(&self, ids: &[ObjectId]) -> crate::WorkspaceResult<()> {
        let Some(provider) = self.lengths.as_ref().filter(|_| !self.resident) else {
            return Ok(());
        };
        let wanted: Vec<ObjectId> = {
            let mut state = self.state()?;
            let mut wanted: Vec<ObjectId> = ids
                .iter()
                .copied()
                .filter(|id| state.cache.length(*id).is_none())
                .collect();
            wanted.sort_unstable();
            wanted.dedup();
            wanted
        };
        if wanted.is_empty() {
            return Ok(());
        }
        let values = provider.file_lengths(&wanted)?;
        if values.len() != wanted.len() {
            return Err(ContentError::BatchCardinality {
                requested: wanted.len(),
                returned: values.len(),
            }
            .into());
        }
        let mut state = self.state()?;
        for (id, length) in wanted.into_iter().zip(values) {
            let evictions = state.cache.remember_length(id, length).unwrap_or(0);
            state.work.evictions = state.work.evictions.saturating_add(evictions);
        }
        Ok(())
    }
    /// Bounded cumulative acquisition counters and logical cache charge.
    pub fn diagnostics(&self) -> ContentResult<ClientWork> {
        self.cache.diagnostics()
    }
    /// What a base read of `start..end` of chunked content `root` (of
    /// `length` bytes) should fetch ahead, if anything. A read that continues
    /// a followed stream ([`Streams`]) and passes what it already fetched
    /// fetches the rest of its own range and up to [`READ_AHEAD`] bytes past
    /// it ([`Self::read_ahead`]), so the windows after it are served from the
    /// cache; any other read fetches nothing ahead and starts a stream. What
    /// is fetched ahead is cached like any other object, within the allowance.
    pub(crate) fn read_through(
        &self,
        root: ObjectId,
        start: u64,
        end: u64,
        length: u64,
    ) -> Option<Range<u64>> {
        if self.resident {
            return None;
        }
        let mut state = self.state().ok()?;
        let capacity = state.cache.capacity();
        if capacity < READ_AHEAD_CACHE {
            return None;
        }
        let until = state
            .streams
            .follow(root, start, end, capacity / READ_AHEAD_CACHE)?;
        if end <= until {
            return None;
        }
        let through = end.saturating_add(READ_AHEAD).min(length).max(end);
        state.streams.fetched(through);
        // At most one window of the read itself: a larger read fetches the
        // rest of its own range on demand.
        Some(until.max(start).max(end.saturating_sub(READ_AHEAD))..through)
    }
    /// Prototype (`async`): the range a background read-ahead should fetch
    /// for a read of `start..end` that continues a stream: from what the
    /// stream already fetched to one window past it, once the reader is
    /// within one window of that frontier and no read-ahead of this content
    /// runs. The read's own window is read on demand as usual.
    pub(crate) fn read_next(
        &self,
        root: ObjectId,
        start: u64,
        end: u64,
        length: u64,
    ) -> Option<Range<u64>> {
        if self.resident {
            return None;
        }
        let mut state = self.state().ok()?;
        let capacity = state.cache.capacity();
        if capacity < READ_AHEAD_CACHE {
            return None;
        }
        let until = state
            .streams
            .follow(root, start, end, capacity / READ_AHEAD_CACHE)?;
        if state.streams.busy(root) || until >= end.saturating_add(READ_AHEAD) {
            return None;
        }
        let from = until.max(end);
        let through = from.saturating_add(READ_AHEAD).min(length);
        if from >= through {
            return None;
        }
        state.streams.fetched(through);
        state.streams.1.push(root);
        Some(from..through)
    }
    /// Prototype (`async`): the background read-ahead of `root` ended.
    pub(crate) fn ahead_done(&self, root: ObjectId) {
        if let Ok(mut state) = self.state() {
            state.streams.finish(root);
        }
    }
    /// Places the chunks of `range` of chunked content `state` that the
    /// cache lacks in the cache, demanded in one upstream batch. The mapping
    /// pages on the way are demanded a level at a time, at most
    /// [`AHEAD_PAGES`] per level, and at most [`AHEAD_IDS`] chunks are
    /// fetched; the window is cut there and the rest is read on demand.
    /// Nothing is written anywhere and no fetched byte is held past the call
    /// except in the cache. A failure fails the read that asked for it.
    pub(crate) fn read_ahead(&self, state: FileState, range: Range<u64>) -> ContentResult<()> {
        let mut level = vec![Page {
            id: state.mapping_root,
            root: true,
            base: 0,
            bytes: state.logical_len,
            extents: state.extent_count,
            level: state.tree_level,
        }];
        let mut end = range.end;
        let mut chunks = Vec::new();
        while !level.is_empty() {
            let mut pages = Vec::with_capacity(level.len().min(AHEAD_PAGES));
            for page in level {
                let page_end = page
                    .base
                    .checked_add(page.bytes)
                    .ok_or(ContentError::LengthOverflow)?;
                if page_end <= range.start || page.base >= end {
                    continue;
                }
                if pages.len() == AHEAD_PAGES {
                    end = page.base;
                    break;
                }
                pages.push(page);
            }
            let ids: Vec<ObjectId> = pages.iter().map(|page| page.id).collect();
            let values = self.read_canonical_batch(&ids)?;
            level = Vec::new();
            for (page, value) in pages.iter().zip(values) {
                let node = decode_node_with_context(&value, page.root)?;
                if node.level() != page.level
                    || node.logical_len() != page.bytes
                    || node.extent_count() != page.extents
                {
                    return Err(ContentError::InvalidRecord("mapping page summary"));
                }
                match node {
                    ExtentNode::Leaf { extents, .. } => {
                        let mut at = page.base;
                        for extent in extents {
                            let next = at
                                .checked_add(u64::from(extent.logical_length()))
                                .ok_or(ContentError::LengthOverflow)?;
                            if at < end && next > range.start {
                                chunks.push(extent.payload_object_id());
                            }
                            at = next;
                        }
                    }
                    ExtentNode::Branch { children, .. } => {
                        let child_level = page
                            .level
                            .checked_sub(1)
                            .ok_or(ContentError::MappingDepthExceeded)?;
                        let (mut bytes, mut extents) = (0_u64, 0_u64);
                        for child in children {
                            level.push(Page {
                                id: child.child_object_id,
                                root: false,
                                base: page
                                    .base
                                    .checked_add(bytes)
                                    .ok_or(ContentError::LengthOverflow)?,
                                bytes: child
                                    .cumulative_logical_end
                                    .checked_sub(bytes)
                                    .ok_or(ContentError::InvalidRecord("mapping branch order"))?,
                                extents: child
                                    .cumulative_extent_end
                                    .checked_sub(extents)
                                    .ok_or(ContentError::InvalidRecord("mapping branch order"))?,
                                level: child_level,
                            });
                            bytes = child.cumulative_logical_end;
                            extents = child.cumulative_extent_end;
                        }
                    }
                }
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        chunks.retain(|id| seen.insert(*id));
        chunks.truncate(AHEAD_IDS);
        self.fetch_ahead(&chunks)
    }
    /// Demands the `ids` the cache lacks in one upstream batch and caches
    /// them, checked as any demanded object is; a failed batch caches none.
    fn fetch_ahead(&self, ids: &[ObjectId]) -> ContentResult<()> {
        let probe = probe();
        let mut flight = Flight {
            cache: &self.cache,
            ids: Vec::new(),
        };
        let wanted: Vec<ObjectId> = {
            let mut state = self.state()?;
            let wanted: Vec<ObjectId> = ids
                .iter()
                .copied()
                .filter(|id| !state.cache.within(*id, usize::MAX))
                .filter(|id| !probe.single || !state.inflight.contains(id))
                .collect();
            if probe.single {
                state.inflight.extend(wanted.iter().copied());
                flight.ids = wanted.clone();
            }
            if !wanted.is_empty() {
                state.work.cache_misses =
                    state.work.cache_misses.saturating_add(wanted.len() as u64);
                state.work.upstream_batches = state.work.upstream_batches.saturating_add(1);
            }
            wanted
        };
        if wanted.is_empty() {
            return Ok(());
        }
        // No cache lock spans transport, provider service or authentication.
        let fetched = self.source.read_canonical_batch(&wanted)?;
        if fetched.len() != wanted.len() {
            return Err(ContentError::BatchCardinality {
                requested: wanted.len(),
                returned: fetched.len(),
            });
        }
        let mut bytes = 0_usize;
        for (id, value) in wanted.iter().zip(&fetched) {
            if value.len() > CANONICAL_BYTES {
                return Err(ContentError::ObjectLimitExceeded {
                    limit: CANONICAL_BYTES,
                    actual: value.len(),
                });
            }
            bytes = bytes
                .checked_add(value.len())
                .ok_or(ContentError::LengthOverflow)?;
            if bytes > DEMAND_BYTES {
                return Err(ContentError::BoundedCapacityExceeded {
                    what: "base read-ahead bytes",
                    limit: DEMAND_BYTES as u64,
                    actual: bytes as u64,
                });
            }
            if !probe.noverify && ObjectId::for_bytes(value) != *id {
                return Err(ContentError::IdentityMismatch);
            }
        }
        let moved = layerfs_telemetry::timer::copies::switches().moved;
        let mut state = self.state()?;
        for (id, value) in wanted.into_iter().zip(fetched) {
            let length = value.len() as u64;
            // Prototype (copies experiment, `move`): the fetched object is
            // moved into the cache instead of copied.
            let evictions = if moved {
                state.cache.insert_shared(id, Arc::new(value))
            } else {
                state.cache.insert(id, &value)
            };
            state.work.evictions = state.work.evictions.saturating_add(evictions);
            state.work.authenticated_bytes = state.work.authenticated_bytes.saturating_add(length);
        }
        Ok(())
    }
    /// Whether this client answers from memory only.
    pub(crate) const fn is_resident(&self) -> bool {
        self.resident
    }
    fn state(&self) -> ContentResult<std::sync::MutexGuard<'_, crate::cache::State>> {
        self.cache
            .state
            .lock()
            .map_err(|_| ContentError::ProviderFailure {
                what: "base cache owner",
            })
    }
    /// The checked portable mode and mtime of one inode's metadata root. It
    /// is a pure function of that immutable root and the inode kind, so an
    /// answer decoded once from objects this client authenticated is
    /// remembered in the cache's own allowance and returned from memory
    /// afterwards, with no object acquired. An evicted answer costs the
    /// same reads again and nothing else; a failed read remembers nothing.
    pub(crate) fn portable(
        &self,
        root: InodeMetadata,
        kind: InodeKind,
    ) -> ContentResult<PortableMetadata> {
        let Some(root) = root.root() else {
            return read_portable(self, root, kind, &mut AttributeReadWork::default());
        };
        if let Some(remembered) = self.state()?.cache.portable(root, kind) {
            return Ok(remembered);
        }
        let metadata = read_portable(
            self,
            InodeMetadata::Root(root),
            kind,
            &mut AttributeReadWork::default(),
        )?;
        let mut state = self.state()?;
        let evictions = state.cache.remember_portable(root, kind, metadata);
        state.work.evictions = state.work.evictions.saturating_add(evictions);
        state.work.portable_metadata_reads = state.work.portable_metadata_reads.saturating_add(1);
        Ok(metadata)
    }
    /// The remembered child-directory count of one directory content root.
    pub(crate) fn directory_count(&self, root: ObjectId) -> ContentResult<Option<u64>> {
        Ok(self.state()?.counts.get(root))
    }
    /// Remembers one count a Commit constructed, without counting a scan.
    pub(crate) fn remember_constructed_count(
        &self,
        root: ObjectId,
        count: u64,
    ) -> ContentResult<()> {
        self.state()?.counts.insert(root, count);
        Ok(())
    }
    /// Remembers one derived count; the least recently used one makes room.
    pub(crate) fn remember_directory_count(&self, root: ObjectId, count: u64) -> ContentResult<()> {
        let mut state = self.state()?;
        state.counts.insert(root, count);
        state.work.directory_count_scans = state.work.directory_count_scans.saturating_add(1);
        Ok(())
    }
}
impl AuthenticatedObjects for CanonicalClient {
    fn decoded_pages(&self) -> Option<&layerfs_content::filesystem::decoded::DecodedPages> {
        Some(&self.cache.decoded)
    }
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        // Unshared, every value is this demand's own buffer: unwrapping it
        // copies nothing.
        Ok(self
            .demand(ids, false)?
            .into_iter()
            .map(|value| Arc::try_unwrap(value).unwrap_or_else(|shared| Vec::clone(&shared)))
            .collect())
    }
    /// Prototype (copies experiment, `share`): cache hits are the cached
    /// buffers themselves and a fetched object is cached and returned as one
    /// buffer. Unset, each value is copied exactly as [`Self::demand`] did.
    fn read_shared_batch_scoped(
        &self,
        ids: &[ObjectId],
        _scope: layerfs_telemetry::timer::TimingScope<'_>,
    ) -> ContentResult<Vec<Arc<Vec<u8>>>> {
        self.demand(ids, layerfs_telemetry::timer::copies::switches().shared)
    }
}
impl CanonicalClient {
    /// One bounded demand: cache hits, then one upstream batch for the rest,
    /// checked and cached. `shared` hands out and caches shared buffers
    /// instead of copies (prototype, copies experiment).
    fn demand(&self, ids: &[ObjectId], shared: bool) -> ContentResult<Vec<Arc<Vec<u8>>>> {
        let _stage = layerfs_telemetry::timer::stages::Timer::start(
            layerfs_telemetry::timer::stages::Stage::ClientDemand,
        );
        if ids.len() > DEMAND_IDS {
            return Err(ContentError::BoundedCapacityExceeded {
                what: "base demand ids",
                limit: DEMAND_IDS as u64,
                actual: ids.len() as u64,
            });
        }
        let probe = probe();
        let mut flight = Flight {
            cache: &self.cache,
            ids: Vec::new(),
        };
        let hit = |cache: &mut crate::cache::Cache, id: ObjectId, remaining: usize| {
            if shared {
                cache.get_shared(id, remaining)
            } else {
                Ok(cache.get(id, remaining)?.map(Arc::new))
            }
        };
        let mut values: Vec<Option<Arc<Vec<u8>>>> = vec![None; ids.len()];
        let mut misses = Vec::new();
        let mut cached_bytes = 0;
        {
            let mut state = self
                .cache
                .state
                .lock()
                .map_err(|_| ContentError::ProviderFailure {
                    what: "base cache owner",
                })?;
            for (index, id) in ids.iter().enumerate() {
                if self.resident && !state.cache.within(*id, RESIDENT_BYTES) {
                    return Err(NOT_RESIDENT);
                }
                if let Some(value) = hit(&mut state.cache, *id, DEMAND_BYTES - cached_bytes)? {
                    cached_bytes += value.len();
                    values[index] = Some(value);
                    state.work.cache_hits = state.work.cache_hits.saturating_add(1);
                } else {
                    misses.push((index, *id));
                    state.work.cache_misses = state.work.cache_misses.saturating_add(1);
                }
            }
            if probe.single && !misses.is_empty() {
                // Wait for objects another demand is fetching, then take
                // what landed; the rest this demand fetches itself.
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
                while misses.iter().any(|(_, id)| state.inflight.contains(id)) {
                    let left = deadline.saturating_duration_since(std::time::Instant::now());
                    if left.is_zero() {
                        break;
                    }
                    state = self
                        .cache
                        .landed
                        .wait_timeout(state, left)
                        .map_err(|_| ContentError::ProviderFailure {
                            what: "base cache owner",
                        })?
                        .0;
                }
                let mut rest = Vec::with_capacity(misses.len());
                for (index, id) in misses {
                    match hit(&mut state.cache, id, DEMAND_BYTES - cached_bytes)? {
                        Some(value) => {
                            cached_bytes += value.len();
                            values[index] = Some(value);
                        }
                        None => rest.push((index, id)),
                    }
                }
                misses = rest;
                let mine: Vec<ObjectId> = misses
                    .iter()
                    .map(|(_, id)| *id)
                    .filter(|id| !state.inflight.contains(id))
                    .collect();
                state.inflight.extend(mine.iter().copied());
                flight.ids = mine;
            }
            if !misses.is_empty() {
                state.work.upstream_batches = state.work.upstream_batches.saturating_add(1);
            }
        }
        // No cache lock spans transport, provider service or authentication.
        if !misses.is_empty() {
            let demand: Vec<_> = misses.iter().map(|(_, id)| *id).collect();
            let fetched = self.source.read_canonical_batch(&demand)?;
            if fetched.len() != misses.len() {
                return Err(ContentError::BatchCardinality {
                    requested: misses.len(),
                    returned: fetched.len(),
                });
            }
            for ((index, id), value) in misses.iter().zip(fetched) {
                if value.len() > CANONICAL_BYTES {
                    return Err(ContentError::ObjectLimitExceeded {
                        limit: CANONICAL_BYTES,
                        actual: value.len(),
                    });
                }
                if !probe.noverify && ObjectId::for_bytes(&value) != *id {
                    return Err(ContentError::IdentityMismatch);
                }
                values[*index] = Some(Arc::new(value));
            }
        }
        let mut result = Vec::with_capacity(ids.len());
        let mut bytes = 0_usize;
        for value in values {
            let value = value.ok_or(ContentError::BatchCardinality {
                requested: ids.len(),
                returned: result.len(),
            })?;
            bytes = bytes
                .checked_add(value.len())
                .ok_or(ContentError::LengthOverflow)?;
            if bytes > DEMAND_BYTES {
                return Err(ContentError::BoundedCapacityExceeded {
                    what: "base demand bytes",
                    limit: DEMAND_BYTES as u64,
                    actual: bytes as u64,
                });
            }
            result.push(value);
        }
        let mut state = self
            .cache
            .state
            .lock()
            .map_err(|_| ContentError::ProviderFailure {
                what: "base cache owner",
            })?;
        for (index, id) in misses {
            let evictions = if shared {
                state.cache.insert_shared(id, Arc::clone(&result[index]))
            } else {
                state.cache.insert(id, &result[index])
            };
            state.work.evictions = state.work.evictions.saturating_add(evictions);
            state.work.authenticated_bytes = state
                .work
                .authenticated_bytes
                .saturating_add(result[index].len() as u64);
        }
        Ok(result)
    }
}

impl crate::FileLengths for CanonicalClient {
    /// The length of one regular file's content root, for its attributes.
    /// It is a pure function of that immutable root, so an answer the
    /// provider gave once is remembered in the cache's own allowance and
    /// returned from memory afterwards, with no provider demand. A
    /// memory-only client answers nothing else. An evicted answer costs one
    /// more provider demand and nothing else.
    fn file_length(&self, id: ObjectId) -> crate::WorkspaceResult<u64> {
        if let Some(length) = self.state()?.cache.length(id) {
            return Ok(length);
        }
        let length = self.provided_length(id)?;
        let mut state = self.state()?;
        let evictions = state.cache.remember_length(id, length).unwrap_or(0);
        state.work.evictions = state.work.evictions.saturating_add(evictions);
        Ok(length)
    }
}
