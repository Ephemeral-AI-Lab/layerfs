use super::extent::{
    ChildDescriptorV3, ExtentNodeV3, ExtentSliceV3, FileStateV3, MAX_ENTRIES, MAX_LEVEL,
};
use super::extent_codec::{
    decode_file_state, decode_node_with_context, encode_file_state, encode_node, profile_id,
};
use crate::cdc::FastCdc;
use crate::{encode_bytes_object, CoreError, CoreResult, ObjectId};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::ops::Range;

const STREAM_FLUSH_AT: usize = MAX_ENTRIES + 64;

pub use crate::object::access::{ObjectRead, ObjectStore};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RopeCounters {
    pub payload_bytes_read: u64,
    pub payload_bytes_written: u64,
    pub cdc_bytes_scanned: u64,
    pub chunks_created: u64,
    pub nodes_read: u64,
    pub nodes_created: u64,
    pub deferred_peak_bytes: u64,
    pub deferred_prunes: u64,
    pub tree_level_before: Option<u8>,
    pub logical_len_before: Option<u64>,
    pub logical_len_after: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileStateRoot(pub ObjectId);

#[derive(Clone, Debug)]
pub struct ReadPlan {
    state: FileStateV3,
    mapping: ExtentNodeV3,
}

impl ReadPlan {
    pub fn logical_len(&self) -> u64 {
        self.state.logical_len
    }
}

#[derive(Clone, Copy)]
struct Summary {
    id: ObjectId,
    bytes: u64,
    extents: u64,
    level: u8,
}

enum Pending {
    Extents(Vec<ExtentSliceV3>),
    Children(Vec<Summary>),
}

struct ReplacementScan {
    levels: Vec<Pending>,
    counters: RopeCounters,
    bytes_scanned: u64,
    pending: BTreeMap<ObjectId, Vec<u8>>,
    persisted_nodes: u64,
}

pub fn build<S: ObjectStore, R: Read>(
    store: &mut S,
    source: R,
) -> CoreResult<(FileStateRoot, RopeCounters)> {
    let (root, mut counters) = build_mapping(store, source)?;
    let root = match root {
        Some(root) => root,
        None => emit_leaf(store, Vec::new(), &mut counters)?,
    };
    let state = FileStateV3 {
        logical_len: root.bytes,
        extent_count: root.extents,
        tree_level: root.level,
        profile_id: profile_id(),
        mapping_root: root.id,
    };
    let canonical = encode_file_state(state)?;
    let id = store.put(&canonical)?;
    Ok((FileStateRoot(id), counters))
}

fn build_mapping<S: ObjectStore, R: Read>(
    store: &mut S,
    source: R,
) -> CoreResult<(Option<Summary>, RopeCounters)> {
    let (mut levels, mut counters, bytes_scanned) = scan_mapping(store, source)?;
    if bytes_scanned == 0 {
        Ok((None, counters))
    } else {
        let root = finish(store, &mut levels, &mut counters)?;
        Ok((Some(root), counters))
    }
}

fn scan_mapping<S: ObjectStore, R: Read>(
    store: &mut S,
    source: R,
) -> CoreResult<(Vec<Pending>, RopeCounters, u64)> {
    let mut levels = vec![Pending::Extents(Vec::with_capacity(STREAM_FLUSH_AT + 1))];
    let mut counters = RopeCounters::default();
    let cdc = FastCdc::new().scan(source, |chunk| {
        let canonical = encode_bytes_object(chunk)?;
        let payload = store.put(&canonical)?;
        counters.payload_bytes_written = add(counters.payload_bytes_written, chunk.len() as u64)?;
        counters.chunks_created = add(counters.chunks_created, 1)?;
        match &mut levels[0] {
            Pending::Extents(extents) => {
                extents.push(ExtentSliceV3::new(payload, 0, chunk.len() as u32)?)
            }
            Pending::Children(_) => unreachable!(),
        }
        flush_streaming(store, &mut levels, 0, &mut counters)
    })?;
    counters.cdc_bytes_scanned = cdc.bytes_scanned;
    Ok((levels, counters, cdc.bytes_scanned))
}

fn scan_replacement_mapping_with<S, R, FP, FN>(
    store: &mut S,
    source: R,
    mut put_payload: FP,
    mut put_sealed_node: FN,
) -> CoreResult<ReplacementScan>
where
    S: ObjectStore,
    R: Read,
    FP: FnMut(&mut S, &[u8]) -> CoreResult<ObjectId>,
    FN: FnMut(&mut S, &[u8]) -> CoreResult<ObjectId>,
{
    let mut deferred = DeferredNodes::new(store);
    let mut levels = vec![Pending::Extents(Vec::with_capacity(STREAM_FLUSH_AT + 1))];
    let mut counters = RopeCounters::default();
    let mut flushed = 0_u64;
    let cdc = FastCdc::new().scan(source, |chunk| {
        let canonical = encode_bytes_object(chunk)?;
        let payload = put_payload(deferred.store, &canonical)?;
        counters.payload_bytes_written = add(counters.payload_bytes_written, chunk.len() as u64)?;
        counters.chunks_created = add(counters.chunks_created, 1)?;
        match &mut levels[0] {
            Pending::Extents(extents) => {
                extents.push(ExtentSliceV3::new(payload, 0, chunk.len() as u32)?)
            }
            Pending::Children(_) => unreachable!(),
        }
        flush_streaming(&mut deferred, &mut levels, 0, &mut counters)?;
        flushed = add(
            flushed,
            deferred.flush_sealed_with(&levels, &mut put_sealed_node)?,
        )?;
        Ok(())
    })?;
    counters.cdc_bytes_scanned = cdc.bytes_scanned;
    Ok(ReplacementScan {
        levels,
        counters,
        bytes_scanned: cdc.bytes_scanned,
        pending: deferred.into_nodes(),
        persisted_nodes: flushed,
    })
}

pub fn state<S: ObjectRead>(
    store: &S,
    root: FileStateRoot,
    counters: &mut RopeCounters,
) -> CoreResult<FileStateV3> {
    counters.nodes_read = add(counters.nodes_read, 1)?;
    store.with_authenticated_canonical(root.0, decode_file_state)
}

pub fn validate_file<S: ObjectRead>(store: &S, root: FileStateRoot) -> CoreResult<()> {
    let mut counters = RopeCounters::default();
    let state = state(store, root, &mut counters)?;
    validate_node(
        store,
        Summary {
            id: state.mapping_root,
            bytes: state.logical_len,
            extents: state.extent_count,
            level: state.tree_level,
        },
        true,
        &mut counters,
        &mut Vec::new(),
    )
}

pub fn read_range<S: ObjectRead, W: Write>(
    store: &S,
    root: FileStateRoot,
    range: Range<u64>,
    mut sink: W,
) -> CoreResult<RopeCounters> {
    let mut counters = RopeCounters::default();
    let plan = read_plan(store, root, &mut counters)?;
    read_range_with_plan_into(store, &plan, range, &mut sink, &mut counters)?;
    Ok(counters)
}

pub fn read_plan<S: ObjectRead>(
    store: &S,
    root: FileStateRoot,
    counters: &mut RopeCounters,
) -> CoreResult<ReadPlan> {
    let state = state(store, root, counters)?;
    let summary = Summary {
        id: state.mapping_root,
        bytes: state.logical_len,
        extents: state.extent_count,
        level: state.tree_level,
    };
    counters.nodes_read = add(counters.nodes_read, 1)?;
    let mapping = store.with_authenticated_canonical(summary.id, |canonical| {
        decode_node_with_context(canonical, true)
    })?;
    validate_summary(&mapping, summary)?;
    Ok(ReadPlan { state, mapping })
}

pub fn read_range_with_plan<S: ObjectRead, W: Write>(
    store: &S,
    plan: &ReadPlan,
    range: Range<u64>,
    mut sink: W,
) -> CoreResult<RopeCounters> {
    let mut counters = RopeCounters::default();
    read_range_with_plan_into(store, plan, range, &mut sink, &mut counters)?;
    Ok(counters)
}

fn read_range_with_plan_into<S: ObjectRead, W: Write>(
    store: &S,
    plan: &ReadPlan,
    range: Range<u64>,
    sink: &mut W,
    counters: &mut RopeCounters,
) -> CoreResult<()> {
    if range.start > range.end || range.end > plan.state.logical_len {
        return Err(CoreError::InvalidRange {
            start: range.start,
            end: range.end,
            length: plan.state.logical_len,
        });
    }
    if range.is_empty() {
        return Ok(());
    }
    let summary = Summary {
        id: plan.state.mapping_root,
        bytes: plan.state.logical_len,
        extents: plan.state.extent_count,
        level: plan.state.tree_level,
    };
    let mut ancestors = vec![summary.id];
    let mut selected = Vec::with_capacity(64);
    read_decoded_node(
        store,
        0,
        &range,
        sink,
        counters,
        &mut ancestors,
        &plan.mapping,
        &mut selected,
    )?;
    flush_read_batch(store, sink, counters, &mut selected)?;
    Ok(())
}

pub fn read_all<S: ObjectRead, W: Write>(
    store: &S,
    root: FileStateRoot,
    sink: W,
) -> CoreResult<RopeCounters> {
    read_all_bounded(store, root, u64::MAX, sink)
}

pub fn read_all_bounded<S: ObjectRead, W: Write>(
    store: &S,
    root: FileStateRoot,
    maximum: u64,
    mut sink: W,
) -> CoreResult<RopeCounters> {
    let mut counters = RopeCounters::default();
    let state = state(store, root, &mut counters)?;
    if state.logical_len > maximum {
        return Err(CoreError::ObjectLimitExceeded);
    }
    let mut selected = Vec::with_capacity(64);
    read_node(
        store,
        Summary {
            id: state.mapping_root,
            bytes: state.logical_len,
            extents: state.extent_count,
            level: state.tree_level,
        },
        true,
        0,
        &(0..state.logical_len),
        &mut sink,
        &mut counters,
        &mut Vec::new(),
        &mut selected,
    )?;
    flush_read_batch(store, &mut sink, &mut counters, &mut selected)?;
    Ok(counters)
}

pub fn visit_extents<S: ObjectRead>(
    store: &S,
    root: FileStateRoot,
    mut visitor: impl FnMut(&[ExtentSliceV3]) -> CoreResult<()>,
) -> CoreResult<(FileStateV3, RopeCounters)> {
    let mut counters = RopeCounters::default();
    let state = state(store, root, &mut counters)?;
    visit_extent_node(
        store,
        Summary {
            id: state.mapping_root,
            bytes: state.logical_len,
            extents: state.extent_count,
            level: state.tree_level,
        },
        true,
        &mut counters,
        &mut Vec::new(),
        &mut visitor,
    )?;
    Ok((state, counters))
}

/// Emits coalesced logical ranges whose extent identities differ. Equal file
/// or mapping object identities stop before fetching descendants. A false
/// return means the logical lengths differ and the caller must use a full
/// fallback; mapping nodes and payloads are not read in that case.
pub fn diff_ranges<S: ObjectRead>(
    store: &S,
    old: FileStateRoot,
    new: FileStateRoot,
    mut visitor: impl FnMut(Range<u64>) -> CoreResult<()>,
) -> CoreResult<(bool, RopeCounters)> {
    if old == new {
        return Ok((true, RopeCounters::default()));
    }
    let mut counters = RopeCounters::default();
    let old_state = state(store, old, &mut counters)?;
    let new_state = state(store, new, &mut counters)?;
    if old_state.logical_len != new_state.logical_len {
        return Ok((false, counters));
    }
    let old_summary = Summary {
        id: old_state.mapping_root,
        bytes: old_state.logical_len,
        extents: old_state.extent_count,
        level: old_state.tree_level,
    };
    let new_summary = Summary {
        id: new_state.mapping_root,
        bytes: new_state.logical_len,
        extents: new_state.extent_count,
        level: new_state.tree_level,
    };
    if old_summary.id == new_summary.id {
        if !same_summary(old_summary, new_summary) {
            return Err(CoreError::InvalidRecord("extent summary"));
        }
        return Ok((true, counters));
    }
    let mut emitter = ChangedRanges {
        start: None,
        visitor: &mut visitor,
    };
    let mut old_cache = None;
    let mut new_cache = None;
    diff_span(
        store,
        Span::node(old_summary),
        Span::node(new_summary),
        0,
        &mut counters,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut old_cache,
        &mut new_cache,
        &mut emitter,
    )?;
    emitter.finish(old_state.logical_len)?;
    Ok((true, counters))
}

pub fn replace<S: ObjectStore, R: Read>(
    store: &mut S,
    root: FileStateRoot,
    start: u64,
    delete_len: u64,
    replacement: R,
) -> CoreResult<(FileStateRoot, RopeCounters)> {
    replace_with_sinks(
        store,
        root,
        start,
        delete_len,
        replacement,
        |store, canonical| store.put(canonical),
        |store, canonical| store.put(canonical),
    )
}

fn replace_with_sinks<S, R, FP, FN>(
    store: &mut S,
    root: FileStateRoot,
    start: u64,
    delete_len: u64,
    replacement: R,
    put_payload: FP,
    put_sealed_node: FN,
) -> CoreResult<(FileStateRoot, RopeCounters)>
where
    S: ObjectStore,
    R: Read,
    FP: FnMut(&mut S, &[u8]) -> CoreResult<ObjectId>,
    FN: FnMut(&mut S, &[u8]) -> CoreResult<ObjectId>,
{
    let mut counters = RopeCounters::default();
    let old = state(store, root, &mut counters)?;
    counters.tree_level_before = Some(old.tree_level);
    counters.logical_len_before = Some(old.logical_len);
    let end = start
        .checked_add(delete_len)
        .ok_or(CoreError::LengthOverflow)?;
    if end > old.logical_len {
        return Err(CoreError::InvalidRange {
            start,
            end,
            length: old.logical_len,
        });
    }
    let old_summary = Summary {
        id: old.mapping_root,
        bytes: old.logical_len,
        extents: old.extent_count,
        level: old.tree_level,
    };
    let scan = scan_replacement_mapping_with(store, replacement, put_payload, put_sealed_node)?;
    merge_counters(&mut counters, scan.counters)?;
    let persisted_nodes = scan.persisted_nodes;
    let mut levels = scan.levels;
    let mut deferred = DeferredNodes::with_nodes(store, scan.pending);
    let middle = if scan.bytes_scanned == 0 {
        None
    } else {
        let root = finish(&mut deferred, &mut levels, &mut counters)?;
        Some(root)
    };
    let (left, tail) = split(&mut deferred, old_summary, start, true, &mut counters)?;
    let (_, right) = split_optional(&mut deferred, tail, delete_len, &mut counters)?;
    let prefix = concat_optional(&mut deferred, left, middle, &mut counters)?;
    let joined = concat_optional(&mut deferred, prefix, right, &mut counters)?;
    let mapping = match joined {
        Some(summary) => summary,
        None => emit_leaf(&mut deferred, Vec::new(), &mut counters)?,
    };
    let committed = deferred.commit(mapping)?;
    counters.nodes_created = add(persisted_nodes, committed)?;
    let next = FileStateV3 {
        logical_len: mapping.bytes,
        extent_count: mapping.extents,
        tree_level: mapping.level,
        profile_id: profile_id(),
        mapping_root: mapping.id,
    };
    counters.logical_len_after = Some(next.logical_len);
    let canonical = encode_file_state(next)?;
    let id = store.put(&canonical)?;
    Ok((FileStateRoot(id), counters))
}

/// Coalesces multiple ordered file edits in one private object overlay and
/// publishes only the extent objects reachable from the final file state.
/// The returned root is byte-identical to applying [`replace`] in the same
/// order directly to the underlying store. Edits containing replacement bytes
/// must describe non-overlapping final ranges in ascending order; callers must
/// normalize repeated writes before constructing the batch.
pub struct FileMutationBatch<'a, S> {
    objects: DeferredFileObjects<'a, S>,
    root: FileStateRoot,
    counters: RopeCounters,
    initial_level: u8,
    initial_len: u64,
    current_len: u64,
    finalized_through: u64,
}

