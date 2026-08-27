use crate::content::rope::{read_range, state, validate_file, FileStateRoot, RopeCounters};
use crate::content::rope::{ObjectRead, ObjectStore};
use crate::inode::{InodeId, InodeKind, InodeRecordV1};
use crate::metadata::{
    decode_apple_acl, visit_metadata_entries, PortableMetadataV1, SUPPORTED_BSD_FLAGS,
};
use crate::namespace_codec::{
    decode_directory_node, decode_directory_state, decode_symlink, encode_directory_node,
    encode_directory_state, profile_id, DirectoryNodeV1,
};
use crate::{CanonicalName, CoreError, CoreResult, ObjectId};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NamespaceRootV1 {
    pub profile_id: ObjectId,
    pub root_directory_inode: InodeId,
    pub inode_table_root: ObjectId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DirectoryStateV1 {
    pub entry_count: u64,
    pub tree_level: u8,
    pub profile_id: ObjectId,
    pub mapping_root: ObjectId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SymlinkStateV1 {
    pub target: Vec<u8>,
}

impl SymlinkStateV1 {
    pub fn new(target: Vec<u8>) -> CoreResult<Self> {
        if target.len() > 4096 || target.contains(&0) {
            return Err(CoreError::InvalidRecord("symlink target"));
        }
        Ok(Self { target })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DirectoryStateRoot(pub ObjectId);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct NamespaceCounters {
    pub nodes_read: u64,
    pub nodes_created: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryPage {
    pub entries: Vec<(CanonicalName, InodeId)>,
    pub continuation: Option<CanonicalName>,
}

#[derive(Clone)]
struct NodeSummary {
    id: ObjectId,
    min: Option<CanonicalName>,
    max: Option<CanonicalName>,
    entries: u64,
    encoded_bytes: u64,
    level: u8,
}

struct ValidatedNode {
    node: DirectoryNodeV1,
    summary: NodeSummary,
}

pub fn empty_directory<S: ObjectStore>(store: &mut S) -> CoreResult<DirectoryStateRoot> {
    let mut counters = NamespaceCounters::default();
    let node = emit_directory_node(
        store,
        DirectoryNodeV1::Leaf {
            subtree_encoded_bytes: 0,
            entries: Vec::new(),
        },
        &mut counters,
    )?;
    store_directory_state(store, node)
}

pub fn directory_lookup<S: ObjectRead>(
    store: &S,
    root: DirectoryStateRoot,
    name: &CanonicalName,
    counters: &mut NamespaceCounters,
) -> CoreResult<Option<InodeId>> {
    let state = load_directory_state(store, root, counters)?;
    let mut current = load_directory_root_shallow(store, &state, counters)?;
    loop {
        match current.node {
            DirectoryNodeV1::Leaf { entries, .. } => {
                return Ok(entries
                    .binary_search_by(|(candidate, _)| candidate.cmp(name))
                    .ok()
                    .map(|index| entries[index].1))
            }
            DirectoryNodeV1::Branch { children, .. } => {
                let index = children
                    .partition_point(|(maximum, _)| maximum < name)
                    .min(children.len().saturating_sub(1));
                let expected_max = children[index].0.clone();
                let child =
                    load_directory_node_shallow(store, children[index].1, false, None, counters)?;
                if child.summary.max.as_ref() != Some(&expected_max)
                    || child.summary.level.checked_add(1) != Some(current.summary.level)
                {
                    return Err(CoreError::InvalidRecord("directory child summary"));
                }
                current = child;
            }
        }
    }
}

pub fn directory_entries<S: ObjectRead>(
    store: &S,
    root: DirectoryStateRoot,
    counters: &mut NamespaceCounters,
) -> CoreResult<Vec<(CanonicalName, InodeId)>> {
    let mut output = Vec::new();
    visit_directory_entries(store, root, counters, |entries| {
        output.extend_from_slice(entries);
        Ok(())
    })?;
    Ok(output)
}

/// Returns one bounded ordered page strictly after `exclusive_after`.
pub fn directory_page_after<S: ObjectRead>(
    store: &S,
    root: DirectoryStateRoot,
    exclusive_after: Option<&CanonicalName>,
    max_entries: usize,
    max_bytes: usize,
    counters: &mut NamespaceCounters,
) -> CoreResult<DirectoryPage> {
    if max_entries == 0 || max_bytes == 0 {
        return Err(CoreError::ObjectLimitExceeded);
    }
    let state = load_directory_state(store, root, counters)?;
    let mut cursor =
        DirectoryEntryCursor::after(store, &state, exclusive_after, counters)?.peekable();
    let mut entries = Vec::new();
    let mut bytes = 0_usize;
    while entries.len() < max_entries {
        let Some(next) = cursor.peek() else {
            return Ok(DirectoryPage {
                entries,
                continuation: None,
            });
        };
        let width = match next {
            Ok((name, _)) => 34_usize
                .checked_add(name.as_bytes().len())
                .ok_or(CoreError::LengthOverflow)?,
            Err(_) => return Err(cursor.next().expect("peeked entry").unwrap_err()),
        };
        if bytes
            .checked_add(width)
            .is_none_or(|total| total > max_bytes)
        {
            if entries.is_empty() {
                return Err(CoreError::ObjectLimitExceeded);
            }
            break;
        }
        bytes += width;
        entries.push(cursor.next().expect("peeked entry")?);
    }
    Ok(DirectoryPage {
        continuation: entries.last().map(|entry| entry.0.clone()),
        entries,
    })
}

pub fn visit_directory_entries<S: ObjectRead>(
    store: &S,
    root: DirectoryStateRoot,
    counters: &mut NamespaceCounters,
    mut visitor: impl FnMut(&[(CanonicalName, InodeId)]) -> CoreResult<()>,
) -> CoreResult<()> {
    let state = load_directory_state(store, root, counters)?;
    let summary = walk_directory_node(
        store,
        state.mapping_root,
        true,
        None,
        None,
        counters,
        &mut visitor,
    )?;
    if summary.entries != state.entry_count || summary.level != state.tree_level {
        return Err(CoreError::InvalidRecord("directory state summary"));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryEntryDiff {
    pub name: CanonicalName,
    pub before: Option<InodeId>,
    pub after: Option<InodeId>,
}

/// Streams changed directory entries while pruning equal persistent
/// subtrees. Unequal heights or page partitions use bounded leaf cursors.
pub fn diff_directory_entries<S: ObjectRead>(
    store: &S,
    old: DirectoryStateRoot,
    new: DirectoryStateRoot,
    mut visitor: impl FnMut(DirectoryEntryDiff) -> CoreResult<()>,
) -> CoreResult<NamespaceCounters> {
    let mut counters = NamespaceCounters::default();
    if old == new {
        return Ok(counters);
    }
    let old_state = load_directory_state(store, old, &mut counters)?;
    let new_state = load_directory_state(store, new, &mut counters)?;
    if old_state.mapping_root == new_state.mapping_root {
        if old_state.entry_count != new_state.entry_count
            || old_state.tree_level != new_state.tree_level
        {
            return Err(CoreError::InvalidRecord("directory state summary"));
        }
        return Ok(counters);
    }
    diff_directory_nodes(
        store,
        old_state.mapping_root,
        new_state.mapping_root,
        true,
        &mut counters,
        &mut visitor,
    )?;
    Ok(counters)
}

/// Merges entry-wise changes from `base -> source` onto `destination` without
/// retaining a directory inventory. Conflicting changes to the same name
/// return `None`.
pub fn merge_directory_roots<S: ObjectStore>(
    store: &mut S,
    base: DirectoryStateRoot,
    source: DirectoryStateRoot,
    destination: DirectoryStateRoot,
) -> CoreResult<Option<(DirectoryStateRoot, NamespaceCounters)>> {
    let mut counters = NamespaceCounters::default();
    if source == base || source == destination {
        return Ok(Some((destination, counters)));
    }
    if destination == base {
        return Ok(Some((source, counters)));
    }
    let base_state = load_directory_state(store, base, &mut counters)?;
    let source_state = load_directory_state(store, source, &mut counters)?;
    let destination_state = load_directory_state(store, destination, &mut counters)?;
    for state in [&source_state, &destination_state] {
        if state.profile_id != base_state.profile_id {
            return Err(CoreError::InvalidRecord("directory profile"));
        }
    }
    let mut diffs = StreamingDirectoryDiff::new(base_state.mapping_root, source_state.mapping_root);
    let mut merged = destination;
    while let Some(change) = diffs.next(store, &mut counters)? {
        let mut visits = NamespaceCounters::default();
        let current = directory_lookup(store, merged, &change.name, &mut visits)?;
        add_namespace_counters(&mut counters, visits)?;
        let selected = if change.after == change.before || change.after == current {
            current
        } else if current == change.before {
            change.after
        } else {
            return Ok(None);
        };
        if selected == current {
            continue;
        }
        if current.is_some() {
            let (next, _, visits) = directory_remove(store, merged, &change.name)?;
            add_namespace_counters(&mut counters, visits)?;
            merged = next;
        }
        if let Some(inode) = selected {
            let (next, visits) = directory_insert(store, merged, change.name, inode)?;
            add_namespace_counters(&mut counters, visits)?;
            merged = next;
        }
    }
    Ok(Some((merged, counters)))
}

fn add_namespace_counters(
    target: &mut NamespaceCounters,
    source: NamespaceCounters,
) -> CoreResult<()> {
    target.nodes_read = target
        .nodes_read
        .checked_add(source.nodes_read)
        .ok_or(CoreError::LengthOverflow)?;
    target.nodes_created = target
        .nodes_created
        .checked_add(source.nodes_created)
        .ok_or(CoreError::LengthOverflow)?;
    Ok(())
}

pub fn directory_insert<S: ObjectStore>(
    store: &mut S,
    root: DirectoryStateRoot,
    name: CanonicalName,
    inode: InodeId,
) -> CoreResult<(DirectoryStateRoot, NamespaceCounters)> {
    let mut counters = NamespaceCounters::default();
    let state = load_directory_state(store, root, &mut counters)?;
    let summary = load_directory_root_shallow(store, &state, &mut counters)?.summary;
    let mut replacements = insert_node(store, summary, true, name, inode, &mut counters)?;
    let next = if replacements.len() == 1 {
        replacements.remove(0)
    } else {
        emit_branch_from_summaries(
            store,
            state
                .tree_level
                .checked_add(1)
                .filter(|level| *level <= 31)
                .ok_or(CoreError::MappingDepthExceeded)?,
            replacements,
            &mut counters,
        )?
    };
    Ok((store_directory_state(store, next)?, counters))
}

pub fn directory_remove<S: ObjectStore>(
    store: &mut S,
    root: DirectoryStateRoot,
    name: &CanonicalName,
) -> CoreResult<(DirectoryStateRoot, InodeId, NamespaceCounters)> {
    let mut deferred = DeferredDirectory::new(store);
    let (root, removed, mut counters) = directory_remove_inner(&mut deferred, root, name)?;
    counters.nodes_created = deferred.commit(root)?;
    Ok((root, removed, counters))
}

fn directory_remove_inner<S: ObjectStore>(
    store: &mut S,
    root: DirectoryStateRoot,
    name: &CanonicalName,
) -> CoreResult<(DirectoryStateRoot, InodeId, NamespaceCounters)> {
    let mut counters = NamespaceCounters::default();
    let state = load_directory_state(store, root, &mut counters)?;
    let summary = load_directory_root_shallow(store, &state, &mut counters)?.summary;
    let (mut next, removed) = remove_node(store, summary, true, name, &mut counters)?;
    if let DirectoryNodeV1::Branch { children, .. } =
        load_directory_node(store, next.id, &mut counters)?
    {
        if children.len() == 1 {
            next = load_directory_summary(store, children[0].1, &mut counters)?;
        }
    }
    Ok((store_directory_state(store, next)?, removed, counters))
}

pub fn directory_rename<S: ObjectStore>(
    store: &mut S,
    root: DirectoryStateRoot,
    from: &CanonicalName,
    to: CanonicalName,
) -> CoreResult<(DirectoryStateRoot, NamespaceCounters)> {
    if directory_lookup(store, root, &to, &mut NamespaceCounters::default())?.is_some() {
        return Err(CoreError::NameCollision);
    }
    let mut deferred = DeferredDirectory::new(store);
    let (without, inode, mut counters) = directory_remove_inner(&mut deferred, root, from)?;
    let (renamed, inserted) = directory_insert(&mut deferred, without, to, inode)?;
    counters.nodes_read = counters
        .nodes_read
        .checked_add(inserted.nodes_read)
        .ok_or(CoreError::LengthOverflow)?;
    counters.nodes_created = counters
        .nodes_created
        .checked_add(inserted.nodes_created)
        .ok_or(CoreError::LengthOverflow)?;
    counters.nodes_created = deferred.commit(renamed)?;
    Ok((renamed, counters))
}

pub(crate) struct DeferredDirectory<'a, S> {
    store: &'a mut S,
    objects: BTreeMap<ObjectId, Vec<u8>>,
    charged_bytes: usize,
    peak_charged_bytes: usize,
    prunes: u64,
}

const DEFERRED_DIRECTORY_PRUNE_BYTES: usize = 4 * 1024 * 1024;
pub(crate) const DEFERRED_DIRECTORY_MAX_BYTES: usize = 8 * 1024 * 1024 - 1;
const DEFERRED_OBJECT_CHARGE_BYTES: usize = 128;

impl<'a, S: ObjectStore> DeferredDirectory<'a, S> {
    pub(crate) fn new(store: &'a mut S) -> Self {
        Self {
            store,
            objects: BTreeMap::new(),
            charged_bytes: 0,
            peak_charged_bytes: 0,
            prunes: 0,
        }
    }

    pub(crate) fn prune_to(&mut self, root: DirectoryStateRoot) -> CoreResult<()> {
        if self.charged_bytes <= DEFERRED_DIRECTORY_PRUNE_BYTES {
            return Ok(());
        }
        let mut reachable = BTreeSet::new();
        self.collect_state(root, &mut reachable)?;
        self.objects.retain(|id, _| reachable.contains(id));
        self.charged_bytes = self.objects.values().try_fold(0_usize, |total, bytes| {
            total
                .checked_add(object_charge(bytes.len())?)
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
        root: DirectoryStateRoot,
        reachable: &mut BTreeSet<ObjectId>,
    ) -> CoreResult<()> {
        let Some(canonical) = self.objects.get(&root.0) else {
            return Ok(());
        };
        reachable.insert(root.0);
        let state = decode_directory_state(canonical)?;
        self.collect_node(state.mapping_root, reachable)
    }

    fn collect_node(&self, id: ObjectId, reachable: &mut BTreeSet<ObjectId>) -> CoreResult<()> {
        let Some(canonical) = self.objects.get(&id) else {
            return Ok(());
        };
        if !reachable.insert(id) {
            return Ok(());
        }
        if let DirectoryNodeV1::Branch { children, .. } = decode_directory_node(canonical)? {
            for (_, child) in children {
                self.collect_node(child, reachable)?;
            }
        }
        Ok(())
    }

    pub(crate) fn peak_charged_bytes(&self) -> usize {
        self.peak_charged_bytes
    }

    pub(crate) fn prunes(&self) -> u64 {
        self.prunes
    }

    pub(crate) fn commit(&mut self, root: DirectoryStateRoot) -> CoreResult<u64> {
        let Some(state_bytes) = self.objects.get(&root.0).cloned() else {
            return Ok(0);
        };
        let state = decode_directory_state(&state_bytes)?;
        let mut committed = BTreeSet::new();
        self.commit_node(state.mapping_root, &mut committed)?;
        if self.store.put(&state_bytes)? != root.0 {
            return Err(CoreError::IdentityMismatch);
        }
        u64::try_from(committed.len()).map_err(|_| CoreError::LengthOverflow)
    }

    fn commit_node(&mut self, id: ObjectId, committed: &mut BTreeSet<ObjectId>) -> CoreResult<()> {
        let Some(canonical) = self.objects.get(&id).cloned() else {
            return Ok(());
        };
        if !committed.insert(id) {
            return Ok(());
        }
        if let DirectoryNodeV1::Branch { children, .. } = decode_directory_node(&canonical)? {
            for (_, child) in children {
                self.commit_node(child, committed)?;
            }
        }
        if self.store.put(&canonical)? != id {
            return Err(CoreError::IdentityMismatch);
        }
        Ok(())
    }
}

impl<S: ObjectStore> ObjectStore for DeferredDirectory<'_, S> {
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
        let charged = object_charge(canonical.len())?;
        let next = self
            .charged_bytes
            .checked_add(charged)
            .ok_or(CoreError::LengthOverflow)?;
        if next > DEFERRED_DIRECTORY_MAX_BYTES {
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

fn object_charge(canonical_bytes: usize) -> CoreResult<usize> {
    canonical_bytes
        .checked_add(DEFERRED_OBJECT_CHARGE_BYTES)
        .ok_or(CoreError::LengthOverflow)
}

fn remove_node<S: ObjectStore>(
    store: &mut S,
    summary: NodeSummary,
    root: bool,
    name: &CanonicalName,
    counters: &mut NamespaceCounters,
) -> CoreResult<(NodeSummary, InodeId)> {
    let loaded = load_directory_node_expected_shallow(store, &summary, root, counters)?;
    match loaded.node {
        DirectoryNodeV1::Leaf { mut entries, .. } => {
            let index = entries
                .binary_search_by(|(candidate, _)| candidate.cmp(name))
                .map_err(|_| CoreError::PathNotFound)?;
            let removed = entries.remove(index).1;
            Ok((
                emit_directory_node(store, leaf(entries)?, counters)?,
                removed,
            ))
        }
        DirectoryNodeV1::Branch {
            level,
            subtree_entry_count,
            subtree_encoded_bytes,
            mut children,
        } => {
            let index = children
                .partition_point(|(maximum, _)| maximum < name)
                .min(children.len() - 1);
            let old = load_directory_summary(store, children[index].1, counters)?;
            if old.max.as_ref() != Some(&children[index].0)
                || old.level.checked_add(1) != Some(level)
            {
                return Err(CoreError::InvalidRecord("directory child summary"));
            }
            let (next, removed) = remove_node(store, old.clone(), false, name, counters)?;
            children[index] = (
                next.max
                    .clone()
                    .unwrap_or_else(|| children[index].0.clone()),
                next.id,
            );
            if children.len() > 1 && is_underfull(store, next.id, counters)? {
                children = rebalance_children(store, level - 1, children, index, counters)?;
            }
            let entries = subtree_entry_count
                .checked_sub(old.entries)
                .and_then(|value| value.checked_add(next.entries))
                .ok_or(CoreError::LengthOverflow)?;
            let bytes = subtree_encoded_bytes
                .checked_sub(old.encoded_bytes)
                .and_then(|value| value.checked_add(next.encoded_bytes))
                .ok_or(CoreError::LengthOverflow)?;
            let node = DirectoryNodeV1::Branch {
                level,
                subtree_entry_count: entries,
                subtree_encoded_bytes: bytes,
                children,
            };
            Ok((emit_directory_node(store, node, counters)?, removed))
        }
    }
}

fn rebalance_children<S: ObjectStore>(
    store: &mut S,
    child_level: u8,
    children: Vec<(CanonicalName, ObjectId)>,
    index: usize,
    counters: &mut NamespaceCounters,
) -> CoreResult<Vec<(CanonicalName, ObjectId)>> {
    if index > 0 {
        if let Some(replacements) = try_directory_borrow(
            store,
            child_level,
            children[index - 1].1,
            children[index].1,
            true,
            counters,
        )? {
            return replace_directory_pair(children, index - 1, replacements);
        }
    }
    if index + 1 < children.len() {
        if let Some(replacements) = try_directory_borrow(
            store,
            child_level,
            children[index].1,
            children[index + 1].1,
            false,
            counters,
        )? {
            return replace_directory_pair(children, index, replacements);
        }
    }
    if index > 0 {
        if let Some(replacement) = try_directory_merge(
            store,
            child_level,
            children[index - 1].1,
            children[index].1,
            counters,
        )? {
            return replace_directory_pair(children, index - 1, vec![replacement]);
        }
    }
    if index + 1 < children.len() {
        if let Some(replacement) = try_directory_merge(
            store,
            child_level,
            children[index].1,
            children[index + 1].1,
            counters,
        )? {
            return replace_directory_pair(children, index, vec![replacement]);
        }
    }
    Err(CoreError::NonCanonicalPagePartition)
}

fn try_directory_borrow<S: ObjectStore>(
    store: &mut S,
    child_level: u8,
    left_id: ObjectId,
    right_id: ObjectId,
    borrow_left: bool,
    counters: &mut NamespaceCounters,
) -> CoreResult<Option<Vec<NodeSummary>>> {
    let left = load_directory_node(store, left_id, counters)?;
    let right = load_directory_node(store, right_id, counters)?;
    let (left, right) = match (left, right) {
        (
            DirectoryNodeV1::Leaf {
                entries: mut left, ..
            },
            DirectoryNodeV1::Leaf {
                entries: mut right, ..
            },
        ) => loop {
            if borrow_left {
                let Some(entry) = left.pop() else {
                    return Ok(None);
                };
                right.insert(0, entry);
            } else {
                if right.is_empty() {
                    return Ok(None);
                }
                left.push(right.remove(0));
            }
            let left_node = leaf(left.clone())?;
            let right_node = leaf(right.clone())?;
            if !directory_filled(if borrow_left { &left_node } else { &right_node }) {
                return Ok(None);
            }
            if directory_filled(if borrow_left { &right_node } else { &left_node }) {
                break (left_node, right_node);
            }
        },
        (
            DirectoryNodeV1::Branch {
                children: mut left, ..
            },
            DirectoryNodeV1::Branch {
                children: mut right,
                ..
            },
        ) => loop {
            if borrow_left {
                let Some(entry) = left.pop() else {
                    return Ok(None);
                };
                right.insert(0, entry);
            } else {
                if right.is_empty() {
                    return Ok(None);
                }
                left.push(right.remove(0));
            }
            let left_node = branch(store, child_level, left.clone(), counters)?;
            let right_node = branch(store, child_level, right.clone(), counters)?;
            if !directory_filled(if borrow_left { &left_node } else { &right_node }) {
                return Ok(None);
            }
            if directory_filled(if borrow_left { &right_node } else { &left_node }) {
                break (left_node, right_node);
            }
        },
        _ => return Err(CoreError::WrongLogicalRole),
    };
    if !directory_filled(&left) || !directory_filled(&right) {
        return Ok(None);
    }
    Ok(Some(vec![
        emit_directory_node(store, left, counters)?,
        emit_directory_node(store, right, counters)?,
    ]))
}

fn try_directory_merge<S: ObjectStore>(
    store: &mut S,
    child_level: u8,
    left_id: ObjectId,
    right_id: ObjectId,
    counters: &mut NamespaceCounters,
) -> CoreResult<Option<NodeSummary>> {
    let node = match (
        load_directory_node(store, left_id, counters)?,
        load_directory_node(store, right_id, counters)?,
    ) {
        (
            DirectoryNodeV1::Leaf {
                entries: mut left, ..
            },
            DirectoryNodeV1::Leaf { entries: right, .. },
        ) => {
            left.extend(right);
            leaf(left)?
        }
        (
            DirectoryNodeV1::Branch {
                children: mut left, ..
            },
            DirectoryNodeV1::Branch {
                children: right, ..
            },
        ) => {
            left.extend(right);
            branch(store, child_level, left, counters)?
        }
        _ => return Err(CoreError::WrongLogicalRole),
    };
    if encode_directory_node(&node).is_err() {
        return Ok(None);
    }
    emit_directory_node(store, node, counters).map(Some)
}

fn directory_filled(node: &DirectoryNodeV1) -> bool {
    encode_directory_node(node).is_ok_and(|bytes| bytes.len() * 5 >= 8192 * 2)
}

fn replace_directory_pair(
    mut children: Vec<(CanonicalName, ObjectId)>,
    start: usize,
    replacements: Vec<NodeSummary>,
) -> CoreResult<Vec<(CanonicalName, ObjectId)>> {
    children.splice(
        start..=start + 1,
        replacements
            .into_iter()
            .map(|child| {
                Ok((
                    child
                        .max
                        .ok_or(CoreError::InvalidRecord("empty rebalanced child"))?,
                    child.id,
                ))
            })
            .collect::<CoreResult<Vec<_>>>()?,
    );
    Ok(children)
}

fn is_underfull<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    counters: &mut NamespaceCounters,
) -> CoreResult<bool> {
    Ok(encode_directory_node(&load_directory_node(store, id, counters)?)?.len() * 5 < 8192 * 2)
}

fn insert_node<S: ObjectStore>(
    store: &mut S,
    summary: NodeSummary,
    root: bool,
    name: CanonicalName,
    inode: InodeId,
    counters: &mut NamespaceCounters,
) -> CoreResult<Vec<NodeSummary>> {
    let loaded = load_directory_node_expected_shallow(store, &summary, root, counters)?;
    match loaded.node {
        DirectoryNodeV1::Leaf { mut entries, .. } => {
            match entries.binary_search_by(|(candidate, _)| candidate.cmp(&name)) {
                Ok(_) => return Err(CoreError::NameCollision),
                Err(index) => entries.insert(index, (name, inode)),
            }
            split_leaf_if_needed(store, entries, counters)
        }
        DirectoryNodeV1::Branch {
            level,
            subtree_entry_count,
            subtree_encoded_bytes,
            mut children,
        } => {
            let index = children
                .partition_point(|(maximum, _)| maximum < &name)
                .min(children.len() - 1);
            let child = load_directory_summary(store, children[index].1, counters)?;
            if child.max.as_ref() != Some(&children[index].0)
                || child.level.checked_add(1) != Some(level)
            {
                return Err(CoreError::InvalidRecord("directory child summary"));
            }
            let child_entries = child.entries;
            let child_bytes = child.encoded_bytes;
            let replacements = insert_node(store, child, false, name, inode, counters)?;
            let replacement_entries = replacements.iter().try_fold(0_u64, |sum, item| {
                sum.checked_add(item.entries)
                    .ok_or(CoreError::LengthOverflow)
            })?;
            let replacement_bytes = replacements.iter().try_fold(0_u64, |sum, item| {
                sum.checked_add(item.encoded_bytes)
                    .ok_or(CoreError::LengthOverflow)
            })?;
            let next_entries = subtree_entry_count
                .checked_sub(child_entries)
                .and_then(|value| value.checked_add(replacement_entries))
                .ok_or(CoreError::LengthOverflow)?;
            let next_bytes = subtree_encoded_bytes
                .checked_sub(child_bytes)
                .and_then(|value| value.checked_add(replacement_bytes))
                .ok_or(CoreError::LengthOverflow)?;
            children.splice(
                index..=index,
                replacements.iter().map(|child| {
                    (
                        child.max.clone().expect("nonempty nonroot directory"),
                        child.id,
                    )
                }),
            );
            split_branch_if_needed(store, level, children, next_entries, next_bytes, counters)
        }
    }
}

fn split_leaf_if_needed<S: ObjectStore>(
    store: &mut S,
    entries: Vec<(CanonicalName, InodeId)>,
    counters: &mut NamespaceCounters,
) -> CoreResult<Vec<NodeSummary>> {
    let node = leaf(entries.clone())?;
    if encode_directory_node(&node).is_ok() {
        return emit_directory_node(store, node, counters).map(|node| vec![node]);
    }
    let split = nearest_half(
        entries
            .iter()
            .map(|(name, _)| 34 + name.as_bytes().len())
            .collect(),
    );
    Ok(vec![
        emit_directory_node(store, leaf(entries[..split].to_vec())?, counters)?,
        emit_directory_node(store, leaf(entries[split..].to_vec())?, counters)?,
    ])
}

fn split_branch_if_needed<S: ObjectStore>(
    store: &mut S,
    level: u8,
    children: Vec<(CanonicalName, ObjectId)>,
    entries: u64,
    bytes: u64,
    counters: &mut NamespaceCounters,
) -> CoreResult<Vec<NodeSummary>> {
    let node = DirectoryNodeV1::Branch {
        level,
        subtree_entry_count: entries,
        subtree_encoded_bytes: bytes,
        children: children.clone(),
    };
    if encode_directory_node(&node).is_ok() {
        return emit_directory_node(store, node, counters).map(|node| vec![node]);
    }
    let split = nearest_half(
        children
            .iter()
            .map(|(name, _)| 34 + name.as_bytes().len())
            .collect(),
    );
    Ok(vec![
        emit_directory_node(
            store,
            branch(store, level, children[..split].to_vec(), counters)?,
            counters,
        )?,
        emit_directory_node(
            store,
            branch(store, level, children[split..].to_vec(), counters)?,
            counters,
        )?,
    ])
}

fn leaf(entries: Vec<(CanonicalName, InodeId)>) -> CoreResult<DirectoryNodeV1> {
    let bytes = entries.iter().try_fold(0_u64, |sum, (name, _)| {
        sum.checked_add(34 + name.as_bytes().len() as u64)
            .ok_or(CoreError::LengthOverflow)
    })?;
    Ok(DirectoryNodeV1::Leaf {
        subtree_encoded_bytes: bytes,
        entries,
    })
}

fn branch<S: ObjectRead>(
    store: &S,
    level: u8,
    children: Vec<(CanonicalName, ObjectId)>,
    counters: &mut NamespaceCounters,
) -> CoreResult<DirectoryNodeV1> {
    let mut entry_count = 0_u64;
    let mut encoded_bytes = 0_u64;
    for (_, id) in &children {
        let child = load_directory_summary(store, *id, counters)?;
        entry_count = entry_count
            .checked_add(child.entries)
            .ok_or(CoreError::LengthOverflow)?;
        encoded_bytes = encoded_bytes
            .checked_add(child.encoded_bytes)
            .ok_or(CoreError::LengthOverflow)?;
    }
    Ok(DirectoryNodeV1::Branch {
        level,
        subtree_entry_count: entry_count,
        subtree_encoded_bytes: encoded_bytes,
        children,
    })
}

fn emit_branch_from_summaries<S: ObjectStore>(
    store: &mut S,
    level: u8,
    children: Vec<NodeSummary>,
    counters: &mut NamespaceCounters,
) -> CoreResult<NodeSummary> {
    let entries = children.iter().try_fold(0_u64, |sum, child| {
        sum.checked_add(child.entries)
            .ok_or(CoreError::LengthOverflow)
    })?;
    let bytes = children.iter().try_fold(0_u64, |sum, child| {
        sum.checked_add(child.encoded_bytes)
            .ok_or(CoreError::LengthOverflow)
    })?;
    let descriptors = children
        .into_iter()
        .map(|child| (child.max.expect("nonempty child"), child.id))
        .collect();
    let node = DirectoryNodeV1::Branch {
        level,
        subtree_entry_count: entries,
        subtree_encoded_bytes: bytes,
        children: descriptors,
    };
    emit_directory_node(store, node, counters)
}

fn emit_directory_node<S: ObjectStore>(
    store: &mut S,
    node: DirectoryNodeV1,
    counters: &mut NamespaceCounters,
) -> CoreResult<NodeSummary> {
    let (max, entries, encoded_bytes, level) = node_fields(&node);
    let canonical = encode_directory_node(&node)?;
    let id = store.put(&canonical)?;
    counters.nodes_created = counters
        .nodes_created
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    Ok(NodeSummary {
        id,
        min: node_min(&node),
        max,
        entries,
        encoded_bytes,
        level,
    })
}

fn load_directory_summary<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    counters: &mut NamespaceCounters,
) -> CoreResult<NodeSummary> {
    Ok(load_directory_node_shallow(store, id, true, None, counters)?.summary)
}

fn load_directory_root_shallow<S: ObjectRead>(
    store: &S,
    state: &DirectoryStateV1,
    counters: &mut NamespaceCounters,
) -> CoreResult<ValidatedNode> {
    let loaded = load_directory_node_shallow(store, state.mapping_root, true, None, counters)?;
    if loaded.summary.entries != state.entry_count || loaded.summary.level != state.tree_level {
        return Err(CoreError::InvalidRecord("directory state summary"));
    }
    Ok(loaded)
}

fn load_directory_node_expected_shallow<S: ObjectRead>(
    store: &S,
    expected: &NodeSummary,
    root: bool,
    counters: &mut NamespaceCounters,
) -> CoreResult<ValidatedNode> {
    load_directory_node_shallow(store, expected.id, root, Some(expected), counters)
}

fn load_directory_node_shallow<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    root: bool,
    expected: Option<&NodeSummary>,
    counters: &mut NamespaceCounters,
) -> CoreResult<ValidatedNode> {
    let node = load_directory_node(store, id, counters)?;
    let summary = directory_node_shape(id, &node, root)?;
    if expected.is_some_and(|expected| {
        summary.max != expected.max
            || summary.entries != expected.entries
            || summary.encoded_bytes != expected.encoded_bytes
            || summary.level != expected.level
    }) {
        return Err(CoreError::InvalidRecord("directory child summary"));
    }
    Ok(ValidatedNode { node, summary })
}

fn walk_directory_node<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    root: bool,
    expected_level: Option<u8>,
    expected_max: Option<&CanonicalName>,
    counters: &mut NamespaceCounters,
    visitor: &mut impl FnMut(&[(CanonicalName, InodeId)]) -> CoreResult<()>,
) -> CoreResult<NodeSummary> {
    let node = load_directory_node(store, id, counters)?;
    let mut summary = directory_node_shape(id, &node, root)?;
    if expected_level.is_some_and(|level| summary.level != level)
        || expected_max.is_some_and(|maximum| summary.max.as_ref() != Some(maximum))
    {
        return Err(CoreError::InvalidRecord("directory child summary"));
    }
    match &node {
        DirectoryNodeV1::Leaf { entries, .. } => visitor(entries)?,
        DirectoryNodeV1::Branch {
            level,
            subtree_entry_count,
            subtree_encoded_bytes,
            children,
        } => {
            let child_level = level
                .checked_sub(1)
                .ok_or(CoreError::InvalidRecord("directory child summary"))?;
            let mut entries = 0_u64;
            let mut bytes = 0_u64;
            let mut minimum = None;
            let mut previous_max: Option<CanonicalName> = None;
            for (maximum, child_id) in children {
                let child = walk_directory_node(
                    store,
                    *child_id,
                    false,
                    Some(child_level),
                    Some(maximum),
                    counters,
                    visitor,
                )?;
                if previous_max
                    .as_ref()
                    .zip(child.min.as_ref())
                    .is_some_and(|(previous, next)| previous >= next)
                {
                    return Err(CoreError::NonCanonicalOrdering);
                }
                if minimum.is_none() {
                    minimum = child.min.clone();
                }
                entries = entries
                    .checked_add(child.entries)
                    .ok_or(CoreError::LengthOverflow)?;
                bytes = bytes
                    .checked_add(child.encoded_bytes)
                    .ok_or(CoreError::LengthOverflow)?;
                previous_max = child.max;
            }
            if entries != *subtree_entry_count || bytes != *subtree_encoded_bytes {
                return Err(CoreError::InvalidRecord("directory branch summary"));
            }
            summary.min = minimum;
        }
    }
    Ok(summary)
}

fn diff_directory_nodes<S: ObjectRead>(
    store: &S,
    old: ObjectId,
    new: ObjectId,
    root: bool,
    counters: &mut NamespaceCounters,
    visitor: &mut impl FnMut(DirectoryEntryDiff) -> CoreResult<()>,
) -> CoreResult<()> {
    if old == new {
        return Ok(());
    }
    let old_node = load_directory_node_shallow(store, old, root, None, counters)?;
    let new_node = load_directory_node_shallow(store, new, root, None, counters)?;
    match (&old_node.node, &new_node.node) {
        (
            DirectoryNodeV1::Leaf { entries: old, .. },
            DirectoryNodeV1::Leaf { entries: new, .. },
        ) => merge_directory_entries(
            old.iter().cloned().map(Ok),
            new.iter().cloned().map(Ok),
            visitor,
        ),
        (
            DirectoryNodeV1::Branch {
                level: old_level,
                children: old_children,
                ..
            },
            DirectoryNodeV1::Branch {
                level: new_level,
                children: new_children,
                ..
            },
        ) if old_level == new_level => diff_directory_children(
            store,
            *old_level,
            old_children,
            new_children,
            counters,
            visitor,
        ),
        _ => {
            let mut old_counters = NamespaceCounters::default();
            let mut new_counters = NamespaceCounters::default();
            let result = merge_directory_entries(
                DirectoryEntryCursor::new(store, old, root, &mut old_counters),
                DirectoryEntryCursor::new(store, new, root, &mut new_counters),
                visitor,
            );
            counters.nodes_read = counters
                .nodes_read
                .checked_add(old_counters.nodes_read)
                .and_then(|value| value.checked_add(new_counters.nodes_read))
                .ok_or(CoreError::LengthOverflow)?;
            result
        }
    }
}

fn diff_directory_children<S: ObjectRead>(
    store: &S,
    level: u8,
    old: &[(CanonicalName, ObjectId)],
    new: &[(CanonicalName, ObjectId)],
    counters: &mut NamespaceCounters,
    visitor: &mut impl FnMut(DirectoryEntryDiff) -> CoreResult<()>,
) -> CoreResult<()> {
    let child_level = level
        .checked_sub(1)
        .ok_or(CoreError::InvalidRecord("directory child summary"))?;
    let (mut old_index, mut new_index) = (0_usize, 0_usize);
    while old_index < old.len() && new_index < new.len() {
        if old[old_index].0 == new[new_index].0 {
            diff_directory_nodes(
                store,
                old[old_index].1,
                new[new_index].1,
                false,
                counters,
                visitor,
            )?;
            old_index += 1;
            new_index += 1;
            continue;
        }
        let (old_stop, new_stop) = next_directory_boundary(old, new, old_index, new_index)
            .unwrap_or((old.len() - 1, new.len() - 1));
        let mut old_counters = NamespaceCounters::default();
        let mut new_counters = NamespaceCounters::default();
        let result = merge_directory_entries(
            DirectoryEntryCursor::from_children(
                store,
                &old[old_index..=old_stop],
                child_level,
                &mut old_counters,
            ),
            DirectoryEntryCursor::from_children(
                store,
                &new[new_index..=new_stop],
                child_level,
                &mut new_counters,
            ),
            visitor,
        );
        counters.nodes_read = counters
            .nodes_read
            .checked_add(old_counters.nodes_read)
            .and_then(|value| value.checked_add(new_counters.nodes_read))
            .ok_or(CoreError::LengthOverflow)?;
        result?;
        old_index = old_stop + 1;
        new_index = new_stop + 1;
    }
    if old_index < old.len() {
        let mut old_counters = NamespaceCounters::default();
        merge_directory_entries(
            DirectoryEntryCursor::from_children(
                store,
                &old[old_index..],
                child_level,
                &mut old_counters,
            ),
            std::iter::empty(),
            visitor,
        )?;
        counters.nodes_read = counters
            .nodes_read
            .checked_add(old_counters.nodes_read)
            .ok_or(CoreError::LengthOverflow)?;
    }
    if new_index < new.len() {
        let mut new_counters = NamespaceCounters::default();
        merge_directory_entries(
            std::iter::empty(),
            DirectoryEntryCursor::from_children(
                store,
                &new[new_index..],
                child_level,
                &mut new_counters,
            ),
            visitor,
        )?;
        counters.nodes_read = counters
            .nodes_read
            .checked_add(new_counters.nodes_read)
            .ok_or(CoreError::LengthOverflow)?;
    }
    Ok(())
}

fn next_directory_boundary(
    old: &[(CanonicalName, ObjectId)],
    new: &[(CanonicalName, ObjectId)],
    old_start: usize,
    new_start: usize,
) -> Option<(usize, usize)> {
    for (old_index, old_child) in old.iter().enumerate().skip(old_start) {
        if let Some(new_index) = new
            .iter()
            .enumerate()
            .skip(new_start)
            .find_map(|(index, child)| (child.0 == old_child.0).then_some(index))
        {
            return Some((old_index, new_index));
        }
    }
    None
}

fn merge_directory_entries(
    old: impl Iterator<Item = CoreResult<(CanonicalName, InodeId)>>,
    new: impl Iterator<Item = CoreResult<(CanonicalName, InodeId)>>,
    visitor: &mut impl FnMut(DirectoryEntryDiff) -> CoreResult<()>,
) -> CoreResult<()> {
    let mut old = old;
    let mut new = new;
    let mut old_entry = old.next().transpose()?;
    let mut new_entry = new.next().transpose()?;
    loop {
        match (old_entry.take(), new_entry.take()) {
            (None, None) => return Ok(()),
            (Some((old_name, before)), Some((new_name, after))) if old_name == new_name => {
                if before != after {
                    visitor(DirectoryEntryDiff {
                        name: old_name,
                        before: Some(before),
                        after: Some(after),
                    })?;
                }
                old_entry = old.next().transpose()?;
                new_entry = new.next().transpose()?;
            }
            (Some((old_name, before)), Some((new_name, after))) if old_name < new_name => {
                visitor(DirectoryEntryDiff {
                    name: old_name,
                    before: Some(before),
                    after: None,
                })?;
                old_entry = old.next().transpose()?;
                new_entry = Some((new_name, after));
            }
            (Some((old_name, before)), Some((new_name, after))) => {
                visitor(DirectoryEntryDiff {
                    name: new_name,
                    before: None,
                    after: Some(after),
                })?;
                old_entry = Some((old_name, before));
                new_entry = new.next().transpose()?;
            }
            (Some((old_name, before)), None) => {
                visitor(DirectoryEntryDiff {
                    name: old_name,
                    before: Some(before),
                    after: None,
                })?;
                old_entry = old.next().transpose()?;
            }
            (None, Some((new_name, after))) => {
                visitor(DirectoryEntryDiff {
                    name: new_name,
                    before: None,
                    after: Some(after),
                })?;
                new_entry = new.next().transpose()?;
            }
        }
    }
}

struct DirectoryWalkItem {
    id: ObjectId,
    root: bool,
    expected_level: Option<u8>,
    expected_max: Option<CanonicalName>,
}

struct StreamingDirectoryDiff {
    base: StreamingDirectoryCursor,
    source: StreamingDirectoryCursor,
    base_entry: Option<(CanonicalName, InodeId)>,
    source_entry: Option<(CanonicalName, InodeId)>,
    initialized: bool,
}

impl StreamingDirectoryDiff {
    fn new(base: ObjectId, source: ObjectId) -> Self {
        Self {
            base: StreamingDirectoryCursor::new(base),
            source: StreamingDirectoryCursor::new(source),
            base_entry: None,
            source_entry: None,
            initialized: false,
        }
    }

    fn next<S: ObjectRead>(
        &mut self,
        store: &S,
        counters: &mut NamespaceCounters,
    ) -> CoreResult<Option<DirectoryEntryDiff>> {
        if !self.initialized {
            self.base_entry = self.base.next(store, counters)?;
            self.source_entry = self.source.next(store, counters)?;
            self.initialized = true;
        }
        loop {
            match (&self.base_entry, &self.source_entry) {
                (None, None) => return Ok(None),
                (Some((base_name, before)), Some((source_name, after)))
                    if base_name == source_name =>
                {
                    let name = base_name.clone();
                    let (before, after) = (*before, *after);
                    self.base_entry = self.base.next(store, counters)?;
                    self.source_entry = self.source.next(store, counters)?;
                    if before != after {
                        return Ok(Some(DirectoryEntryDiff {
                            name,
                            before: Some(before),
                            after: Some(after),
                        }));
                    }
                }
                (Some((base_name, before)), Some((source_name, _))) if base_name < source_name => {
                    let change = DirectoryEntryDiff {
                        name: base_name.clone(),
                        before: Some(*before),
                        after: None,
                    };
                    self.base_entry = self.base.next(store, counters)?;
                    return Ok(Some(change));
                }
                (Some(_), Some((source_name, after))) => {
                    let change = DirectoryEntryDiff {
                        name: source_name.clone(),
                        before: None,
                        after: Some(*after),
                    };
                    self.source_entry = self.source.next(store, counters)?;
                    return Ok(Some(change));
                }
                (Some((base_name, before)), None) => {
                    let change = DirectoryEntryDiff {
                        name: base_name.clone(),
                        before: Some(*before),
                        after: None,
                    };
                    self.base_entry = self.base.next(store, counters)?;
                    return Ok(Some(change));
                }
                (None, Some((source_name, after))) => {
                    let change = DirectoryEntryDiff {
                        name: source_name.clone(),
                        before: None,
                        after: Some(*after),
                    };
                    self.source_entry = self.source.next(store, counters)?;
                    return Ok(Some(change));
                }
            }
        }
    }
}

struct StreamingDirectoryCursor {
    stack: Vec<DirectoryWalkItem>,
    leaf: std::vec::IntoIter<(CanonicalName, InodeId)>,
}

impl StreamingDirectoryCursor {
    fn new(root: ObjectId) -> Self {
        Self {
            stack: vec![DirectoryWalkItem {
                id: root,
                root: true,
                expected_level: None,
                expected_max: None,
            }],
            leaf: Vec::new().into_iter(),
        }
    }

    fn next<S: ObjectRead>(
        &mut self,
        store: &S,
        counters: &mut NamespaceCounters,
    ) -> CoreResult<Option<(CanonicalName, InodeId)>> {
        loop {
            if let Some(entry) = self.leaf.next() {
                return Ok(Some(entry));
            }
            let Some(item) = self.stack.pop() else {
                return Ok(None);
            };
            let loaded = load_directory_node_shallow(store, item.id, item.root, None, counters)?;
            if item
                .expected_level
                .is_some_and(|level| loaded.summary.level != level)
                || item
                    .expected_max
                    .as_ref()
                    .is_some_and(|maximum| loaded.summary.max.as_ref() != Some(maximum))
            {
                return Err(CoreError::InvalidRecord("directory child summary"));
            }
            match loaded.node {
                DirectoryNodeV1::Leaf { entries, .. } => self.leaf = entries.into_iter(),
                DirectoryNodeV1::Branch {
                    level, children, ..
                } => {
                    let child_level = level
                        .checked_sub(1)
                        .ok_or(CoreError::InvalidRecord("directory child summary"))?;
                    self.stack
                        .extend(children.into_iter().rev().map(|(maximum, id)| {
                            DirectoryWalkItem {
                                id,
                                root: false,
                                expected_level: Some(child_level),
                                expected_max: Some(maximum),
                            }
                        }));
                }
            }
        }
    }
}

struct DirectoryEntryCursor<'a, S> {
    store: &'a S,
    stack: Vec<DirectoryWalkItem>,
    leaf: std::vec::IntoIter<(CanonicalName, InodeId)>,
    counters: &'a mut NamespaceCounters,
}

impl<'a, S> DirectoryEntryCursor<'a, S> {
    fn new(
        store: &'a S,
        root: ObjectId,
        root_context: bool,
        counters: &'a mut NamespaceCounters,
    ) -> Self {
        Self {
            store,
            stack: vec![DirectoryWalkItem {
                id: root,
                root: root_context,
                expected_level: None,
                expected_max: None,
            }],
            leaf: Vec::new().into_iter(),
            counters,
        }
    }

    fn from_children(
        store: &'a S,
        children: &[(CanonicalName, ObjectId)],
        child_level: u8,
        counters: &'a mut NamespaceCounters,
    ) -> Self {
        Self {
            store,
            stack: children
                .iter()
                .rev()
                .map(|(maximum, id)| DirectoryWalkItem {
                    id: *id,
                    root: false,
                    expected_level: Some(child_level),
                    expected_max: Some(maximum.clone()),
                })
                .collect(),
            leaf: Vec::new().into_iter(),
            counters,
        }
    }

    fn after(
        store: &'a S,
        state: &DirectoryStateV1,
        exclusive_after: Option<&CanonicalName>,
        counters: &'a mut NamespaceCounters,
    ) -> CoreResult<Self>
    where
        S: ObjectRead,
    {
        let mut loaded = load_directory_root_shallow(store, state, counters)?;
        let mut stack = Vec::new();
        loop {
            match loaded.node {
                DirectoryNodeV1::Leaf { entries, .. } => {
                    let start = exclusive_after
                        .map(|after| entries.partition_point(|entry| entry.0 <= *after))
                        .unwrap_or(0);
                    return Ok(Self {
                        store,
                        stack,
                        leaf: entries
                            .into_iter()
                            .skip(start)
                            .collect::<Vec<_>>()
                            .into_iter(),
                        counters,
                    });
                }
                DirectoryNodeV1::Branch {
                    level, children, ..
                } => {
                    let child_level = level
                        .checked_sub(1)
                        .ok_or(CoreError::InvalidRecord("directory child summary"))?;
                    let selected = exclusive_after
                        .map(|after| children.partition_point(|entry| entry.0 <= *after))
                        .unwrap_or(0);
                    if selected == children.len() {
                        return Ok(Self {
                            store,
                            stack,
                            leaf: Vec::new().into_iter(),
                            counters,
                        });
                    }
                    stack.extend(children[selected + 1..].iter().rev().map(|(maximum, id)| {
                        DirectoryWalkItem {
                            id: *id,
                            root: false,
                            expected_level: Some(child_level),
                            expected_max: Some(maximum.clone()),
                        }
                    }));
                    let (maximum, id) = &children[selected];
                    loaded = load_directory_node_shallow(store, *id, false, None, counters)?;
                    if loaded.summary.level != child_level
                        || loaded.summary.max.as_ref() != Some(maximum)
                    {
                        return Err(CoreError::InvalidRecord("directory child summary"));
                    }
                }
            }
        }
    }
}

impl<S: ObjectRead> Iterator for DirectoryEntryCursor<'_, S> {
    type Item = CoreResult<(CanonicalName, InodeId)>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(entry) = self.leaf.next() {
                return Some(Ok(entry));
            }
            let item = self.stack.pop()?;
            let loaded = match load_directory_node_shallow(
                self.store,
                item.id,
                item.root,
                None,
                self.counters,
            ) {
                Ok(loaded) => loaded,
                Err(error) => return Some(Err(error)),
            };
            if item
                .expected_level
                .is_some_and(|level| loaded.summary.level != level)
                || item
                    .expected_max
                    .as_ref()
                    .is_some_and(|maximum| loaded.summary.max.as_ref() != Some(maximum))
            {
                return Some(Err(CoreError::InvalidRecord("directory child summary")));
            }
            match loaded.node {
                DirectoryNodeV1::Leaf { entries, .. } => self.leaf = entries.into_iter(),
                DirectoryNodeV1::Branch {
                    level, children, ..
                } => {
                    let Some(child_level) = level.checked_sub(1) else {
                        return Some(Err(CoreError::InvalidRecord("directory child summary")));
                    };
                    self.stack
                        .extend(children.into_iter().rev().map(|(maximum, id)| {
                            DirectoryWalkItem {
                                id,
                                root: false,
                                expected_level: Some(child_level),
                                expected_max: Some(maximum),
                            }
                        }));
                }
            }
        }
    }
}

fn directory_node_shape(
    id: ObjectId,
    node: &DirectoryNodeV1,
    root: bool,
) -> CoreResult<NodeSummary> {
    let canonical_len = encode_directory_node(node)?.len();
    if !root && canonical_len * 5 < 8192 * 2 {
        return Err(CoreError::NonCanonicalPagePartition);
    }
    match node {
        DirectoryNodeV1::Leaf { entries, .. } if !root && entries.is_empty() => {
            return Err(CoreError::NonCanonicalPagePartition)
        }
        DirectoryNodeV1::Branch { children, .. } if children.len() < 2 => {
            return Err(CoreError::NonCanonicalPagePartition)
        }
        _ => {}
    }
    let (max, entries, encoded_bytes, level) = node_fields(node);
    Ok(NodeSummary {
        id,
        min: node_min(node),
        max,
        entries,
        encoded_bytes,
        level,
    })
}

fn node_fields(node: &DirectoryNodeV1) -> (Option<CanonicalName>, u64, u64, u8) {
    match node {
        DirectoryNodeV1::Leaf {
            subtree_encoded_bytes,
            entries,
        } => (
            entries.last().map(|entry| entry.0.clone()),
            entries.len() as u64,
            *subtree_encoded_bytes,
            0,
        ),
        DirectoryNodeV1::Branch {
            level,
            subtree_entry_count,
            subtree_encoded_bytes,
            children,
        } => (
            children.last().map(|entry| entry.0.clone()),
            *subtree_entry_count,
            *subtree_encoded_bytes,
            *level,
        ),
    }
}

fn node_min(node: &DirectoryNodeV1) -> Option<CanonicalName> {
    match node {
        DirectoryNodeV1::Leaf { entries, .. } => entries.first().map(|entry| entry.0.clone()),
        DirectoryNodeV1::Branch { children, .. } => children.first().map(|entry| entry.0.clone()),
    }
}

fn load_directory_node<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    counters: &mut NamespaceCounters,
) -> CoreResult<DirectoryNodeV1> {
    counters.nodes_read = counters
        .nodes_read
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    store.with_authenticated_canonical(id, decode_directory_node)
}

