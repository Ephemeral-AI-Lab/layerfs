use crate::content::rope::{ObjectRead, ObjectStore};
use crate::metadata::merge_metadata_roots;
use crate::namespace::{merge_directory_roots, DirectoryStateRoot, NamespaceCounters};
use crate::namespace_codec::{
    decode_inode_record, decode_inode_table_node, encode_inode_record, encode_inode_table_node,
    InodeTableNodeV1,
};
use crate::CoreError;
use crate::{CoreResult, ObjectId};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InodeId(pub [u8; 32]);

impl InodeId {
    pub fn allocate(store_id: [u8; 32], serial: u64) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"layerfs/inode-id/v1\0");
        hasher.update(&store_id);
        hasher.update(&serial.to_be_bytes());
        Self(*hasher.finalize().as_bytes())
    }

    pub fn from_slice(bytes: &[u8]) -> CoreResult<Self> {
        Ok(Self(ObjectId::from_bytes(bytes)?.to_bytes()))
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum InodeKind {
    RegularFile = 1,
    Directory = 2,
    Symlink = 3,
}

impl TryFrom<u8> for InodeKind {
    type Error = crate::CoreError;
    fn try_from(value: u8) -> CoreResult<Self> {
        match value {
            1 => Ok(Self::RegularFile),
            2 => Ok(Self::Directory),
            3 => Ok(Self::Symlink),
            _ => Err(crate::CoreError::InvalidRecord("inode kind")),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InodeRecordV1 {
    pub kind: InodeKind,
    pub namespace_ref_count: u64,
    pub content_root: ObjectId,
    pub metadata_root: ObjectId,
}

impl InodeRecordV1 {
    pub fn validate(self, is_root: bool) -> CoreResult<()> {
        let valid = if is_root {
            self.kind == InodeKind::Directory && self.namespace_ref_count == 0
        } else {
            self.namespace_ref_count >= 1
                && (self.kind == InodeKind::RegularFile || self.namespace_ref_count == 1)
        };
        if valid {
            Ok(())
        } else {
            Err(crate::CoreError::InvalidRecord("namespace ref count"))
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InodeTableRoot(pub ObjectId);

pub struct GeneratedInodeTable(InodeTableRoot);

impl GeneratedInodeTable {
    pub const fn root(&self) -> InodeTableRoot {
        self.0
    }

    pub const fn into_root(self) -> InodeTableRoot {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InodeTableCounters {
    pub nodes_read: u64,
    pub nodes_created: u64,
}

#[derive(Clone, Copy)]
struct Summary {
    id: ObjectId,
    min: InodeId,
    max: InodeId,
    entries: u64,
    level: u8,
}

struct ValidatedNode {
    node: InodeTableNodeV1,
    summary: Summary,
}

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InodeTableDiff {
    pub inode: InodeId,
    pub before: Option<ObjectId>,
    pub after: Option<ObjectId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InodeTableMergeConflict {
    pub inode: InodeId,
    pub source: Option<ObjectId>,
    pub destination: Option<ObjectId>,
}

/// Merges `base -> source` onto `destination` one changed inode at a time.
/// The two cursors retain only tree frontiers; equal subtrees and unchanged
/// suffixes are never collected. ponytail: adjacent changed keys path-copy
/// separately; batch them only if merge throughput becomes a measured limit.
pub fn merge_inode_tables<S: ObjectStore>(
    store: &mut S,
    base: InodeTableRoot,
    source: InodeTableRoot,
    destination: InodeTableRoot,
) -> CoreResult<
    std::result::Result<
        (InodeTableRoot, InodeTableCounters, NamespaceCounters),
        InodeTableMergeConflict,
    >,
> {
    let mut counters = InodeTableCounters::default();
    let mut namespace = NamespaceCounters::default();
    if source == base || source == destination {
        return Ok(Ok((destination, counters, namespace)));
    }
    if destination == base {
        return Ok(Ok((source, counters, namespace)));
    }
    let mut merged = destination;
    if let Some(conflict) = merge_inode_node_diffs(
        store,
        base.0,
        source.0,
        true,
        &mut merged,
        &mut counters,
        &mut namespace,
    )? {
        return Ok(Err(conflict));
    }
    Ok(Ok((merged, counters, namespace)))
}

fn merge_inode_node_diffs<S: ObjectStore>(
    store: &mut S,
    base: ObjectId,
    source: ObjectId,
    root: bool,
    merged: &mut InodeTableRoot,
    counters: &mut InodeTableCounters,
    namespace: &mut NamespaceCounters,
) -> CoreResult<Option<InodeTableMergeConflict>> {
    if base == source {
        return Ok(None);
    }
    let base_node = load_shallow(store, base, root, None, counters)?;
    let source_node = load_shallow(store, source, root, None, counters)?;
    match (base_node.node, source_node.node) {
        (InodeTableNodeV1::Leaf(base), InodeTableNodeV1::Leaf(source)) => {
            let mut conflict = None;
            merge_inode_entries(
                base.into_iter().map(Ok),
                source.into_iter().map(Ok),
                &mut |change| {
                    if conflict.is_none() {
                        conflict =
                            apply_inode_table_change(store, merged, change, counters, namespace)?;
                    }
                    Ok(())
                },
            )?;
            Ok(conflict)
        }
        (
            InodeTableNodeV1::Branch {
                level: base_level,
                children: base_children,
                ..
            },
            InodeTableNodeV1::Branch {
                level: source_level,
                children: source_children,
                ..
            },
        ) if base_level == source_level
            && base_children.len() == source_children.len()
            && base_children
                .iter()
                .zip(&source_children)
                .all(|(base, source)| base.0 == source.0) =>
        {
            for ((_, base_child), (_, source_child)) in
                base_children.into_iter().zip(source_children)
            {
                if let Some(conflict) = merge_inode_node_diffs(
                    store,
                    base_child,
                    source_child,
                    false,
                    merged,
                    counters,
                    namespace,
                )? {
                    return Ok(Some(conflict));
                }
            }
            Ok(None)
        }
        _ => {
            let mut diffs = StreamingInodeDiff::new(base, source, root);
            while let Some(change) = diffs.next(store, counters)? {
                if let Some(conflict) =
                    apply_inode_table_change(store, merged, change, counters, namespace)?
                {
                    return Ok(Some(conflict));
                }
            }
            Ok(None)
        }
    }
}

fn apply_inode_table_change<S: ObjectStore>(
    store: &mut S,
    merged: &mut InodeTableRoot,
    change: InodeTableDiff,
    counters: &mut InodeTableCounters,
    namespace: &mut NamespaceCounters,
) -> CoreResult<Option<InodeTableMergeConflict>> {
    let mut lookup = InodeTableCounters::default();
    let destination = inode_table_lookup(store, *merged, change.inode, &mut lookup)?;
    add_inode_counters(counters, lookup)?;
    let selected = if destination == change.before {
        change.after
    } else if destination == change.after {
        if concurrent_namespace_identity_change(store, change.before, change.after)? {
            return Ok(Some(InodeTableMergeConflict {
                inode: change.inode,
                source: change.after,
                destination,
            }));
        }
        destination
    } else if let (Some(base), Some(source), Some(destination)) =
        (change.before, change.after, destination)
    {
        match merge_inode_records(store, base, source, destination, namespace)? {
            Some(record) => Some(record),
            None => {
                return Ok(Some(InodeTableMergeConflict {
                    inode: change.inode,
                    source: change.after,
                    destination: Some(destination),
                }));
            }
        }
    } else {
        return Ok(Some(InodeTableMergeConflict {
            inode: change.inode,
            source: change.after,
            destination,
        }));
    };
    let Some(selected) = selected else {
        if destination.is_none() {
            return Ok(None);
        }
        let (next, _, changed) = inode_table_remove(store, *merged, change.inode)?;
        add_inode_counters(counters, changed)?;
        *merged = next;
        return Ok(None);
    };
    if Some(selected) == destination {
        return Ok(None);
    }
    let (next, changed) = inode_table_upsert(store, *merged, change.inode, selected)?;
    add_inode_counters(counters, changed)?;
    *merged = next;
    Ok(None)
}

fn concurrent_namespace_identity_change<S: ObjectStore>(
    store: &S,
    before: Option<ObjectId>,
    after: Option<ObjectId>,
) -> CoreResult<bool> {
    match (before, after) {
        (None, Some(_)) => Ok(true),
        (Some(before), Some(after)) => {
            let before = store.with_authenticated_canonical(before, decode_inode_record)?;
            let after = store.with_authenticated_canonical(after, decode_inode_record)?;
            Ok(before.namespace_ref_count != after.namespace_ref_count)
        }
        _ => Ok(false),
    }
}

fn merge_inode_records<S: ObjectStore>(
    store: &mut S,
    base: ObjectId,
    source: ObjectId,
    destination: ObjectId,
    namespace: &mut NamespaceCounters,
) -> CoreResult<Option<ObjectId>> {
    let base = store.with_authenticated_canonical(base, decode_inode_record)?;
    let source = store.with_authenticated_canonical(source, decode_inode_record)?;
    let destination = store.with_authenticated_canonical(destination, decode_inode_record)?;
    let Some(kind) = merge_field(base.kind, source.kind, destination.kind) else {
        return Ok(None);
    };
    let Some(namespace_ref_count) = merge_namespace_ref_count(
        base.namespace_ref_count,
        source.namespace_ref_count,
        destination.namespace_ref_count,
    ) else {
        return Ok(None);
    };
    let content_root = match merge_field(
        base.content_root,
        source.content_root,
        destination.content_root,
    ) {
        Some(root) => root,
        None if [base.kind, source.kind, destination.kind]
            .into_iter()
            .all(|kind| kind == InodeKind::Directory) =>
        {
            let Some((root, counters)) = merge_directory_roots(
                store,
                DirectoryStateRoot(base.content_root),
                DirectoryStateRoot(source.content_root),
                DirectoryStateRoot(destination.content_root),
            )?
            else {
                return Ok(None);
            };
            add_namespace_merge_counters(namespace, counters)?;
            root.0
        }
        None => return Ok(None),
    };
    let metadata_root = match merge_field(
        base.metadata_root,
        source.metadata_root,
        destination.metadata_root,
    ) {
        Some(root) => root,
        None => {
            let Some(root) = merge_metadata_roots(
                store,
                base.metadata_root,
                source.metadata_root,
                destination.metadata_root,
            )?
            else {
                return Ok(None);
            };
            root
        }
    };
    if namespace_ref_count == 0 && kind != InodeKind::Directory
        || namespace_ref_count != 1 && kind == InodeKind::Symlink
        || namespace_ref_count == 0 && kind == InodeKind::RegularFile
    {
        return Ok(None);
    }
    store
        .put(&encode_inode_record(InodeRecordV1 {
            kind,
            namespace_ref_count,
            content_root,
            metadata_root,
        })?)
        .map(Some)
}

fn add_namespace_merge_counters(
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

fn merge_field<T: Copy + Eq>(base: T, source: T, destination: T) -> Option<T> {
    if source == base || source == destination {
        Some(destination)
    } else if destination == base {
        Some(source)
    } else {
        None
    }
}

fn merge_namespace_ref_count(base: u64, source: u64, destination: u64) -> Option<u64> {
    if source == base {
        Some(destination)
    } else if destination == base {
        Some(source)
    } else {
        // Absolute link counts cannot distinguish identical link changes from
        // disjoint additions. Refuse concurrent count changes instead of
        // publishing a count that disagrees with the merged directories.
        None
    }
}

fn add_inode_counters(
    target: &mut InodeTableCounters,
    source: InodeTableCounters,
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

/// Streams changed inode-table entries while pruning equal persistent
/// subtrees by identity. Unequal heights or page partitions fall back to a
/// bounded leaf cursor rather than collecting the table.
pub fn diff_inode_table_entries<S: ObjectRead>(
    store: &S,
    old: InodeTableRoot,
    new: InodeTableRoot,
    mut visitor: impl FnMut(InodeTableDiff) -> CoreResult<()>,
) -> CoreResult<InodeTableCounters> {
    let mut counters = InodeTableCounters::default();
    if old == new {
        return Ok(counters);
    }
    diff_inode_nodes(store, old.0, new.0, true, &mut counters, &mut visitor)?;
    Ok(counters)
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

struct DeferredInodes<'a, S> {
    store: &'a mut S,
    nodes: BTreeMap<ObjectId, Vec<u8>>,
}

impl<'a, S: ObjectStore> DeferredInodes<'a, S> {
    fn new(store: &'a mut S) -> Self {
        Self {
            store,
            nodes: BTreeMap::new(),
        }
    }

    fn commit(&mut self, root: ObjectId) -> CoreResult<u64> {
        let mut committed = BTreeSet::new();
        self.commit_node(root, &mut committed)?;
        u64::try_from(committed.len()).map_err(|_| CoreError::LengthOverflow)
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

fn diff_inode_nodes<S: ObjectRead>(
    store: &S,
    old: ObjectId,
    new: ObjectId,
    root: bool,
    counters: &mut InodeTableCounters,
    visitor: &mut impl FnMut(InodeTableDiff) -> CoreResult<()>,
) -> CoreResult<()> {
    if old == new {
        return Ok(());
    }
    let old_node = load_shallow(store, old, root, None, counters)?;
    let new_node = load_shallow(store, new, root, None, counters)?;
    match (&old_node.node, &new_node.node) {
        (InodeTableNodeV1::Leaf(old), InodeTableNodeV1::Leaf(new)) => merge_inode_entries(
            old.iter().copied().map(Ok),
            new.iter().copied().map(Ok),
            visitor,
        ),
        (
            InodeTableNodeV1::Branch {
                level: old_level,
                children: old_children,
                ..
            },
            InodeTableNodeV1::Branch {
                level: new_level,
                children: new_children,
                ..
            },
        ) if old_level == new_level
            && old_children.len() == new_children.len()
            && old_children
                .iter()
                .zip(new_children)
                .all(|(old, new)| old.0 == new.0) =>
        {
            for ((_, old_child), (_, new_child)) in old_children.iter().zip(new_children) {
                diff_inode_nodes(store, *old_child, *new_child, false, counters, visitor)?;
            }
            Ok(())
        }
        _ => {
            let mut old_counters = InodeTableCounters::default();
            let mut new_counters = InodeTableCounters::default();
            let result = merge_inode_entries(
                InodeEntryCursor::new(store, old, root, &mut old_counters),
                InodeEntryCursor::new(store, new, root, &mut new_counters),
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

fn merge_inode_entries(
    old: impl Iterator<Item = CoreResult<(InodeId, ObjectId)>>,
    new: impl Iterator<Item = CoreResult<(InodeId, ObjectId)>>,
    visitor: &mut impl FnMut(InodeTableDiff) -> CoreResult<()>,
) -> CoreResult<()> {
    let mut old = old;
    let mut new = new;
    let mut old_entry = old.next().transpose()?;
    let mut new_entry = new.next().transpose()?;
    loop {
        match (old_entry, new_entry) {
            (None, None) => return Ok(()),
            (Some((old_key, before)), Some((new_key, after))) if old_key == new_key => {
                if before != after {
                    visitor(InodeTableDiff {
                        inode: old_key,
                        before: Some(before),
                        after: Some(after),
                    })?;
                }
                old_entry = old.next().transpose()?;
                new_entry = new.next().transpose()?;
            }
            (Some((old_key, before)), Some((new_key, after))) if old_key < new_key => {
                visitor(InodeTableDiff {
                    inode: old_key,
                    before: Some(before),
                    after: None,
                })?;
                old_entry = old.next().transpose()?;
                new_entry = Some((new_key, after));
            }
            (Some((old_key, before)), Some((new_key, after))) => {
                visitor(InodeTableDiff {
                    inode: new_key,
                    before: None,
                    after: Some(after),
                })?;
                old_entry = Some((old_key, before));
                new_entry = new.next().transpose()?;
            }
            (Some((old_key, before)), None) => {
                visitor(InodeTableDiff {
                    inode: old_key,
                    before: Some(before),
                    after: None,
                })?;
                old_entry = old.next().transpose()?;
            }
            (None, Some((new_key, after))) => {
                visitor(InodeTableDiff {
                    inode: new_key,
                    before: None,
                    after: Some(after),
                })?;
                new_entry = new.next().transpose()?;
            }
        }
    }
}

struct StreamingInodeDiff {
    base: StreamingInodeCursor,
    source: StreamingInodeCursor,
    base_entry: Option<(InodeId, ObjectId)>,
    source_entry: Option<(InodeId, ObjectId)>,
    initialized: bool,
}

impl StreamingInodeDiff {
    fn new(base: ObjectId, source: ObjectId, root: bool) -> Self {
        Self {
            base: StreamingInodeCursor::new(base, root),
            source: StreamingInodeCursor::new(source, root),
            base_entry: None,
            source_entry: None,
            initialized: false,
        }
    }

    fn next<S: ObjectRead>(
        &mut self,
        store: &S,
        counters: &mut InodeTableCounters,
    ) -> CoreResult<Option<InodeTableDiff>> {
        if !self.initialized {
            self.base_entry = self.base.next(store, counters)?;
            self.source_entry = self.source.next(store, counters)?;
            self.initialized = true;
        }
        loop {
            match (self.base_entry, self.source_entry) {
                (None, None) => return Ok(None),
                (Some((base_inode, before)), Some((source_inode, after)))
                    if base_inode == source_inode =>
                {
                    self.base_entry = self.base.next(store, counters)?;
                    self.source_entry = self.source.next(store, counters)?;
                    if before != after {
                        return Ok(Some(InodeTableDiff {
                            inode: base_inode,
                            before: Some(before),
                            after: Some(after),
                        }));
                    }
                }
                (Some((base_inode, before)), Some((source_inode, _)))
                    if base_inode < source_inode =>
                {
                    self.base_entry = self.base.next(store, counters)?;
                    return Ok(Some(InodeTableDiff {
                        inode: base_inode,
                        before: Some(before),
                        after: None,
                    }));
                }
                (Some(_), Some((source_inode, after))) => {
                    self.source_entry = self.source.next(store, counters)?;
                    return Ok(Some(InodeTableDiff {
                        inode: source_inode,
                        before: None,
                        after: Some(after),
                    }));
                }
                (Some((base_inode, before)), None) => {
                    self.base_entry = self.base.next(store, counters)?;
                    return Ok(Some(InodeTableDiff {
                        inode: base_inode,
                        before: Some(before),
                        after: None,
                    }));
                }
                (None, Some((source_inode, after))) => {
                    self.source_entry = self.source.next(store, counters)?;
                    return Ok(Some(InodeTableDiff {
                        inode: source_inode,
                        before: None,
                        after: Some(after),
                    }));
                }
            }
        }
    }
}

struct StreamingInodeCursor {
    stack: Vec<InodeWalkItem>,
    leaf: std::vec::IntoIter<(InodeId, ObjectId)>,
}

impl StreamingInodeCursor {
    fn new(root: ObjectId, root_context: bool) -> Self {
        Self {
            stack: vec![InodeWalkItem {
                id: root,
                root: root_context,
                expected_level: None,
                expected_max: None,
            }],
            leaf: Vec::new().into_iter(),
        }
    }

    fn next<S: ObjectRead>(
        &mut self,
        store: &S,
        counters: &mut InodeTableCounters,
    ) -> CoreResult<Option<(InodeId, ObjectId)>> {
        loop {
            if let Some(entry) = self.leaf.next() {
                return Ok(Some(entry));
            }
            let Some(item) = self.stack.pop() else {
                return Ok(None);
            };
            let loaded = load_shallow(store, item.id, item.root, None, counters)?;
            if item
                .expected_level
                .is_some_and(|level| loaded.summary.level != level)
                || item
                    .expected_max
                    .is_some_and(|maximum| loaded.summary.max != maximum)
            {
                return Err(CoreError::InvalidRecord("inode child summary"));
            }
            match loaded.node {
                InodeTableNodeV1::Leaf(entries) => self.leaf = entries.into_iter(),
                InodeTableNodeV1::Branch {
                    level, children, ..
                } => {
                    let child_level = level
                        .checked_sub(1)
                        .ok_or(CoreError::InvalidRecord("inode child summary"))?;
                    self.stack
                        .extend(
                            children
                                .into_iter()
                                .rev()
                                .map(|(maximum, id)| InodeWalkItem {
                                    id,
                                    root: false,
                                    expected_level: Some(child_level),
                                    expected_max: Some(maximum),
                                }),
                        );
                }
            }
        }
    }
}

struct InodeWalkItem {
    id: ObjectId,
    root: bool,
    expected_level: Option<u8>,
    expected_max: Option<InodeId>,
}

struct InodeEntryCursor<'a, S> {
    store: &'a S,
    stack: Vec<InodeWalkItem>,
    leaf: std::vec::IntoIter<(InodeId, ObjectId)>,
    counters: &'a mut InodeTableCounters,
}

impl<'a, S> InodeEntryCursor<'a, S> {
    fn new(
        store: &'a S,
        root: ObjectId,
        root_context: bool,
        counters: &'a mut InodeTableCounters,
    ) -> Self {
        Self {
            store,
            stack: vec![InodeWalkItem {
                id: root,
                root: root_context,
                expected_level: None,
                expected_max: None,
            }],
            leaf: Vec::new().into_iter(),
            counters,
        }
    }
}

impl<S: ObjectRead> Iterator for InodeEntryCursor<'_, S> {
    type Item = CoreResult<(InodeId, ObjectId)>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(entry) = self.leaf.next() {
                return Some(Ok(entry));
            }
            let item = self.stack.pop()?;
            let loaded = match load_shallow(self.store, item.id, item.root, None, self.counters) {
                Ok(loaded) => loaded,
                Err(error) => return Some(Err(error)),
            };
            if item
                .expected_level
                .is_some_and(|level| loaded.summary.level != level)
                || item
                    .expected_max
                    .is_some_and(|maximum| loaded.summary.max != maximum)
            {
                return Some(Err(CoreError::InvalidRecord("inode child summary")));
            }
            match loaded.node {
                InodeTableNodeV1::Leaf(entries) => self.leaf = entries.into_iter(),
                InodeTableNodeV1::Branch {
                    level, children, ..
                } => {
                    let Some(child_level) = level.checked_sub(1) else {
                        return Some(Err(CoreError::InvalidRecord("inode child summary")));
                    };
                    self.stack
                        .extend(
                            children
                                .into_iter()
                                .rev()
                                .map(|(maximum, id)| InodeWalkItem {
                                    id,
                                    root: false,
                                    expected_level: Some(child_level),
                                    expected_max: Some(maximum),
                                }),
                        );
                }
            }
        }
    }
}

fn load_shallow<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    root: bool,
    expected: Option<&Summary>,
    counters: &mut InodeTableCounters,
) -> CoreResult<ValidatedNode> {
    let node = load(store, id, counters)?;
    let summary = inode_node_shape(id, &node, root)?;
    if expected.is_some_and(|expected| {
        summary.max != expected.max
            || summary.entries != expected.entries
            || summary.level != expected.level
    }) {
        return Err(CoreError::InvalidRecord("inode child summary"));
    }
    Ok(ValidatedNode { node, summary })
}

fn inode_node_shape(id: ObjectId, node: &InodeTableNodeV1, root: bool) -> CoreResult<Summary> {
    let (max, entries, level, count) = match node {
        InodeTableNodeV1::Leaf(entries) => (
            entries
                .last()
                .ok_or(CoreError::InvalidRecord("empty inode table"))?
                .0,
            entries.len() as u64,
            0,
            entries.len(),
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
            children.len(),
        ),
    };
    if (root && matches!(node, InodeTableNodeV1::Branch { .. }) && count < 2)
        || (!root && !(64..=127).contains(&count))
    {
        return Err(CoreError::NonCanonicalPagePartition);
    }
    Ok(Summary {
        id,
        min: inode_node_min(node)?,
        max,
        entries,
        level,
    })
}

fn inode_node_min(node: &InodeTableNodeV1) -> CoreResult<InodeId> {
    match node {
        InodeTableNodeV1::Leaf(entries) => entries
            .first()
            .map(|entry| entry.0)
            .ok_or(CoreError::InvalidRecord("empty inode table")),
        InodeTableNodeV1::Branch { children, .. } => children
            .first()
            .map(|entry| entry.0)
            .ok_or(CoreError::InvalidRecord("empty inode branch")),
    }
}

fn load<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    counters: &mut InodeTableCounters,
) -> CoreResult<InodeTableNodeV1> {
    counters.nodes_read = counters
        .nodes_read
        .checked_add(1)
        .ok_or(CoreError::LengthOverflow)?;
    store.with_authenticated_canonical(id, decode_inode_table_node)
}