impl<'a, S: ObjectStore> FileMutationBatch<'a, S> {
    pub fn new(store: &'a mut S, root: Option<FileStateRoot>) -> CoreResult<Self> {
        let mut objects = DeferredFileObjects::new(store);
        let (root, mut counters) = match root {
            Some(root) => (root, RopeCounters::default()),
            None => build(&mut objects, std::io::empty())?,
        };
        let state = state(&objects, root, &mut counters)?;
        Ok(Self {
            objects,
            root,
            counters,
            initial_level: state.tree_level,
            initial_len: state.logical_len,
            current_len: state.logical_len,
            finalized_through: 0,
        })
    }

    pub fn logical_len(&self) -> CoreResult<u64> {
        Ok(self.current_len)
    }

    pub fn deferred_peak_bytes(&self) -> u64 {
        self.objects.peak_charged_bytes as u64
    }

    pub fn deferred_prunes(&self) -> u64 {
        self.objects.prunes
    }

    pub fn replace<R: Read>(
        &mut self,
        start: u64,
        delete_len: u64,
        replacement: R,
    ) -> CoreResult<()> {
        if start < self.finalized_through {
            return Err(CoreError::InvalidRange {
                start,
                end: start
                    .checked_add(delete_len)
                    .ok_or(CoreError::LengthOverflow)?,
                length: self.current_len,
            });
        }
        let (root, counters) = replace_with_sinks(
            &mut self.objects,
            self.root,
            start,
            delete_len,
            replacement,
            |objects, canonical| objects.put_payload(canonical),
            |objects, canonical| objects.put_sealed_node(canonical),
        )?;
        self.objects.prune_to(root)?;
        self.current_len = counters
            .logical_len_after
            .ok_or(CoreError::InvalidRecord("batch replacement length"))?;
        if counters.cdc_bytes_scanned != 0 {
            self.finalized_through = start
                .checked_add(counters.cdc_bytes_scanned)
                .ok_or(CoreError::LengthOverflow)?;
        }
        self.root = root;
        merge_counters(&mut self.counters, counters)
    }