fn load_directory_state<S: ObjectRead>(
    store: &S,
    root: DirectoryStateRoot,
    counters: &mut NamespaceCounters,
) -> CoreResult<DirectoryStateV1> {
    counters.nodes_read = counters
        .nodes_read
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    store.with_authenticated_canonical(root.0, decode_directory_state)
}

fn store_directory_state<S: ObjectStore>(
    store: &mut S,
    node: NodeSummary,
) -> CoreResult<DirectoryStateRoot> {
    let state = DirectoryStateV1 {
        entry_count: node.entries,
        tree_level: node.level,
        profile_id: profile_id(),
        mapping_root: node.id,
    };
    let canonical = encode_directory_state(state)?;
    Ok(DirectoryStateRoot(store.put(&canonical)?))
}

pub fn validate_inode_record<S: ObjectRead>(
    store: &S,
    record: InodeRecordV1,
    root: bool,
    mut child_visitor: impl FnMut(InodeId) -> CoreResult<()>,
) -> CoreResult<()> {
    validate_inode_record_metadata(store, record, root)?;
    match record.kind {
        InodeKind::RegularFile => validate_file(store, FileStateRoot(record.content_root)),
        InodeKind::Symlink => store
            .with_authenticated_canonical(record.content_root, |canonical| {
                decode_symlink(canonical).map(drop)
            }),
        InodeKind::Directory => visit_directory_entries(
            store,
            DirectoryStateRoot(record.content_root),
            &mut NamespaceCounters::default(),
            |entries| {
                for (_, child) in entries {
                    child_visitor(*child)?;
                }
                Ok(())
            },
        ),
    }
}

