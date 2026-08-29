use super::build::{add, build, finish, scan_replacement_mapping_with};
use super::read::{state, validate_summary};
use super::state::{DeferredNodes, FileStateRoot, ObjectRead, ObjectStore, RopeCounters, Summary};
use super::validate::{child_summaries, coalesce, merge_counters};
use crate::error::{CoreError, CoreResult};
use crate::file::extent::{
    ChildDescriptorV3, ExtentNodeV3, ExtentSliceV3, FileStateV3, MAX_ENTRIES, MAX_LEVEL,
};
use crate::file::extent_codec::{
    decode_file_state, decode_node_with_context, encode_file_state, encode_node, profile_id,
};
use crate::object::ObjectId;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;

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

pub(super) fn emit_leaf<S: ObjectStore>(
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

pub(super) fn load_node<S: ObjectRead>(
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

pub(super) fn load_node_cached<S: ObjectRead>(
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

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
mod batch_tests {
    use super::super::read::read_all;
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
