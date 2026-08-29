use super::codec::{
    decode_directory_node, decode_directory_state, encode_directory_node, DirectoryNodeV1,
};
use super::node::{DirectoryStateRoot, NamespaceCounters, NodeSummary};
use super::read::{
    directory_lookup, load_directory_node_expected_shallow, load_directory_root_shallow,
    load_directory_summary,
};
use super::validate::{
    load_directory_node, load_directory_state, nearest_half, node_fields, node_min,
    store_directory_state,
};
use crate::file::rope::{ObjectRead, ObjectStore};
use crate::tree::inode::InodeId;
use crate::{CanonicalName, CoreError, CoreResult, ObjectId};
use std::collections::{BTreeMap, BTreeSet};

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

pub(super) fn try_directory_borrow<S: ObjectStore>(
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

pub(super) fn leaf(entries: Vec<(CanonicalName, InodeId)>) -> CoreResult<DirectoryNodeV1> {
    let bytes = entries.iter().try_fold(0_u64, |sum, (name, _)| {
        sum.checked_add(34 + name.as_bytes().len() as u64)
            .ok_or(CoreError::LengthOverflow)
    })?;
    Ok(DirectoryNodeV1::Leaf {
        subtree_encoded_bytes: bytes,
        entries,
    })
}

pub(super) fn branch<S: ObjectRead>(
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

pub(super) fn emit_directory_node<S: ObjectStore>(
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