pub fn validate_inode_record_metadata<S: ObjectRead>(
    store: &S,
    record: InodeRecordV1,
    root: bool,
) -> CoreResult<()> {
    record.validate(root)?;
    validate_metadata(store, record.metadata_root, record.kind)
}

fn validate_metadata<S: ObjectRead>(store: &S, root: ObjectId, kind: InodeKind) -> CoreResult<()> {
    let mut mode = None;
    let mut mtime = None;
    visit_metadata_entries(store, root, |entries| {
        for entry in entries {
            let root = FileStateRoot(entry.value_file_root);
            validate_file(store, root)?;
            let file = state(store, root, &mut RopeCounters::default())?;
            match (entry.key.domain.as_str(), entry.key.key.as_slice()) {
                ("portable", b"mode") if file.logical_len == 4 => {
                    let mut bytes = Vec::new();
                    read_range(store, root, 0..4, &mut bytes)?;
                    mode = Some(u32::from_be_bytes(bytes.try_into().unwrap()));
                }
                ("portable", b"mtime") if file.logical_len == 12 => {
                    let mut bytes = Vec::new();
                    read_range(store, root, 0..12, &mut bytes)?;
                    mtime = Some((
                        i64::from_be_bytes(bytes[..8].try_into().unwrap()),
                        u32::from_be_bytes(bytes[8..].try_into().unwrap()),
                    ));
                }
                ("apple.acl", b"") if file.logical_len <= 4_620 => {
                    let mut bytes = Vec::new();
                    read_range(store, root, 0..file.logical_len, &mut bytes)?;
                    decode_apple_acl(&bytes)?;
                }
                ("apple.bsd-flags", b"") if file.logical_len == 4 => {
                    let mut bytes = Vec::new();
                    read_range(store, root, 0..4, &mut bytes)?;
                    let flags = u32::from_be_bytes(bytes.try_into().unwrap());
                    if flags == 0 || flags & !SUPPORTED_BSD_FLAGS != 0 {
                        return Err(CoreError::InvalidRecord("BSD flags"));
                    }
                }
                ("apple.xattr", _) if file.logical_len <= 1024 * 1024 => {}
                _ => return Err(CoreError::InvalidRecord("metadata value")),
            }
        }
        Ok(())
    })?;
    let (seconds, nanoseconds) = mtime.ok_or(CoreError::InvalidRecord("mtime missing"))?;
    PortableMetadataV1 {
        permission_mode: mode.ok_or(CoreError::InvalidRecord("mode missing"))?,
        mtime_seconds: seconds,
        mtime_nanoseconds: nanoseconds,
    }
    .validate(kind)
}