    pub fn finish(mut self) -> CoreResult<(FileStateRoot, RopeCounters)> {
        let committed = self.objects.commit(self.root)?;
        self.counters.nodes_created = add(self.objects.sealed_node_puts, committed)?;
        self.counters.deferred_peak_bytes = self.objects.peak_charged_bytes as u64;
        self.counters.deferred_prunes = self.objects.prunes;
        self.counters.tree_level_before = Some(self.initial_level);
        self.counters.logical_len_before = Some(self.initial_len);
        self.counters.logical_len_after = Some(self.current_len);
        Ok((self.root, self.counters))
    }
}

struct DeferredFileObjects<'a, S> {
    store: &'a mut S,
    objects: BTreeMap<ObjectId, Vec<u8>>,
    charged_bytes: usize,
    peak_charged_bytes: usize,
    prunes: u64,
    sealed_node_puts: u64,
}

const DEFERRED_FILE_PRUNE_BYTES: usize = 4 * 1024 * 1024;
pub const FILE_MUTATION_BATCH_MAX_DEFERRED_BYTES: usize = 8 * 1024 * 1024 - 1;
const DEFERRED_OBJECT_CHARGE_BYTES: usize = 128;

impl<'a, S: ObjectStore> DeferredFileObjects<'a, S> {
    fn new(store: &'a mut S) -> Self {
        Self {
            store,
            objects: BTreeMap::new(),
            charged_bytes: 0,
            peak_charged_bytes: 0,
            prunes: 0,
            sealed_node_puts: 0,
        }
    }

    fn put_payload(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
        self.store.put(canonical)
    }

    fn put_sealed_node(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
        let id = self.store.put(canonical)?;
        self.sealed_node_puts = self
            .sealed_node_puts
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        Ok(id)
    }

    fn prune_to(&mut self, root: FileStateRoot) -> CoreResult<()> {
        if self.charged_bytes <= DEFERRED_FILE_PRUNE_BYTES {
            return Ok(());
        }
        let mut reachable = BTreeSet::new();
        self.collect_state(root, &mut reachable)?;
        self.objects.retain(|id, _| reachable.contains(id));
        self.charged_bytes = self.objects.values().try_fold(0_usize, |total, bytes| {
            total
                .checked_add(deferred_object_charge(bytes.len())?)
                .ok_or(CoreError::LengthOverflow)
        })?;
        self.prunes = self
            .prunes
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?;
        Ok(())
    }

    fn collect_state(
        &self,
        root: FileStateRoot,
        reachable: &mut BTreeSet<ObjectId>,
    ) -> CoreResult<()> {
        let Some(canonical) = self.objects.get(&root.0) else {
            return Ok(());
        };
        reachable.insert(root.0);
        let state = decode_file_state(canonical)?;
        self.collect_mapping(
            Summary {
                id: state.mapping_root,
                bytes: state.logical_len,
                extents: state.extent_count,
                level: state.tree_level,
            },
            true,
            reachable,
        )
    }

    fn collect_mapping(
        &self,
        expected: Summary,
        root: bool,
        reachable: &mut BTreeSet<ObjectId>,
    ) -> CoreResult<()> {
        let Some(canonical) = self.objects.get(&expected.id) else {
            return Ok(());
        };
        if !reachable.insert(expected.id) {
            return Ok(());
        }
        let node = decode_node_with_context(canonical, root)?;
        validate_summary(&node, expected)?;
        if let ExtentNodeV3::Branch {
            level, children, ..
        } = &node
        {
            for child in child_summaries(children, *level - 1) {
                self.collect_mapping(child, false, reachable)?;
            }
        }
        Ok(())
    }

    fn commit(&mut self, root: FileStateRoot) -> CoreResult<u64> {
        let Some(canonical) = self.objects.get(&root.0).cloned() else {
            return Ok(0);
        };
        let state = decode_file_state(&canonical)?;
        let summary = Summary {
            id: state.mapping_root,
            bytes: state.logical_len,
            extents: state.extent_count,
            level: state.tree_level,
        };
        let mut visited = BTreeSet::new();
        self.commit_mapping(summary, true, &mut visited)?;
        if self.store.put(&canonical)? != root.0 {
            return Err(CoreError::IdentityMismatch);
        }
        u64::try_from(visited.len()).map_err(|_| CoreError::LengthOverflow)
    }

    fn commit_mapping(
        &mut self,
        expected: Summary,
        root: bool,
        visited: &mut BTreeSet<ObjectId>,
    ) -> CoreResult<()> {
        if !visited.insert(expected.id) {
            return Ok(());
        }
        let Some(canonical) = self.objects.get(&expected.id).cloned() else {
            visited.remove(&expected.id);
            return Ok(());
        };
        let node = decode_node_with_context(&canonical, root)?;
        validate_summary(&node, expected)?;
        match &node {
            ExtentNodeV3::Leaf { .. } => {}
            ExtentNodeV3::Branch {
                level, children, ..
            } => {
                for child in child_summaries(children, *level - 1) {
                    self.commit_mapping(child, false, visited)?;
                }
            }
        }
        if self.store.put(&canonical)? != expected.id {
            return Err(CoreError::IdentityMismatch);
        }
        Ok(())
    }
}

impl<S: ObjectStore> ObjectStore for DeferredFileObjects<'_, S> {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        self.objects
            .get(&id)
            .cloned()
            .map_or_else(|| self.store.get(id), Ok)
    }

    fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
        let id = ObjectId::for_bytes(canonical);
        match self.objects.get(&id) {
            Some(prior) if prior != canonical => return Err(CoreError::IdentityMismatch),
            Some(_) => return Ok(id),
            None => {}
        }
        let charged = deferred_object_charge(canonical.len())?;
        let next = self
            .charged_bytes
            .checked_add(charged)
            .ok_or(CoreError::LengthOverflow)?;
        if next > FILE_MUTATION_BATCH_MAX_DEFERRED_BYTES {
            return Err(CoreError::ObjectLimitExceeded);
        }
        self.objects.insert(id, canonical.to_vec());
        self.charged_bytes = next;
        self.peak_charged_bytes = self.peak_charged_bytes.max(next);
        Ok(id)
    }

    fn with_authenticated_canonical<T, F>(&self, id: ObjectId, callback: F) -> CoreResult<T>
    where
        F: FnOnce(&[u8]) -> CoreResult<T>,
    {
        match self.objects.get(&id) {
            Some(bytes) if ObjectId::for_bytes(bytes) == id => callback(bytes),
            Some(_) => Err(CoreError::IdentityMismatch),
            None => self.store.with_authenticated_canonical(id, callback),
        }
    }
}

fn deferred_object_charge(canonical_bytes: usize) -> CoreResult<usize> {
    canonical_bytes
        .checked_add(DEFERRED_OBJECT_CHARGE_BYTES)
        .ok_or(CoreError::LengthOverflow)
}

struct DeferredNodes<'a, S> {
    store: &'a mut S,
    nodes: BTreeMap<ObjectId, Vec<u8>>,
}

impl<'a, S: ObjectStore> DeferredNodes<'a, S> {
    fn new(store: &'a mut S) -> Self {
        Self {
            store,
            nodes: BTreeMap::new(),
        }
    }

    fn with_nodes(store: &'a mut S, nodes: BTreeMap<ObjectId, Vec<u8>>) -> Self {
        Self { store, nodes }
    }

    fn into_nodes(self) -> BTreeMap<ObjectId, Vec<u8>> {
        self.nodes
    }

    fn flush_sealed_with<F>(&mut self, levels: &[Pending], put: &mut F) -> CoreResult<u64>
    where
        F: FnMut(&mut S, &[u8]) -> CoreResult<ObjectId>,
    {
        let mut protected = BTreeSet::new();
        for pending in levels {
            if let Pending::Children(children) = pending {
                if let Some(first) = children.first() {
                    self.protect_boundary(*first, &mut protected)?;
                }
                if let Some(last) = children.last() {
                    self.protect_boundary(*last, &mut protected)?;
                }
            }
        }
        let sealed = self
            .nodes
            .keys()
            .filter(|id| !protected.contains(id))
            .copied()
            .collect::<Vec<_>>();
        let mut flushed = BTreeSet::new();
        for id in sealed {
            self.flush_node_with(id, &mut flushed, put)?;
        }
        u64::try_from(flushed.len()).map_err(|_| CoreError::LengthOverflow)
    }

