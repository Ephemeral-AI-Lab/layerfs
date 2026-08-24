use super::extent::{
    ChildDescriptorV3, ExtentNodeV3, ExtentSliceV3, FileStateV3, MAX_ENTRIES, MAX_LEVEL,
};
use super::extent_codec::{
    decode_file_state, decode_node_with_context, encode_file_state, encode_node, profile_id,
};
use crate::cdc::FastCdc;
use crate::{decode_bytes_object, encode_bytes_object, CoreError, CoreResult, ObjectId};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::ops::Range;

const STREAM_FLUSH_AT: usize = MAX_ENTRIES + 64;

pub trait ObjectRead {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>>;

    fn with_authenticated_canonical<T, F>(&self, id: ObjectId, callback: F) -> CoreResult<T>
    where
        F: FnOnce(&[u8]) -> CoreResult<T>,
    {
        let bytes = self.get(id)?;
        if ObjectId::for_bytes(&bytes) != id {
            return Err(CoreError::IdentityMismatch);
        }
        callback(&bytes)
    }

    fn get_authenticated_batch<F>(&self, ids: &[ObjectId], mut callback: F) -> CoreResult<()>
    where
        F: FnMut(ObjectId, &[u8]) -> CoreResult<()>,
    {
        for id in ids {
            self.with_authenticated_canonical(*id, |canonical| {
                callback(*id, decode_bytes_object(canonical)?)
            })?;
        }
        Ok(())
    }
}

pub trait ObjectStore {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>>;
    fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId>;

    fn with_authenticated_canonical<T, F>(&self, id: ObjectId, callback: F) -> CoreResult<T>
    where
        F: FnOnce(&[u8]) -> CoreResult<T>,
    {
        let bytes = self.get(id)?;
        if ObjectId::for_bytes(&bytes) != id {
            return Err(CoreError::IdentityMismatch);
        }
        callback(&bytes)
    }
}

impl<T: ObjectStore> ObjectRead for T {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        ObjectStore::get(self, id)
    }

    fn with_authenticated_canonical<U, F>(&self, id: ObjectId, callback: F) -> CoreResult<U>
    where
        F: FnOnce(&[u8]) -> CoreResult<U>,
    {
        ObjectStore::with_authenticated_canonical(self, id, callback)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RopeCounters {
    pub payload_bytes_read: u64,
    pub payload_bytes_written: u64,
    pub cdc_bytes_scanned: u64,
    pub chunks_created: u64,
    pub nodes_read: u64,
    pub nodes_created: u64,
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

fn scan_replacement_mapping<S: ObjectStore, R: Read>(
    store: &mut S,
    source: R,
) -> CoreResult<ReplacementScan> {
    let mut deferred = DeferredNodes::new(store);
    let mut levels = vec![Pending::Extents(Vec::with_capacity(STREAM_FLUSH_AT + 1))];
    let mut counters = RopeCounters::default();
    let mut flushed = 0_u64;
    let cdc = FastCdc::new().scan(source, |chunk| {
        let canonical = encode_bytes_object(chunk)?;
        let payload = deferred.store.put(&canonical)?;
        counters.payload_bytes_written = add(counters.payload_bytes_written, chunk.len() as u64)?;
        counters.chunks_created = add(counters.chunks_created, 1)?;
        match &mut levels[0] {
            Pending::Extents(extents) => {
                extents.push(ExtentSliceV3::new(payload, 0, chunk.len() as u32)?)
            }
            Pending::Children(_) => unreachable!(),
        }
        flush_streaming(&mut deferred, &mut levels, 0, &mut counters)?;
        flushed = add(flushed, deferred.flush_sealed(&levels)?)?;
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
    read_decoded_node(
        store,
        0,
        &range,
        sink,
        counters,
        &mut ancestors,
        &plan.mapping,
    )?;
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
    )?;
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
    let mut counters = RopeCounters::default();
    let old = state(store, root, &mut counters)?;
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
    let scan = scan_replacement_mapping(store, replacement)?;
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
    let canonical = encode_file_state(next)?;
    let id = store.put(&canonical)?;
    Ok((FileStateRoot(id), counters))
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

    fn flush_sealed(&mut self, levels: &[Pending]) -> CoreResult<u64> {
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
            self.flush_node(id, &mut flushed)?;
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

    fn flush_node(&mut self, id: ObjectId, flushed: &mut BTreeSet<ObjectId>) -> CoreResult<()> {
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
                self.flush_node(child.id, flushed)?;
            }
        }
        if self.store.put(&canonical)? != id {
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
            for extent in extents {
                store.with_authenticated_canonical(extent.payload_object_id, |canonical| {
                    let payload = decode_bytes_object(canonical)?;
                    if payload.len() > crate::cdc::MAXIMUM_CHUNK_BYTES
                        || extent
                            .source_offset
                            .checked_add(extent.logical_length)
                            .is_none_or(|end| end as usize > payload.len())
                    {
                        return Err(CoreError::ChunkLengthMismatch);
                    }
                    Ok(())
                })?;
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
    Ok(())
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
    read_decoded_node(store, base, range, sink, counters, ancestors, &node)?;
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
) -> CoreResult<()> {
    match node {
        ExtentNodeV3::Leaf { extents, .. } => {
            let mut logical = base;
            let mut selected = Vec::new();
            for extent in extents {
                let end = logical
                    .checked_add(u64::from(extent.logical_length))
                    .ok_or(CoreError::LengthOverflow)?;
                if end > range.start && logical < range.end {
                    selected.push((*extent, logical, end));
                }
                if logical >= range.end {
                    break;
                }
                logical = end;
            }
            for batch in selected.chunks(64) {
                let ids = batch
                    .iter()
                    .map(|(extent, _, _)| extent.payload_object_id)
                    .collect::<Vec<_>>();
                let mut index = 0_usize;
                store.get_authenticated_batch(&ids, |id, payload| {
                    let (extent, logical, end) = batch
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
                        .ok_or(CoreError::LengthOverflow)?
                        as usize;
                    if source_end > payload.len() {
                        return Err(CoreError::ChunkLengthMismatch);
                    }
                    let overlap_start = range.start.max(logical) - logical;
                    let overlap_end = range.end.min(end) - logical;
                    let start = extent.source_offset as usize + overlap_start as usize;
                    let stop = extent.source_offset as usize + overlap_end as usize;
                    sink.write_all(&payload[start..stop])
                        .map_err(|_| CoreError::Io)?;
                    counters.payload_bytes_read =
                        add(counters.payload_bytes_read, (stop - start) as u64)?;
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
                )?;
                prior = child.cumulative_logical_end;
            }
        }
    }
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