fn nearest_half(widths: Vec<usize>) -> usize {
    let total: usize = widths.iter().sum();
    let mut prefix = 0;
    let mut best = 1;
    let mut distance = usize::MAX;
    for (index, width) in widths.iter().enumerate().take(widths.len() - 1) {
        prefix += width;
        let next = total.abs_diff(prefix * 2);
        if next < distance {
            best = index + 1;
            distance = next;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct MemoryStore(BTreeMap<ObjectId, Vec<u8>>);

    impl ObjectStore for MemoryStore {
        fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
            self.0.get(&id).cloned().ok_or(CoreError::MissingObject)
        }

        fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
            let id = ObjectId::for_bytes(canonical);
            self.0.insert(id, canonical.to_vec());
            Ok(id)
        }
    }

    #[test]
    fn directory_pages_resume_without_rescanning_or_skipping() {
        let mut store = MemoryStore::default();
        let mut root = empty_directory(&mut store).unwrap();
        for serial in 0..300_u64 {
            let name = CanonicalName::new(&format!("entry-{serial:03}")).unwrap();
            root = directory_insert(
                &mut store,
                root,
                name,
                InodeId::allocate([0x31; 32], serial),
            )
            .unwrap()
            .0;
        }
        let mut after = None;
        let mut names = Vec::new();
        loop {
            let page = directory_page_after(
                &store,
                root,
                after.as_ref(),
                17,
                2048,
                &mut NamespaceCounters::default(),
            )
            .unwrap();
            names.extend(page.entries.iter().map(|entry| entry.0.as_str().to_owned()));
            after = page.continuation;
            if after.is_none() {
                break;
            }
        }
        assert_eq!(names.len(), 300);
        assert!(names.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn variable_width_directory_borrows_until_both_branches_are_filled() {
        let mut store = MemoryStore::default();
        let mut serial = 0_u64;
        let mut children = |prefix: char, count: usize, width: usize, store: &mut MemoryStore| {
            (0..count)
                .map(|index| {
                    let text = if width == 4 {
                        format!("{prefix}{index:03}")
                    } else {
                        format!("{prefix}{}{index:04}", prefix.to_string().repeat(width - 5))
                    };
                    let name = CanonicalName::new(&text).unwrap();
                    let inode = InodeId::allocate([0x51; 32], serial);
                    serial += 1;
                    let id = store.put(
                        &encode_directory_node(&leaf(vec![(name.clone(), inode)])?).unwrap(),
                    )?;
                    Ok((name, id))
                })
                .collect::<CoreResult<Vec<_>>>()
        };
        let left_children = children('a', 131, 4, &mut store).unwrap();
        let right_children = children('m', 11, 255, &mut store).unwrap();
        let left = store
            .put(
                &encode_directory_node(
                    &branch(&store, 1, left_children, &mut NamespaceCounters::default()).unwrap(),
                )
                .unwrap(),
            )
            .unwrap();
        let right = store
            .put(
                &encode_directory_node(
                    &branch(&store, 1, right_children, &mut NamespaceCounters::default()).unwrap(),
                )
                .unwrap(),
            )
            .unwrap();
        let replacements = try_directory_borrow(
            &mut store,
            1,
            left,
            right,
            true,
            &mut NamespaceCounters::default(),
        )
        .unwrap()
        .unwrap();
        let counts = replacements
            .into_iter()
            .map(
                |summary| match decode_directory_node(&store.0[&summary.id]).unwrap() {
                    DirectoryNodeV1::Branch { children, .. } => children.len(),
                    _ => panic!("expected branch"),
                },
            )
            .collect::<Vec<_>>();
        assert_eq!(counts, [129, 13]);
    }
}