    fn protect_boundary(
        &self,
        expected: Summary,
        protected: &mut BTreeSet<ObjectId>,
    ) -> CoreResult<()> {
        if !protected.insert(expected.id) {
            return Ok(());
        }
        let Some(canonical) = self.nodes.get(&expected.id) else {
            return Ok(());
        };
        if let ExtentNodeV3::Branch {
            level, children, ..
        } = decode_node_with_context(canonical, true)?
        {
            let summaries = child_summaries(&children, level - 1);
            if let Some(first) = summaries.first() {
                self.protect_boundary(*first, protected)?;
            }
            if let Some(last) = summaries.last() {
                self.protect_boundary(*last, protected)?;
            }
        }
        Ok(())
    }

    fn flush_node_with<F>(
        &mut self,
        id: ObjectId,
        flushed: &mut BTreeSet<ObjectId>,
        put: &mut F,
    ) -> CoreResult<()>
    where
        F: FnMut(&mut S, &[u8]) -> CoreResult<ObjectId>,
    {
        if !flushed.insert(id) {
            return Ok(());
        }
        let Some(canonical) = self.nodes.get(&id).cloned() else {
            flushed.remove(&id);
            return Ok(());
        };
        if let ExtentNodeV3::Branch {
            level, children, ..
        } = decode_node_with_context(&canonical, true)?
        {
            for child in child_summaries(&children, level - 1) {
                self.flush_node_with(child.id, flushed, put)?;
            }
        }
        if put(self.store, &canonical)? != id {
            return Err(CoreError::IdentityMismatch);
        }
        self.nodes.remove(&id);
        Ok(())
    }

    fn commit(&mut self, root: Summary) -> CoreResult<u64> {
        let mut visited = BTreeSet::new();
        self.commit_node(root, true, &mut visited)?;
        u64::try_from(visited.len()).map_err(|_| CoreError::LengthOverflow)
    }

    fn commit_node(
        &mut self,
        expected: Summary,
        root: bool,
        visited: &mut BTreeSet<ObjectId>,
    ) -> CoreResult<()> {
        if !visited.insert(expected.id) {
            return Ok(());
        }
        let Some(canonical) = self.nodes.get(&expected.id).cloned() else {
            visited.remove(&expected.id);
            return Ok(());
        };
        let node = decode_node_with_context(&canonical, root)?;
        if node.level() != expected.level
            || node.logical_len() != expected.bytes
            || node.extent_count() != expected.extents
        {
            return Err(CoreError::InvalidRecord("deferred extent summary"));
        }
        if let ExtentNodeV3::Branch {
            level, children, ..
        } = node
        {
            for child in child_summaries(&children, level - 1) {
                self.commit_node(child, false, visited)?;
            }
        }
        if self.store.put(&canonical)? != expected.id {
            return Err(CoreError::IdentityMismatch);
        }
        Ok(())
    }
}

impl<S: ObjectStore> ObjectStore for DeferredNodes<'_, S> {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        self.nodes
            .get(&id)
            .cloned()
            .map_or_else(|| self.store.get(id), Ok)
    }

    fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
        let id = ObjectId::for_bytes(canonical);
        if self
            .nodes
            .insert(id, canonical.to_vec())
            .is_some_and(|prior| prior != canonical)
        {
            return Err(CoreError::IdentityMismatch);
        }
        Ok(id)
    }

    fn with_authenticated_canonical<T, F>(&self, id: ObjectId, callback: F) -> CoreResult<T>
    where
        F: FnOnce(&[u8]) -> CoreResult<T>,
    {
        match self.nodes.get(&id) {
            Some(bytes) if ObjectId::for_bytes(bytes) == id => callback(bytes),
            Some(_) => Err(CoreError::IdentityMismatch),
            None => self.store.with_authenticated_canonical(id, callback),
        }
    }
}

fn split_optional<S: ObjectStore>(
    store: &mut S,
    root: Option<Summary>,
    offset: u64,
    counters: &mut RopeCounters,
) -> CoreResult<(Option<Summary>, Option<Summary>)> {
    match root {
        Some(root) => split(store, root, offset, true, counters),
        None if offset == 0 => Ok((None, None)),
        None => Err(CoreError::InvalidRange {
            start: offset,
            end: offset,
            length: 0,
        }),
    }
}

fn split<S: ObjectStore>(
    store: &mut S,
    root: Summary,
    offset: u64,
    root_context: bool,
    counters: &mut RopeCounters,
) -> CoreResult<(Option<Summary>, Option<Summary>)> {
    if offset > root.bytes {
        return Err(CoreError::InvalidRange {
            start: offset,
            end: offset,
            length: root.bytes,
        });
    }
    if offset == 0 {
        return Ok((None, Some(root)));
    }
    if offset == root.bytes {
        return Ok((Some(root), None));
    }
    let node = load_node(store, root, root_context, counters)?;
    match node {
        ExtentNodeV3::Leaf { extents, .. } => {
            let mut left = Vec::new();
            let mut right = Vec::new();
            let mut logical = 0_u64;
            for extent in extents {
                let end = add(logical, u64::from(extent.logical_length))?;
                if end <= offset {
                    left.push(extent);
                } else if logical >= offset {
                    right.push(extent);
                } else {
                    let left_len =
                        u32::try_from(offset - logical).map_err(|_| CoreError::LengthOverflow)?;
                    left.push(ExtentSliceV3::new(
                        extent.payload_object_id,
                        extent.source_offset,
                        left_len,
                    )?);
                    right.push(ExtentSliceV3::new(
                        extent.payload_object_id,
                        extent
                            .source_offset
                            .checked_add(left_len)
                            .ok_or(CoreError::LengthOverflow)?,
                        extent.logical_length - left_len,
                    )?);
                }
                logical = end;
            }
            Ok((
                Some(emit_leaf(store, left, counters)?),
                Some(emit_leaf(store, right, counters)?),
            ))
        }
        ExtentNodeV3::Branch {
            level, children, ..
        } => {
            let index = children.partition_point(|child| child.cumulative_logical_end < offset);
            let before_bytes = if index == 0 {
                0
            } else {
                children[index - 1].cumulative_logical_end
            };
            let summaries = child_summaries(&children, level - 1);
            let child_summary = summaries[index];
            let (child_left, child_right) =
                split(store, child_summary, offset - before_bytes, false, counters)?;
            let prefix = root_from_children(store, summaries[..index].to_vec(), counters)?;
            let suffix = root_from_children(store, summaries[index + 1..].to_vec(), counters)?;
            Ok((
                concat_optional(store, prefix, child_left, counters)?,
                concat_optional(store, child_right, suffix, counters)?,
            ))
        }
    }
}

fn concat_optional<S: ObjectStore>(
    store: &mut S,
    left: Option<Summary>,
    right: Option<Summary>,
    counters: &mut RopeCounters,
) -> CoreResult<Option<Summary>> {
    match (left, right) {
        (None, value) | (value, None) => Ok(value),
        (Some(left), Some(right)) => concat(store, left, right, counters).map(Some),
    }
}

fn concat<S: ObjectStore>(
    store: &mut S,
    left: Summary,
    right: Summary,
    counters: &mut RopeCounters,
) -> CoreResult<Summary> {
    concat_inner(store, left, right, counters, 0)
}

