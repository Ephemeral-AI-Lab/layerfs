use super::codec::{decode_inode_record, encode_inode_record, InodeTableNodeV1};
use super::cursor::{load_shallow, InodeEntryCursor, StreamingInodeDiff};
use super::{
    inode_table_lookup, inode_table_remove, inode_table_upsert, InodeId, InodeKind, InodeRecordV1,
    InodeTableCounters, InodeTableRoot,
};
use crate::file::rope::{ObjectRead, ObjectStore};
use crate::tree::directory::{merge_directory_roots, DirectoryStateRoot, NamespaceCounters};
use crate::tree::metadata::merge_metadata_roots;
use crate::{CoreError, CoreResult, ObjectId};

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
