use super::codec::{decode_inode_table_node, encode_inode_table_node, InodeTableNodeV1};
use super::cursor::{inode_node_min, load, load_shallow};
use super::{GeneratedInodeTable, InodeId, InodeTableCounters, InodeTableRoot, Summary};
use crate::file::rope::{ObjectRead, ObjectStore};
use crate::{CoreError, CoreResult, ObjectId};
use std::collections::{BTreeMap, BTreeSet};

pub fn inode_table_from_root<S: ObjectStore>(
    store: &mut S,
    root_inode: InodeId,
    record: ObjectId,
) -> CoreResult<InodeTableRoot> {
    let mut counters = InodeTableCounters::default();
    Ok(InodeTableRoot(
        emit(
            store,
            InodeTableNodeV1::Leaf(vec![(root_inode, record)]),
            &mut counters,
        )?
        .id,
    ))
}

pub fn inode_table_lookup<S: ObjectRead>(
    store: &S,
    root: InodeTableRoot,
    key: InodeId,
    counters: &mut InodeTableCounters,
) -> CoreResult<Option<ObjectId>> {
    let mut current = load_shallow(store, root.0, true, None, counters)?;
    loop {
        match current.node {
            InodeTableNodeV1::Leaf(entries) => {
                return Ok(entries
                    .binary_search_by_key(&key, |entry| entry.0)
                    .ok()
                    .map(|index| entries[index].1))
            }
            InodeTableNodeV1::Branch { children, .. } => {
                let index = children
                    .partition_point(|entry| entry.0 < key)
                    .min(children.len() - 1);
                let expected_max = children[index].0;
                let child = load_shallow(store, children[index].1, false, None, counters)?;
                if child.summary.max != expected_max
                    || child.summary.level.checked_add(1) != Some(current.summary.level)
                {
                    return Err(CoreError::InvalidRecord("inode child summary"));
                }
                current = child;
            }
        }
    }
}

pub fn inode_table_entries<S: ObjectRead>(
    store: &S,
    root: InodeTableRoot,
    counters: &mut InodeTableCounters,
) -> CoreResult<Vec<(InodeId, ObjectId)>> {
    let mut output = Vec::new();
    visit_inode_table_entries(store, root, counters, |entries| {
        output.extend_from_slice(entries);
        Ok(())
    })?;
    Ok(output)
}

pub fn visit_inode_table_entries<S: ObjectRead>(
    store: &S,
    root: InodeTableRoot,
    counters: &mut InodeTableCounters,
    mut visitor: impl FnMut(&[(InodeId, ObjectId)]) -> CoreResult<()>,
) -> CoreResult<()> {
    walk_inode_node(store, root.0, true, None, None, counters, &mut visitor).map(drop)
}

pub fn inode_table_upsert<S: ObjectStore>(
    store: &mut S,
    root: InodeTableRoot,
    key: InodeId,
    record: ObjectId,
) -> CoreResult<(InodeTableRoot, InodeTableCounters)> {
    let mut counters = InodeTableCounters::default();
    let summary = summary(store, root.0, &mut counters)?;
    upsert_validated(store, summary, key, record, counters)
}

pub fn inode_table_remove<S: ObjectStore>(
    store: &mut S,
    root: InodeTableRoot,
    key: InodeId,
) -> CoreResult<(InodeTableRoot, ObjectId, InodeTableCounters)> {
    let mut deferred = DeferredInodes::new(store);
    let mut counters = InodeTableCounters::default();
    let current = summary(&deferred, root.0, &mut counters)?;
    let (mut next, removed) = remove(&mut deferred, current, true, key, &mut counters)?;
    if let InodeTableNodeV1::Branch { children, .. } = load(&deferred, next.id, &mut counters)? {
        if children.len() == 1 {
            next = summary(&deferred, children[0].1, &mut counters)?;
        }
    }
    counters.nodes_created = deferred.commit(next.id)?;
    Ok((InodeTableRoot(next.id), removed, counters))
}