fn concat_inner<S: ObjectStore>(
    store: &mut S,
    left: Summary,
    right: Summary,
    counters: &mut RopeCounters,
    depth: u8,
) -> CoreResult<Summary> {
    if depth > MAX_LEVEL {
        return Err(CoreError::MappingDepthExceeded);
    }
    if left.level == right.level {
        let left_node = load_node(store, left, true, counters)?;
        let right_node = load_node(store, right, true, counters)?;
        return match (left_node, right_node) {
            (ExtentNodeV3::Leaf { mut extents, .. }, ExtentNodeV3::Leaf { extents: right, .. }) => {
                extents.extend(right);
                coalesce(&mut extents)?;
                root_from_extents(store, extents, counters)
            }
            (
                ExtentNodeV3::Branch {
                    level, children, ..
                },
                ExtentNodeV3::Branch {
                    children: right, ..
                },
            ) => {
                let mut summaries = child_summaries(&children, level - 1);
                summaries.extend(child_summaries(&right, level - 1));
                root_from_children(store, summaries, counters)?
                    .ok_or(CoreError::InvalidRecord("empty branch concat"))
            }
            _ => Err(CoreError::WrongLogicalRole),
        };
    }
    if left.level > right.level {
        let ExtentNodeV3::Branch {
            level, children, ..
        } = load_node(store, left, true, counters)?
        else {
            return Err(CoreError::WrongLogicalRole);
        };
        let summaries = child_summaries(&children, level - 1);
        let (last, prefix) = summaries
            .split_last()
            .ok_or(CoreError::InvalidRecord("empty branch"))?;
        load_node(store, *last, false, counters)?;
        let prefix = root_from_children(store, prefix.to_vec(), counters)?;
        let boundary = concat_inner(store, *last, right, counters, depth + 1)?;
        return match prefix {
            None => Ok(boundary),
            Some(prefix) if prefix.level == boundary.level => {
                concat_inner(store, prefix, boundary, counters, depth + 1)
            }
            Some(prefix) if prefix.level.checked_add(1) == Some(boundary.level) => {
                let ExtentNodeV3::Branch {
                    level, children, ..
                } = load_node(store, boundary, true, counters)?
                else {
                    return Err(CoreError::WrongLogicalRole);
                };
                let mut children = child_summaries(&children, level - 1);
                children.insert(0, prefix);
                root_from_children(store, children, counters)?
                    .ok_or(CoreError::InvalidRecord("empty concat"))
            }
            Some(prefix) if boundary.level.checked_add(1) == Some(prefix.level) => {
                let ExtentNodeV3::Branch {
                    level, children, ..
                } = load_node(store, prefix, true, counters)?
                else {
                    return Err(CoreError::WrongLogicalRole);
                };
                let mut children = child_summaries(&children, level - 1);
                children.push(boundary);
                root_from_children(store, children, counters)?
                    .ok_or(CoreError::InvalidRecord("empty concat"))
            }
            Some(_) => Err(CoreError::InvalidRecord("left concat levels")),
        };
    }
    let ExtentNodeV3::Branch {
        level, children, ..
    } = load_node(store, right, true, counters)?
    else {
        return Err(CoreError::WrongLogicalRole);
    };
    let summaries = child_summaries(&children, level - 1);
    let (first, suffix) = summaries
        .split_first()
        .ok_or(CoreError::InvalidRecord("empty branch"))?;
    load_node(store, *first, false, counters)?;
    let boundary = concat_inner(store, left, *first, counters, depth + 1)?;
    let suffix = root_from_children(store, suffix.to_vec(), counters)?;
    match suffix {
        None => Ok(boundary),
        Some(suffix) if suffix.level == boundary.level => {
            concat_inner(store, boundary, suffix, counters, depth + 1)
        }
        Some(suffix) if suffix.level.checked_add(1) == Some(boundary.level) => {
            let ExtentNodeV3::Branch {
                level, children, ..
            } = load_node(store, boundary, true, counters)?
            else {
                return Err(CoreError::WrongLogicalRole);
            };
            let mut children = child_summaries(&children, level - 1);
            children.push(suffix);
            root_from_children(store, children, counters)?
                .ok_or(CoreError::InvalidRecord("empty concat"))
        }
        Some(suffix) if boundary.level.checked_add(1) == Some(suffix.level) => {
            let ExtentNodeV3::Branch {
                level, children, ..
            } = load_node(store, suffix, true, counters)?
            else {
                return Err(CoreError::WrongLogicalRole);
            };
            let mut children = child_summaries(&children, level - 1);
            children.insert(0, boundary);
            root_from_children(store, children, counters)?
                .ok_or(CoreError::InvalidRecord("empty concat"))
        }
        Some(_) => Err(CoreError::InvalidRecord("right concat levels")),
    }
}

fn root_from_extents<S: ObjectStore>(
    store: &mut S,
    extents: Vec<ExtentSliceV3>,
    counters: &mut RopeCounters,
) -> CoreResult<Summary> {
    if extents.len() <= MAX_ENTRIES {
        return emit_leaf(store, extents, counters);
    }
    let split = extents.len() / 2;
    let left = emit_leaf(store, extents[..split].to_vec(), counters)?;
    let right = emit_leaf(store, extents[split..].to_vec(), counters)?;
    root_from_children(store, vec![left, right], counters)?
        .ok_or(CoreError::InvalidRecord("empty extent root"))
}

fn root_from_children<S: ObjectStore>(
    store: &mut S,
    children: Vec<Summary>,
    counters: &mut RopeCounters,
) -> CoreResult<Option<Summary>> {
    if children.is_empty() {
        return Ok(None);
    }
    if children.len() == 1 {
        return Ok(Some(children[0]));
    }
    if children.len() <= MAX_ENTRIES {
        return emit_branch(store, children, counters).map(Some);
    }
    let split = children.len() / 2;
    let left = emit_branch(store, children[..split].to_vec(), counters)?;
    let right = emit_branch(store, children[split..].to_vec(), counters)?;
    emit_branch(store, vec![left, right], counters).map(Some)
}

fn emit_leaf<S: ObjectStore>(
    store: &mut S,
    extents: Vec<ExtentSliceV3>,
    counters: &mut RopeCounters,
) -> CoreResult<Summary> {
    let bytes = extents.iter().try_fold(0_u64, |sum, extent| {
        add(sum, u64::from(extent.logical_length))
    })?;
    let node = ExtentNodeV3::Leaf {
        subtree_logical_bytes: bytes,
        extents,
    };
    emit_node(store, node, counters)
}

fn emit_branch<S: ObjectStore>(
    store: &mut S,
    children: Vec<Summary>,
    counters: &mut RopeCounters,
) -> CoreResult<Summary> {
    let level = children[0]
        .level
        .checked_add(1)
        .ok_or(CoreError::MappingDepthExceeded)?;
    if children.iter().any(|child| child.level + 1 != level) {
        return Err(CoreError::InvalidRecord("mixed branch levels"));
    }
    let mut bytes = 0;
    let mut extents = 0;
    let descriptors = children
        .into_iter()
        .map(|child| {
            bytes = add(bytes, child.bytes)?;
            extents = add(extents, child.extents)?;
            Ok(ChildDescriptorV3 {
                cumulative_logical_end: bytes,
                cumulative_extent_end: extents,
                child_object_id: child.id,
            })
        })
        .collect::<CoreResult<Vec<_>>>()?;
    emit_node(
        store,
        ExtentNodeV3::Branch {
            level,
            subtree_logical_bytes: bytes,
            subtree_extent_count: extents,
            children: descriptors,
        },
        counters,
    )
}

fn emit_node<S: ObjectStore>(
    store: &mut S,
    node: ExtentNodeV3,
    counters: &mut RopeCounters,
) -> CoreResult<Summary> {
    let canonical = encode_node(&node)?;
    let id = store.put(&canonical)?;
    counters.nodes_created = add(counters.nodes_created, 1)?;
    Ok(Summary {
        id,
        bytes: node.logical_len(),
        extents: node.extent_count(),
        level: node.level(),
    })
}

fn load_node<S: ObjectRead>(
    store: &S,
    summary: Summary,
    root: bool,
    counters: &mut RopeCounters,
) -> CoreResult<ExtentNodeV3> {
    counters.nodes_read = add(counters.nodes_read, 1)?;
    let node = store.with_authenticated_canonical(summary.id, |canonical| {
        decode_node_with_context(canonical, root)
    })?;
    if node.level() != summary.level
        || node.logical_len() != summary.bytes
        || node.extent_count() != summary.extents
    {
        return Err(CoreError::InvalidRecord("extent summary"));
    }
    node.validate(root)?;
    Ok(node)
}

fn load_node_cached<S: ObjectRead>(
    store: &S,
    summary: Summary,
    root: bool,
    counters: &mut RopeCounters,
    cache: &mut Option<(ObjectId, ExtentNodeV3)>,
) -> CoreResult<ExtentNodeV3> {
    let node = match cache {
        Some((id, node)) if *id == summary.id => node.clone(),
        _ => {
            let node = load_node(store, summary, root, counters)?;
            *cache = Some((summary.id, node.clone()));
            node
        }
    };
    if node.level() != summary.level
        || node.logical_len() != summary.bytes
        || node.extent_count() != summary.extents
    {
        return Err(CoreError::InvalidRecord("extent summary"));
    }
    Ok(node)
}

#[derive(Clone, Copy)]
enum SpanKind {
    Node { summary: Summary, root: bool },
    Extent(ExtentSliceV3),
}

#[derive(Clone, Copy)]
struct Span {
    kind: SpanKind,
    offset: u64,
    len: u64,
}

impl Span {
    fn node(summary: Summary) -> Self {
        Self {
            kind: SpanKind::Node {
                summary,
                root: true,
            },
            offset: 0,
            len: summary.bytes,
        }
    }

