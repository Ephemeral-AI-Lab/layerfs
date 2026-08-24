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

struct DeferredDirectory<'a, S> {
    store: &'a mut S,
    objects: BTreeMap<ObjectId, Vec<u8>>,
}

impl<'a, S: ObjectStore> DeferredDirectory<'a, S> {
    fn new(store: &'a mut S) -> Self {
        Self {
            store,
            objects: BTreeMap::new(),
        }
    }

    fn commit(&mut self, root: DirectoryStateRoot) -> CoreResult<u64> {
        let state_bytes = self
            .objects
            .get(&root.0)
            .cloned()
            .ok_or(CoreError::MissingObject)?;
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
        if self
            .objects
            .insert(id, canonical.to_vec())
            .is_some_and(|prior| prior != canonical)
        {
            return Err(CoreError::IdentityMismatch);
        }
        Ok(id)
    }
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
    let canonical = store.get(id)?;
    if ObjectId::for_bytes(&canonical) != id {
        return Err(CoreError::IdentityMismatch);
    }
    counters.nodes_read = counters
        .nodes_read
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    decode_directory_node(&canonical)
}

fn load_directory_state<S: ObjectRead>(
    store: &S,
    root: DirectoryStateRoot,
    counters: &mut NamespaceCounters,
) -> CoreResult<DirectoryStateV1> {
    let canonical = store.get(root.0)?;
    if ObjectId::for_bytes(&canonical) != root.0 {
        return Err(CoreError::IdentityMismatch);
    }
    counters.nodes_read = counters
        .nodes_read
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    decode_directory_state(&canonical)
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
    record.validate(root)?;
    validate_metadata(store, record.metadata_root, record.kind)?;
    match record.kind {
        InodeKind::RegularFile => validate_file(store, FileStateRoot(record.content_root)),
        InodeKind::Symlink => {
            decode_symlink(&authenticated(store, record.content_root)?)?;
            Ok(())
        }
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

fn authenticated<S: ObjectRead>(store: &S, id: ObjectId) -> CoreResult<Vec<u8>> {
    let canonical = store.get(id)?;
    if ObjectId::for_bytes(&canonical) != id {
        return Err(CoreError::IdentityMismatch);
    }
    Ok(canonical)
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