pub(crate) struct DeferredInodes<'a, S> {
    store: &'a mut S,
    nodes: BTreeMap<ObjectId, Vec<u8>>,
    charged_bytes: usize,
    peak_charged_bytes: usize,
    prunes: u64,
}

const DEFERRED_INODE_PRUNE_BYTES: usize = 4 * 1024 * 1024;
pub(crate) const DEFERRED_INODE_MAX_BYTES: usize = 8 * 1024 * 1024 - 1;
const DEFERRED_OBJECT_CHARGE_BYTES: usize = 128;

impl<'a, S: ObjectStore> DeferredInodes<'a, S> {
    pub(crate) fn new(store: &'a mut S) -> Self {
        Self {
            store,
            nodes: BTreeMap::new(),
            charged_bytes: 0,
            peak_charged_bytes: 0,
            prunes: 0,
        }
    }

    pub(crate) fn prune_to(&mut self, root: ObjectId) -> CoreResult<()> {
        if self.charged_bytes <= DEFERRED_INODE_PRUNE_BYTES {
            return Ok(());
        }
        let mut reachable = BTreeSet::new();
        self.collect_node(root, &mut reachable)?;
        self.nodes.retain(|id, _| reachable.contains(id));
        self.charged_bytes = self.nodes.values().try_fold(0_usize, |total, bytes| {
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

    fn collect_node(&self, id: ObjectId, reachable: &mut BTreeSet<ObjectId>) -> CoreResult<()> {
        let Some(canonical) = self.nodes.get(&id) else {
            return Ok(());
        };
        if !reachable.insert(id) {
            return Ok(());
        }
        if let InodeTableNodeV1::Branch { children, .. } = decode_inode_table_node(canonical)? {
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

    pub(crate) fn commit(&mut self, root: ObjectId) -> CoreResult<u64> {
        let mut committed = BTreeSet::new();
        self.commit_node(root, &mut committed)?;
        u64::try_from(committed.len()).map_err(|_| CoreError::LengthOverflow)
    }

    pub(crate) fn put_persistent(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
        self.store.put(canonical)
    }

    fn commit_node(&mut self, id: ObjectId, committed: &mut BTreeSet<ObjectId>) -> CoreResult<()> {
        let Some(canonical) = self.nodes.get(&id).cloned() else {
            return Ok(());
        };
        if !committed.insert(id) {
            return Ok(());
        }
        if let InodeTableNodeV1::Branch { children, .. } = decode_inode_table_node(&canonical)? {
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

impl<S: ObjectStore> ObjectStore for DeferredInodes<'_, S> {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        self.nodes
            .get(&id)
            .cloned()
            .map_or_else(|| self.store.get(id), Ok)
    }

    fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
        let id = ObjectId::for_bytes(canonical);
        match self.nodes.get(&id) {
            Some(prior) if prior != canonical => return Err(CoreError::IdentityMismatch),
            Some(_) => return Ok(id),
            None => {}
        }
        let charged = object_charge(canonical.len())?;
        let next = self
            .charged_bytes
            .checked_add(charged)
            .ok_or(CoreError::LengthOverflow)?;
        if next > DEFERRED_INODE_MAX_BYTES {
            return Err(CoreError::ObjectLimitExceeded);
        }
        self.nodes.insert(id, canonical.to_vec());
        self.charged_bytes = next;
        self.peak_charged_bytes = self.peak_charged_bytes.max(next);
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

fn object_charge(canonical_bytes: usize) -> CoreResult<usize> {
    canonical_bytes
        .checked_add(DEFERRED_OBJECT_CHARGE_BYTES)
        .ok_or(CoreError::LengthOverflow)
}

pub fn generated_inode_table_from_root<S: ObjectStore>(
    store: &mut S,
    root_inode: InodeId,
    record: ObjectId,
) -> CoreResult<GeneratedInodeTable> {
    inode_table_from_root(store, root_inode, record).map(GeneratedInodeTable)
}

pub fn generated_inode_table_upsert<S: ObjectStore>(
    store: &mut S,
    root: GeneratedInodeTable,
    key: InodeId,
    record: ObjectId,
) -> CoreResult<(GeneratedInodeTable, InodeTableCounters)> {
    let mut counters = InodeTableCounters::default();
    let summary = summary(store, root.0 .0, &mut counters)?;
    upsert_validated(store, summary, key, record, counters)
        .map(|(root, counters)| (GeneratedInodeTable(root), counters))
}

fn upsert_validated<S: ObjectStore>(
    store: &mut S,
    summary: Summary,
    key: InodeId,
    record: ObjectId,
    mut counters: InodeTableCounters,
) -> CoreResult<(InodeTableRoot, InodeTableCounters)> {
    let mut nodes = upsert(store, summary, true, key, record, &mut counters)?;
    let root = if nodes.len() == 1 {
        nodes.remove(0)
    } else {
        emit_branch(
            store,
            nodes[0]
                .level
                .checked_add(1)
                .ok_or(CoreError::MappingDepthExceeded)?,
            nodes,
            &mut counters,
        )?
    };
    Ok((InodeTableRoot(root.id), counters))
}

fn upsert<S: ObjectStore>(
    store: &mut S,
    current: Summary,
    root: bool,
    key: InodeId,
    record: ObjectId,
    counters: &mut InodeTableCounters,
) -> CoreResult<Vec<Summary>> {
    let loaded = load_shallow(store, current.id, root, Some(&current), counters)?;
    match loaded.node {
        InodeTableNodeV1::Leaf(mut entries) => {
            match entries.binary_search_by_key(&key, |entry| entry.0) {
                Ok(index) => entries[index].1 = record,
                Err(index) => entries.insert(index, (key, record)),
            }
            if entries.len() <= 127 {
                Ok(vec![emit(
                    store,
                    InodeTableNodeV1::Leaf(entries),
                    counters,
                )?])
            } else {
                Ok(vec![
                    emit(
                        store,
                        InodeTableNodeV1::Leaf(entries[..64].to_vec()),
                        counters,
                    )?,
                    emit(
                        store,
                        InodeTableNodeV1::Leaf(entries[64..].to_vec()),
                        counters,
                    )?,
                ])
            }
        }
        InodeTableNodeV1::Branch {
            level,
            subtree_entry_count,
            mut children,
        } => {
            let index = children
                .partition_point(|entry| entry.0 < key)
                .min(children.len() - 1);
            let old = load_shallow(store, children[index].1, false, None, counters)?.summary;
            if old.max != children[index].0 || old.level.checked_add(1) != Some(level) {
                return Err(CoreError::InvalidRecord("inode child summary"));
            }
            let replacements = upsert(store, old, false, key, record, counters)?;
            let replacement_count = replacements.iter().try_fold(0_u64, |sum, item| {
                sum.checked_add(item.entries)
                    .ok_or(CoreError::LengthOverflow)
            })?;
            children.splice(
                index..=index,
                replacements.iter().map(|item| (item.max, item.id)),
            );
            let count = subtree_entry_count
                .checked_sub(old.entries)
                .and_then(|count| count.checked_add(replacement_count))
                .ok_or(CoreError::LengthOverflow)?;
            if children.len() <= 127 {
                Ok(vec![emit(
                    store,
                    InodeTableNodeV1::Branch {
                        level,
                        subtree_entry_count: count,
                        children,
                    },
                    counters,
                )?])
            } else {
                let left = children[..64].to_vec();
                let right = children[64..].to_vec();
                Ok(vec![
                    emit_branch_descriptors(store, level, left, counters)?,
                    emit_branch_descriptors(store, level, right, counters)?,
                ])
            }
        }
    }
}

fn remove<S: ObjectStore>(
    store: &mut S,
    current: Summary,
    root: bool,
    key: InodeId,
    counters: &mut InodeTableCounters,
) -> CoreResult<(Summary, ObjectId)> {
    let loaded = load_shallow(store, current.id, root, Some(&current), counters)?;
    match loaded.node {
        InodeTableNodeV1::Leaf(mut entries) => {
            let index = entries
                .binary_search_by_key(&key, |entry| entry.0)
                .map_err(|_| CoreError::PathNotFound)?;
            let removed = entries.remove(index).1;
            if entries.is_empty() {
                return Err(CoreError::InvalidRecord("empty inode table"));
            }
            Ok((
                emit(store, InodeTableNodeV1::Leaf(entries), counters)?,
                removed,
            ))
        }
        InodeTableNodeV1::Branch {
            level,
            subtree_entry_count,
            mut children,
        } => {
            let index = children
                .partition_point(|entry| entry.0 < key)
                .min(children.len() - 1);
            let old = load_shallow(store, children[index].1, false, None, counters)?.summary;
            if old.max != children[index].0 || old.level.checked_add(1) != Some(level) {
                return Err(CoreError::InvalidRecord("inode child summary"));
            }
            let (next, removed) = remove(store, old, false, key, counters)?;
            children[index] = (next.max, next.id);
            if children.len() > 1 && inode_underfull(store, next.id, counters)? {
                children = rebalance_inode_children(store, level - 1, children, index, counters)?;
            }
            let count = subtree_entry_count
                .checked_sub(1)
                .ok_or(CoreError::LengthOverflow)?;
            Ok((
                emit(
                    store,
                    InodeTableNodeV1::Branch {
                        level,
                        subtree_entry_count: count,
                        children,
                    },
                    counters,
                )?,
                removed,
            ))
        }
    }
}

fn inode_underfull<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    counters: &mut InodeTableCounters,
) -> CoreResult<bool> {
    Ok(match load(store, id, counters)? {
        InodeTableNodeV1::Leaf(entries) => entries.len() < 64,
        InodeTableNodeV1::Branch { children, .. } => children.len() < 64,
    })
}

fn rebalance_inode_children<S: ObjectStore>(
    store: &mut S,
    child_level: u8,
    children: Vec<(InodeId, ObjectId)>,
    index: usize,
    counters: &mut InodeTableCounters,
) -> CoreResult<Vec<(InodeId, ObjectId)>> {
    if index > 0 {
        if let Some(replacements) = try_inode_borrow(
            store,
            child_level,
            children[index - 1].1,
            children[index].1,
            true,
            counters,
        )? {
            return Ok(replace_inode_pair(children, index - 1, replacements));
        }
    }
    if index + 1 < children.len() {
        if let Some(replacements) = try_inode_borrow(
            store,
            child_level,
            children[index].1,
            children[index + 1].1,
            false,
            counters,
        )? {
            return Ok(replace_inode_pair(children, index, replacements));
        }
    }
    if index > 0 {
        if let Some(replacement) = try_inode_merge(
            store,
            child_level,
            children[index - 1].1,
            children[index].1,
            counters,
        )? {
            return Ok(replace_inode_pair(children, index - 1, vec![replacement]));
        }
    }
    if index + 1 < children.len() {
        if let Some(replacement) = try_inode_merge(
            store,
            child_level,
            children[index].1,
            children[index + 1].1,
            counters,
        )? {
            return Ok(replace_inode_pair(children, index, vec![replacement]));
        }
    }
    Err(CoreError::NonCanonicalPagePartition)
}

fn try_inode_borrow<S: ObjectStore>(
    store: &mut S,
    child_level: u8,
    left_id: ObjectId,
    right_id: ObjectId,
    borrow_left: bool,
    counters: &mut InodeTableCounters,
) -> CoreResult<Option<Vec<Summary>>> {
    let (left, right) = match (
        load(store, left_id, counters)?,
        load(store, right_id, counters)?,
    ) {
        (InodeTableNodeV1::Leaf(mut left), InodeTableNodeV1::Leaf(mut right)) => {
            if borrow_left {
                if left.len() <= 64 {
                    return Ok(None);
                }
                right.insert(0, left.pop().unwrap());
            } else {
                if right.len() <= 64 {
                    return Ok(None);
                }
                left.push(right.remove(0));
            }
            (InodeTableNodeV1::Leaf(left), InodeTableNodeV1::Leaf(right))
        }
        (
            InodeTableNodeV1::Branch {
                children: mut left, ..
            },
            InodeTableNodeV1::Branch {
                children: mut right,
                ..
            },
        ) => {
            if borrow_left {
                if left.len() <= 64 {
                    return Ok(None);
                }
                right.insert(0, left.pop().unwrap());
            } else {
                if right.len() <= 64 {
                    return Ok(None);
                }
                left.push(right.remove(0));
            }
            (
                InodeTableNodeV1::Branch {
                    level: child_level,
                    subtree_entry_count: inode_children_count(store, &left, counters)?,
                    children: left,
                },
                InodeTableNodeV1::Branch {
                    level: child_level,
                    subtree_entry_count: inode_children_count(store, &right, counters)?,
                    children: right,
                },
            )
        }
        _ => return Err(CoreError::WrongLogicalRole),
    };
    Ok(Some(vec![
        emit(store, left, counters)?,
        emit(store, right, counters)?,
    ]))
}

fn try_inode_merge<S: ObjectStore>(
    store: &mut S,
    child_level: u8,
    left_id: ObjectId,
    right_id: ObjectId,
    counters: &mut InodeTableCounters,
) -> CoreResult<Option<Summary>> {
    let node = match (
        load(store, left_id, counters)?,
        load(store, right_id, counters)?,
    ) {
        (InodeTableNodeV1::Leaf(mut left), InodeTableNodeV1::Leaf(right)) => {
            left.extend(right);
            if left.len() > 127 {
                return Ok(None);
            }
            InodeTableNodeV1::Leaf(left)
        }
        (
            InodeTableNodeV1::Branch {
                children: mut left, ..
            },
            InodeTableNodeV1::Branch {
                children: right, ..
            },
        ) => {
            left.extend(right);
            if left.len() > 127 {
                return Ok(None);
            }
            InodeTableNodeV1::Branch {
                level: child_level,
                subtree_entry_count: inode_children_count(store, &left, counters)?,
                children: left,
            }
        }
        _ => return Err(CoreError::WrongLogicalRole),
    };
    emit(store, node, counters).map(Some)
}

fn inode_children_count<S: ObjectRead>(
    store: &S,
    children: &[(InodeId, ObjectId)],
    counters: &mut InodeTableCounters,
) -> CoreResult<u64> {
    children.iter().try_fold(0_u64, |sum, (_, id)| {
        sum.checked_add(summary(store, *id, counters)?.entries)
            .ok_or(CoreError::LengthOverflow)
    })
}

fn replace_inode_pair(
    mut children: Vec<(InodeId, ObjectId)>,
    start: usize,
    replacements: Vec<Summary>,
) -> Vec<(InodeId, ObjectId)> {
    children.splice(
        start..=start + 1,
        replacements.into_iter().map(|item| (item.max, item.id)),
    );
    children
}

fn emit_branch<S: ObjectStore>(
    store: &mut S,
    level: u8,
    children: Vec<Summary>,
    counters: &mut InodeTableCounters,
) -> CoreResult<Summary> {
    let count = children.iter().try_fold(0_u64, |sum, child| {
        sum.checked_add(child.entries)
            .ok_or(CoreError::LengthOverflow)
    })?;
    let descriptors = children
        .into_iter()
        .map(|child| (child.max, child.id))
        .collect();
    emit(
        store,
        InodeTableNodeV1::Branch {
            level,
            subtree_entry_count: count,
            children: descriptors,
        },
        counters,
    )
}

fn emit_branch_descriptors<S: ObjectStore>(
    store: &mut S,
    level: u8,
    children: Vec<(InodeId, ObjectId)>,
    counters: &mut InodeTableCounters,
) -> CoreResult<Summary> {
    let mut count = 0_u64;
    for (_, id) in &children {
        count = count
            .checked_add(summary(store, *id, counters)?.entries)
            .ok_or(CoreError::LengthOverflow)?;
    }
    emit(
        store,
        InodeTableNodeV1::Branch {
            level,
            subtree_entry_count: count,
            children,
        },
        counters,
    )
}

fn emit<S: ObjectStore>(
    store: &mut S,
    node: InodeTableNodeV1,
    counters: &mut InodeTableCounters,
) -> CoreResult<Summary> {
    let (max, entries, level) = match &node {
        InodeTableNodeV1::Leaf(entries) => (
            entries
                .last()
                .ok_or(CoreError::InvalidRecord("empty inode table"))?
                .0,
            entries.len() as u64,
            0,
        ),
        InodeTableNodeV1::Branch {
            level,
            subtree_entry_count,
            children,
        } => (
            children
                .last()
                .ok_or(CoreError::InvalidRecord("empty inode branch"))?
                .0,
            *subtree_entry_count,
            *level,
        ),
    };
    let canonical = encode_inode_table_node(&node)?;
    let min = inode_node_min(&node)?;
    let id = store.put(&canonical)?;
    counters.nodes_created = counters
        .nodes_created
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    Ok(Summary {
        id,
        min,
        max,
        entries,
        level,
    })
}

fn summary<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    counters: &mut InodeTableCounters,
) -> CoreResult<Summary> {
    Ok(load_shallow(store, id, true, None, counters)?.summary)
}

fn walk_inode_node<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    root: bool,
    expected_level: Option<u8>,
    expected_max: Option<InodeId>,
    counters: &mut InodeTableCounters,
    visitor: &mut impl FnMut(&[(InodeId, ObjectId)]) -> CoreResult<()>,
) -> CoreResult<Summary> {
    let mut loaded = load_shallow(store, id, root, None, counters)?;
    if expected_level.is_some_and(|level| loaded.summary.level != level)
        || expected_max.is_some_and(|maximum| loaded.summary.max != maximum)
    {
        return Err(CoreError::InvalidRecord("inode child summary"));
    }
    match &loaded.node {
        InodeTableNodeV1::Leaf(entries) => visitor(entries)?,
        InodeTableNodeV1::Branch {
            level,
            subtree_entry_count,
            children,
        } => {
            let child_level = level
                .checked_sub(1)
                .ok_or(CoreError::InvalidRecord("inode child summary"))?;
            let mut count = 0_u64;
            let mut minimum = None;
            let mut previous_max: Option<InodeId> = None;
            for (maximum, child_id) in children {
                let child = walk_inode_node(
                    store,
                    *child_id,
                    false,
                    Some(child_level),
                    Some(*maximum),
                    counters,
                    visitor,
                )?;
                if previous_max.is_some_and(|previous| previous >= child.min) {
                    return Err(CoreError::NonCanonicalOrdering);
                }
                minimum.get_or_insert(child.min);
                count = count
                    .checked_add(child.entries)
                    .ok_or(CoreError::LengthOverflow)?;
                previous_max = Some(child.max);
            }
            if count != *subtree_entry_count {
                return Err(CoreError::InvalidRecord("inode branch summary"));
            }
            loaded.summary.min = minimum.ok_or(CoreError::NonCanonicalPagePartition)?;
        }
    }
    Ok(loaded.summary)
}