    fn slice(self, offset: u64, len: u64) -> CoreResult<Self> {
        if offset.checked_add(len).is_none_or(|end| end > self.len) {
            return Err(CoreError::LengthOverflow);
        }
        Ok(Self {
            kind: self.kind,
            offset: self
                .offset
                .checked_add(offset)
                .ok_or(CoreError::LengthOverflow)?,
            len,
        })
    }
}

struct ChangedRanges<'a, F> {
    start: Option<u64>,
    visitor: &'a mut F,
}

impl<F: FnMut(Range<u64>) -> CoreResult<()>> ChangedRanges<'_, F> {
    fn record(&mut self, start: u64, len: u64, changed: bool) -> CoreResult<()> {
        let end = start.checked_add(len).ok_or(CoreError::LengthOverflow)?;
        if changed {
            self.start.get_or_insert(start);
        } else if let Some(changed_start) = self.start.take() {
            (self.visitor)(changed_start..start)?;
        }
        if end < start {
            return Err(CoreError::LengthOverflow);
        }
        Ok(())
    }

    fn finish(&mut self, end: u64) -> CoreResult<()> {
        if let Some(start) = self.start.take() {
            (self.visitor)(start..end)?;
        }
        Ok(())
    }
}

fn same_summary(left: Summary, right: Summary) -> bool {
    left.id == right.id
        && left.bytes == right.bytes
        && left.extents == right.extents
        && left.level == right.level
}

#[allow(clippy::too_many_arguments)]
fn diff_span<S: ObjectRead, F: FnMut(Range<u64>) -> CoreResult<()>>(
    store: &S,
    old: Span,
    new: Span,
    logical: u64,
    counters: &mut RopeCounters,
    old_ancestors: &mut Vec<ObjectId>,
    new_ancestors: &mut Vec<ObjectId>,
    old_cache: &mut Option<(ObjectId, ExtentNodeV3)>,
    new_cache: &mut Option<(ObjectId, ExtentNodeV3)>,
    emitter: &mut ChangedRanges<'_, F>,
) -> CoreResult<()> {
    if old.len != new.len {
        return Err(CoreError::LengthMismatch {
            expected: old.len,
            actual: new.len,
        });
    }
    match (old.kind, new.kind) {
        (
            SpanKind::Node {
                summary: old_summary,
                ..
            },
            SpanKind::Node {
                summary: new_summary,
                ..
            },
        ) if old_summary.id == new_summary.id && old.offset == new.offset => {
            if !same_summary(old_summary, new_summary) {
                return Err(CoreError::InvalidRecord("extent summary"));
            }
            emitter.record(logical, old.len, false)
        }
        (
            SpanKind::Node {
                summary: old_summary,
                root: old_root,
            },
            SpanKind::Node {
                summary: new_summary,
                root: new_root,
            },
        ) => {
            if old_ancestors.contains(&old_summary.id) || new_ancestors.contains(&new_summary.id) {
                return Err(CoreError::MappingCycle);
            }
            old_ancestors.push(old_summary.id);
            new_ancestors.push(new_summary.id);
            let old_spans = expand_span(store, old, old_root, counters, old_cache)?;
            let new_spans = expand_span(store, new, new_root, counters, new_cache)?;
            merge_spans(
                store,
                &old_spans,
                &new_spans,
                logical,
                counters,
                old_ancestors,
                new_ancestors,
                old_cache,
                new_cache,
                emitter,
            )?;
            old_ancestors.pop();
            new_ancestors.pop();
            Ok(())
        }
        (SpanKind::Extent(old_extent), SpanKind::Extent(new_extent)) => {
            let old_source = u64::from(old_extent.source_offset)
                .checked_add(old.offset)
                .ok_or(CoreError::LengthOverflow)?;
            let new_source = u64::from(new_extent.source_offset)
                .checked_add(new.offset)
                .ok_or(CoreError::LengthOverflow)?;
            emitter.record(
                logical,
                old.len,
                old_extent.payload_object_id != new_extent.payload_object_id
                    || old_source != new_source,
            )
        }
        (SpanKind::Node { summary, root }, _) => {
            if old_ancestors.contains(&summary.id) {
                return Err(CoreError::MappingCycle);
            }
            old_ancestors.push(summary.id);
            let spans = expand_span(store, old, root, counters, old_cache)?;
            merge_spans(
                store,
                &spans,
                &[new],
                logical,
                counters,
                old_ancestors,
                new_ancestors,
                old_cache,
                new_cache,
                emitter,
            )?;
            old_ancestors.pop();
            Ok(())
        }
        (_, SpanKind::Node { summary, root }) => {
            if new_ancestors.contains(&summary.id) {
                return Err(CoreError::MappingCycle);
            }
            new_ancestors.push(summary.id);
            let spans = expand_span(store, new, root, counters, new_cache)?;
            merge_spans(
                store,
                &[old],
                &spans,
                logical,
                counters,
                old_ancestors,
                new_ancestors,
                old_cache,
                new_cache,
                emitter,
            )?;
            new_ancestors.pop();
            Ok(())
        }
    }
}

fn expand_span<S: ObjectRead>(
    store: &S,
    span: Span,
    root: bool,
    counters: &mut RopeCounters,
    cache: &mut Option<(ObjectId, ExtentNodeV3)>,
) -> CoreResult<Vec<Span>> {
    let SpanKind::Node { summary, .. } = span.kind else {
        return Ok(vec![span]);
    };
    let node = load_node_cached(store, summary, root, counters, cache)?;
    let mut output = Vec::with_capacity(node.entry_count());
    let wanted_end = span
        .offset
        .checked_add(span.len)
        .ok_or(CoreError::LengthOverflow)?;
    match node {
        ExtentNodeV3::Leaf { extents, .. } => {
            let mut start = 0_u64;
            for extent in extents {
                let end = start
                    .checked_add(u64::from(extent.logical_length))
                    .ok_or(CoreError::LengthOverflow)?;
                push_overlap(
                    &mut output,
                    SpanKind::Extent(extent),
                    start,
                    end,
                    span.offset,
                    wanted_end,
                )?;
                start = end;
            }
        }
        ExtentNodeV3::Branch {
            level, children, ..
        } => {
            let mut start = 0_u64;
            for summary in child_summaries(&children, level - 1) {
                let end = start
                    .checked_add(summary.bytes)
                    .ok_or(CoreError::LengthOverflow)?;
                push_overlap(
                    &mut output,
                    SpanKind::Node {
                        summary,
                        root: false,
                    },
                    start,
                    end,
                    span.offset,
                    wanted_end,
                )?;
                start = end;
            }
        }
    }
    if output.iter().try_fold(0_u64, |sum, item| {
        sum.checked_add(item.len).ok_or(CoreError::LengthOverflow)
    })? != span.len
    {
        return Err(CoreError::InvalidRecord("extent span"));
    }
    Ok(output)
}

fn push_overlap(
    output: &mut Vec<Span>,
    kind: SpanKind,
    start: u64,
    end: u64,
    wanted_start: u64,
    wanted_end: u64,
) -> CoreResult<()> {
    let overlap_start = start.max(wanted_start);
    let overlap_end = end.min(wanted_end);
    if overlap_start < overlap_end {
        output.push(Span {
            kind,
            offset: overlap_start - start,
            len: overlap_end - overlap_start,
        });
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn merge_spans<S: ObjectRead, F: FnMut(Range<u64>) -> CoreResult<()>>(
    store: &S,
    old: &[Span],
    new: &[Span],
    mut logical: u64,
    counters: &mut RopeCounters,
    old_ancestors: &mut Vec<ObjectId>,
    new_ancestors: &mut Vec<ObjectId>,
    old_cache: &mut Option<(ObjectId, ExtentNodeV3)>,
    new_cache: &mut Option<(ObjectId, ExtentNodeV3)>,
    emitter: &mut ChangedRanges<'_, F>,
) -> CoreResult<()> {
    let (mut old_index, mut new_index) = (0_usize, 0_usize);
    let (mut old_used, mut new_used) = (0_u64, 0_u64);
    while old_index < old.len() && new_index < new.len() {
        let count = (old[old_index].len - old_used).min(new[new_index].len - new_used);
        diff_span(
            store,
            old[old_index].slice(old_used, count)?,
            new[new_index].slice(new_used, count)?,
            logical,
            counters,
            old_ancestors,
            new_ancestors,
            old_cache,
            new_cache,
            emitter,
        )?;
        logical = logical
            .checked_add(count)
            .ok_or(CoreError::LengthOverflow)?;
        old_used += count;
        new_used += count;
        if old_used == old[old_index].len {
            old_index += 1;
            old_used = 0;
        }
        if new_used == new[new_index].len {
            new_index += 1;
            new_used = 0;
        }
    }
    if old_index != old.len() || new_index != new.len() {
        return Err(CoreError::InvalidRecord("extent span"));
    }
    Ok(())
}

fn validate_node<S: ObjectRead>(
    store: &S,
    expected: Summary,
    root: bool,
    counters: &mut RopeCounters,
    ancestors: &mut Vec<ObjectId>,
) -> CoreResult<()> {
    if ancestors.contains(&expected.id) {
        return Err(CoreError::MappingCycle);
    }
    ancestors.push(expected.id);
    match load_node(store, expected, root, counters)? {
        ExtentNodeV3::Leaf { extents, .. } => {
            for batch in extents.chunks(64) {
                let ids = batch
                    .iter()
                    .map(|extent| extent.payload_object_id)
                    .collect::<Vec<_>>();
                let mut index = 0_usize;
                store.get_authenticated_payload_lengths_batch(&ids, |id, payload_length| {
                    let extent = batch
                        .get(index)
                        .ok_or(CoreError::InvalidRecord("payload batch cardinality"))?;
                    if id != extent.payload_object_id {
                        return Err(CoreError::IdentityMismatch);
                    }
                    index += 1;
                    if payload_length as usize > crate::cdc::MAXIMUM_CHUNK_BYTES
                        || extent
                            .source_offset
                            .checked_add(extent.logical_length)
                            .is_none_or(|end| end > payload_length)
                    {
                        return Err(CoreError::ChunkLengthMismatch);
                    }
                    Ok(())
                })?;
                if index != batch.len() {
                    return Err(CoreError::InvalidRecord("payload batch cardinality"));
                }
            }
        }
        ExtentNodeV3::Branch {
            level, children, ..
        } => {
            for child in child_summaries(&children, level - 1) {
                validate_node(store, child, false, counters, ancestors)?;
            }
        }
    }
    ancestors.pop();
    Ok(())
}

fn visit_extent_node<S: ObjectRead>(
    store: &S,
    expected: Summary,
    root: bool,
    counters: &mut RopeCounters,
    ancestors: &mut Vec<ObjectId>,
    visitor: &mut impl FnMut(&[ExtentSliceV3]) -> CoreResult<()>,
) -> CoreResult<()> {
    if ancestors.contains(&expected.id) {
        return Err(CoreError::MappingCycle);
    }
    ancestors.push(expected.id);
    match load_node(store, expected, root, counters)? {
        ExtentNodeV3::Leaf { extents, .. } => visitor(&extents)?,
        ExtentNodeV3::Branch {
            level, children, ..
        } => {
            for child in child_summaries(&children, level - 1) {
                visit_extent_node(store, child, false, counters, ancestors, visitor)?;
            }
        }
    }
    ancestors.pop();
    Ok(())
}

fn child_summaries(children: &[ChildDescriptorV3], level: u8) -> Vec<Summary> {
    let mut prior_bytes = 0;
    let mut prior_extents = 0;
    children
        .iter()
        .map(|child| {
            let summary = Summary {
                id: child.child_object_id,
                bytes: child.cumulative_logical_end - prior_bytes,
                extents: child.cumulative_extent_end - prior_extents,
                level,
            };
            prior_bytes = child.cumulative_logical_end;
            prior_extents = child.cumulative_extent_end;
            summary
        })
        .collect()
}

fn coalesce(extents: &mut Vec<ExtentSliceV3>) -> CoreResult<()> {
    let mut index = 1;
    while index < extents.len() {
        let previous = extents[index - 1];
        let current = extents[index];
        if previous.payload_object_id == current.payload_object_id
            && previous.source_offset.checked_add(previous.logical_length)
                == Some(current.source_offset)
        {
            extents[index - 1].logical_length = previous
                .logical_length
                .checked_add(current.logical_length)
                .ok_or(CoreError::LengthOverflow)?;
            extents.remove(index);
        } else {
            index += 1;
        }
    }
    Ok(())
}

fn merge_counters(target: &mut RopeCounters, source: RopeCounters) -> CoreResult<()> {
    target.payload_bytes_read = add(target.payload_bytes_read, source.payload_bytes_read)?;
    target.payload_bytes_written = add(target.payload_bytes_written, source.payload_bytes_written)?;
    target.cdc_bytes_scanned = add(target.cdc_bytes_scanned, source.cdc_bytes_scanned)?;
    target.chunks_created = add(target.chunks_created, source.chunks_created)?;
    target.nodes_read = add(target.nodes_read, source.nodes_read)?;
    target.nodes_created = add(target.nodes_created, source.nodes_created)?;
    target.deferred_peak_bytes = target.deferred_peak_bytes.max(source.deferred_peak_bytes);
    target.deferred_prunes = add(target.deferred_prunes, source.deferred_prunes)?;
    target.tree_level_before = match (target.tree_level_before, source.tree_level_before) {
        (None, value) | (value, None) => value,
        (Some(left), Some(right)) if left == right => Some(left),
        (Some(_), Some(_)) => None,
    };
    target.logical_len_before =
        merge_optional_equal(target.logical_len_before, source.logical_len_before);
    target.logical_len_after =
        merge_optional_equal(target.logical_len_after, source.logical_len_after);
    Ok(())
}

fn merge_optional_equal<T: Copy + Eq>(left: Option<T>, right: Option<T>) -> Option<T> {
    match (left, right) {
        (None, value) | (value, None) => value,
        (Some(left), Some(right)) if left == right => Some(left),
        (Some(_), Some(_)) => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn read_node<S: ObjectRead, W: Write>(
    store: &S,
    expected: Summary,
    root: bool,
    base: u64,
    range: &Range<u64>,
    sink: &mut W,
    counters: &mut RopeCounters,
    ancestors: &mut Vec<ObjectId>,
    selected: &mut Vec<(ExtentSliceV3, u64, u64)>,
) -> CoreResult<()> {
    if ancestors.contains(&expected.id) {
        return Err(CoreError::MappingCycle);
    }
    ancestors.push(expected.id);
    counters.nodes_read = add(counters.nodes_read, 1)?;
    let node = store.with_authenticated_canonical(expected.id, |canonical| {
        decode_node_with_context(canonical, root)
    })?;
    validate_summary(&node, expected)?;
    read_decoded_node(
        store, base, range, sink, counters, ancestors, &node, selected,
    )?;
    ancestors.pop();
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn read_decoded_node<S: ObjectRead, W: Write>(
    store: &S,
    base: u64,
    range: &Range<u64>,
    sink: &mut W,
    counters: &mut RopeCounters,
    ancestors: &mut Vec<ObjectId>,
    node: &ExtentNodeV3,
    selected: &mut Vec<(ExtentSliceV3, u64, u64)>,
) -> CoreResult<()> {
    match node {
        ExtentNodeV3::Leaf { extents, .. } => {
            let mut logical = base;
            for extent in extents {
                let end = logical
                    .checked_add(u64::from(extent.logical_length))
                    .ok_or(CoreError::LengthOverflow)?;
                if end > range.start && logical < range.end {
                    selected.push((
                        *extent,
                        range.start.max(logical) - logical,
                        range.end.min(end) - logical,
                    ));
                    if selected.len() == 64 {
                        flush_read_batch(store, sink, counters, selected)?;
                    }
                }
                if logical >= range.end {
                    break;
                }
                logical = end;
            }
        }
        ExtentNodeV3::Branch {
            level, children, ..
        } => {
            let first = children.partition_point(|child| {
                base.checked_add(child.cumulative_logical_end)
                    .is_some_and(|end| end <= range.start)
            });
            let mut prior = if first == 0 {
                0
            } else {
                children[first - 1].cumulative_logical_end
            };
            let summaries = child_summaries(children, *level - 1);
            for (child, summary) in children.iter().copied().zip(summaries).skip(first) {
                let child_start = base.checked_add(prior).ok_or(CoreError::LengthOverflow)?;
                if child_start >= range.end {
                    break;
                }
                read_node(
                    store,
                    summary,
                    false,
                    child_start,
                    range,
                    sink,
                    counters,
                    ancestors,
                    selected,
                )?;
                prior = child.cumulative_logical_end;
            }
        }
    }
    Ok(())
}

fn flush_read_batch<S: ObjectRead, W: Write>(
    store: &S,
    sink: &mut W,
    counters: &mut RopeCounters,
    selected: &mut Vec<(ExtentSliceV3, u64, u64)>,
) -> CoreResult<()> {
    if selected.is_empty() {
        return Ok(());
    }
    let ids = selected
        .iter()
        .map(|(extent, _, _)| extent.payload_object_id)
        .collect::<Vec<_>>();
    let mut index = 0_usize;
    store.get_authenticated_batch(&ids, |id, payload| {
        let (extent, overlap_start, overlap_end) = selected
            .get(index)
            .copied()
            .ok_or(CoreError::InvalidRecord("payload batch cardinality"))?;
        if id != extent.payload_object_id {
            return Err(CoreError::IdentityMismatch);
        }
        index += 1;
        if payload.len() > crate::cdc::MAXIMUM_CHUNK_BYTES {
            return Err(CoreError::ChunkLengthMismatch);
        }
        let source_end = extent
            .source_offset
            .checked_add(extent.logical_length)
            .ok_or(CoreError::LengthOverflow)? as usize;
        if source_end > payload.len() {
            return Err(CoreError::ChunkLengthMismatch);
        }
        let start = extent.source_offset as usize + overlap_start as usize;
        let stop = extent.source_offset as usize + overlap_end as usize;
        sink.write_all(&payload[start..stop])
            .map_err(|_| CoreError::Io)?;
        counters.payload_bytes_read = add(counters.payload_bytes_read, (stop - start) as u64)?;
        Ok(())
    })?;
    if index != selected.len() {
        return Err(CoreError::InvalidRecord("payload batch cardinality"));
    }
    selected.clear();
    Ok(())
}

fn validate_summary(node: &ExtentNodeV3, expected: Summary) -> CoreResult<()> {
    if node.level() != expected.level
        || node.logical_len() != expected.bytes
        || node.extent_count() != expected.extents
    {
        Err(CoreError::InvalidRecord("extent summary"))
    } else {
        Ok(())
    }
}

fn flush_streaming<S: ObjectStore>(
    store: &mut S,
    levels: &mut Vec<Pending>,
    level: usize,
    counters: &mut RopeCounters,
) -> CoreResult<()> {
    let len = match &levels[level] {
        Pending::Extents(v) => v.len(),
        Pending::Children(v) => v.len(),
    };
    if len <= STREAM_FLUSH_AT {
        return Ok(());
    }
    let summary = emit_prefix(
        store,
        &mut levels[level],
        MAX_ENTRIES,
        level as u8,
        counters,
    )?;
    push_summary(levels, level + 1, summary)?;
    flush_streaming(store, levels, level + 1, counters)
}

fn finish<S: ObjectStore>(
    store: &mut S,
    levels: &mut Vec<Pending>,
    counters: &mut RopeCounters,
) -> CoreResult<Summary> {
    let mut level = 0;
    loop {
        let higher_nonempty = levels
            .iter()
            .skip(level + 1)
            .any(|pending| pending_len(pending) != 0);
        let len = pending_len(&levels[level]);
        if !higher_nonempty && len <= MAX_ENTRIES {
            if level > 0 && len == 1 {
                if let Pending::Children(children) = &levels[level] {
                    return Ok(children[0]);
                }
            }
            return emit_prefix(store, &mut levels[level], len, level as u8, counters);
        }
        if len != 0 {
            let first = if len > MAX_ENTRIES { len / 2 } else { len };
            let summary = emit_prefix(store, &mut levels[level], first, level as u8, counters)?;
            push_summary(levels, level + 1, summary)?;
            continue;
        }
        level += 1;
        if level >= levels.len() {
            return Err(CoreError::InvalidRecord("empty rope builder"));
        }
    }
}

fn emit_prefix<S: ObjectStore>(
    store: &mut S,
    pending: &mut Pending,
    count: usize,
    level: u8,
    counters: &mut RopeCounters,
) -> CoreResult<Summary> {
    let node = match pending {
        Pending::Extents(entries) => {
            let entries: Vec<_> = entries.drain(..count).collect();
            let bytes = entries.iter().try_fold(0_u64, |sum, entry| {
                add(sum, u64::from(entry.logical_length))
            })?;
            ExtentNodeV3::Leaf {
                subtree_logical_bytes: bytes,
                extents: entries,
            }
        }
        Pending::Children(entries) => {
            let entries: Vec<_> = entries.drain(..count).collect();
            let mut bytes = 0_u64;
            let mut extents = 0_u64;
            let children = entries
                .iter()
                .map(|entry| {
                    bytes = add(bytes, entry.bytes)?;
                    extents = add(extents, entry.extents)?;
                    Ok(ChildDescriptorV3 {
                        cumulative_logical_end: bytes,
                        cumulative_extent_end: extents,
                        child_object_id: entry.id,
                    })
                })
                .collect::<CoreResult<Vec<_>>>()?;
            ExtentNodeV3::Branch {
                level,
                subtree_logical_bytes: bytes,
                subtree_extent_count: extents,
                children,
            }
        }
    };
    let canonical = encode_node(&node)?;
    let id = store.put(&canonical)?;
    counters.nodes_created = add(counters.nodes_created, 1)?;
    Ok(Summary {
        id,
        bytes: node.logical_len(),
        extents: node.extent_count(),
        level,
    })
}

fn push_summary(levels: &mut Vec<Pending>, level: usize, summary: Summary) -> CoreResult<()> {
    if summary.level as usize + 1 != level {
        return Err(CoreError::InvalidRecord("rope builder level"));
    }
    while levels.len() <= level {
        levels.push(Pending::Children(Vec::with_capacity(STREAM_FLUSH_AT + 1)));
    }
    match &mut levels[level] {
        Pending::Children(children) => children.push(summary),
        Pending::Extents(_) => return Err(CoreError::InvalidRecord("rope builder role")),
    }
    Ok(())
}

fn pending_len(pending: &Pending) -> usize {
    match pending {
        Pending::Extents(v) => v.len(),
        Pending::Children(v) => v.len(),
    }
}

fn add(left: u64, right: u64) -> CoreResult<u64> {
    left.checked_add(right).ok_or(CoreError::LengthOverflow)
}

#[cfg(test)]
mod batch_tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    struct XorShiftReader {
        remaining: u64,
        word: u64,
    }

    impl Read for XorShiftReader {
        fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
            let count = output.len().min(self.remaining as usize);
            for byte in &mut output[..count] {
                self.word ^= self.word << 13;
                self.word ^= self.word >> 7;
                self.word ^= self.word << 17;
                *byte = self.word as u8;
            }
            self.remaining -= count as u64;
            Ok(count)
        }
    }

    #[derive(Clone, Default)]
    struct SharedStore {
        objects: Rc<RefCell<BTreeMap<ObjectId, Vec<u8>>>>,
        puts: Rc<RefCell<u64>>,
    }

    impl ObjectStore for SharedStore {
        fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
            self.objects
                .borrow()
                .get(&id)
                .cloned()
                .ok_or(CoreError::MissingObject)
        }

        fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
            let id = ObjectId::for_bytes(canonical);
            self.objects.borrow_mut().insert(id, canonical.to_vec());
            *self.puts.borrow_mut() += 1;
            Ok(id)
        }
    }

    #[test]
    fn file_batch_streams_payload_while_deferring_only_structural_objects() {
        let mut store = SharedStore::default();
        let observed = store.clone();
        let mut word = 0x74a9_32bc_51de_8801_u64;
        let input = (0..2 * 1024 * 1024)
            .map(|_| {
                word ^= word << 13;
                word ^= word >> 7;
                word ^= word << 17;
                word as u8
            })
            .collect::<Vec<_>>();
        let mut batch = FileMutationBatch::new(&mut store, None).unwrap();
        batch.replace(0, 0, input.as_slice()).unwrap();

        let persisted_bytes = observed
            .objects
            .borrow()
            .values()
            .map(Vec::len)
            .sum::<usize>();
        let deferred_bytes = batch.objects.objects.values().map(Vec::len).sum::<usize>();
        assert!(persisted_bytes >= input.len());
        assert!(deferred_bytes * 8 < input.len());

        let puts_before_finish = *observed.puts.borrow();
        let sealed_before_finish = batch.objects.sealed_node_puts;
        let (root, counters) = batch.finish().unwrap();
        assert_eq!(
            *observed.puts.borrow() - puts_before_finish,
            counters.nodes_created - sealed_before_finish + 1
        );
        let mut output = Vec::new();
        read_all(&store, root, &mut output).unwrap();
        assert_eq!(output, input);
    }

    #[test]
    fn file_batch_streams_large_replacement_below_structural_hard_bound() {
        let mut store = SharedStore::default();
        let mut batch = FileMutationBatch::new(&mut store, None).unwrap();
        batch
            .replace(
                0,
                0,
                XorShiftReader {
                    remaining: 64 * 1024 * 1024,
                    word: 0x74a9_32bc_51de_8801,
                },
            )
            .unwrap();
        assert!(batch.deferred_peak_bytes() <= FILE_MUTATION_BATCH_MAX_DEFERRED_BYTES as u64);
        assert!(batch.objects.sealed_node_puts > 0);
        let (root, counters) = batch.finish().unwrap();
        assert_eq!(counters.logical_len_after, Some(64 * 1024 * 1024));
        assert!(counters.nodes_created > 0);
        assert_eq!(
            state(&store, root, &mut RopeCounters::default())
                .unwrap()
                .logical_len,
            64 * 1024 * 1024
        );
    }
}
