use super::apply::{apply_directory_changes, CandidateRoot, InodeMutation};
use super::apply_inode_mutations;
use super::resolve::{namespace, resolve_parent, LogicalCounters};
use crate::file::rope::{read_all, state, FileStateRoot, RopeCounters};
use crate::object::access::{ObjectRead, ObjectStore};
use crate::object::ContentDigestWriter;
use crate::tree::directory::codec::encode_namespace_root;
use crate::tree::directory::{
    directory_lookup, directory_page_after, DirectoryStateRoot, NamespaceCounters,
};
use crate::tree::inode::codec::{decode_inode_record, encode_inode_record};
use crate::tree::inode::{
    inode_table_lookup, inode_table_remove, inode_table_upsert, reconcile_inode_tables, InodeId,
    InodeKind, InodeRecordV1, InodeTableCounters, InodeTableDiff, InodeTableRoot,
};
use crate::tree::NamespaceRootV1;
use crate::{CanonicalName, CanonicalPath, CoreError, CoreResult, ObjectId};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReconcileCollision {
    pub inode: InodeId,
    pub source: Option<ObjectId>,
    pub destination: Option<ObjectId>,
}

/// Applies the exact three-root rule for one inode-table change. Root-level
/// traversal remains streaming in `diff`; callers never need a complete inode
/// inventory to classify a change.
pub fn reconcile_inode_change(
    source: InodeTableDiff,
    destination: Option<ObjectId>,
) -> CoreResult<std::result::Result<Option<ObjectId>, ReconcileCollision>> {
    if destination == source.before {
        Ok(Ok(source.after))
    } else if destination == source.after {
        Ok(Ok(destination))
    } else {
        Ok(Err(ReconcileCollision {
            inode: source.inode,
            source: source.after,
            destination,
        }))
    }
}

pub fn reconcile_roots<S: ObjectStore>(
    store: &mut S,
    base: ObjectId,
    source: ObjectId,
    destination: ObjectId,
) -> CoreResult<std::result::Result<CandidateRoot, ReconcileCollision>> {
    let base_namespace = namespace(store, base)?;
    let source_namespace = namespace(store, source)?;
    let destination_namespace = namespace(store, destination)?;
    if [source_namespace, destination_namespace]
        .into_iter()
        .any(|namespace| {
            namespace.profile_id != base_namespace.profile_id
                || namespace.root_directory_inode != base_namespace.root_directory_inode
        })
    {
        return Err(CoreError::InvalidRecord("namespace identity mismatch"));
    }
    let merged = reconcile_inode_tables(
        store,
        InodeTableRoot(base_namespace.inode_table_root),
        InodeTableRoot(source_namespace.inode_table_root),
        InodeTableRoot(destination_namespace.inode_table_root),
    )?;
    let (table, inode_table, namespace) = match merged {
        Ok(merged) => merged,
        Err(conflict) => {
            return Ok(Err(ReconcileCollision {
                inode: conflict.inode,
                source: conflict.source,
                destination: conflict.destination,
            }))
        }
    };
    let root = store.put(&encode_namespace_root(NamespaceRootV1 {
        inode_table_root: table.0,
        ..destination_namespace
    })?)?;
    Ok(Ok(CandidateRoot::new(
        destination,
        root,
        LogicalCounters {
            inode_table,
            namespace,
            ..LogicalCounters::default()
        },
    )))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReconcileConflictKind {
    Content,
    Type,
    Directory,
    HardLink,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconcileConflict {
    pub inode: InodeId,
    pub kind: ReconcileConflictKind,
    pub affected_paths: Vec<CanonicalPath>,
    pub base: Option<ObjectId>,
    pub branch: Option<ObjectId>,
    pub layer: Option<ObjectId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconcileResult {
    pub root_id: ObjectId,
    pub conflicts: Vec<ReconcileConflict>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReconcileChoice {
    Branch,
    Layer,
    WorkingTree,
}

/// Replaces the selected namespace paths with their exact source snapshot
/// entries. Source inode identities and complete directory subtrees are
/// preserved, which is required for type changes and hard-link choices.
pub fn replace_paths_from_snapshot<S: ObjectStore>(
    store: &mut S,
    destination_root: ObjectId,
    source_root: ObjectId,
    paths: &[CanonicalPath],
) -> CoreResult<ObjectId> {
    let destination = namespace(store, destination_root)?;
    let source = namespace(store, source_root)?;
    if destination.profile_id != source.profile_id
        || destination.root_directory_inode != source.root_directory_inode
    {
        return Err(CoreError::InvalidRecord("namespace identity mismatch"));
    }

    let mut selected = paths.to_vec();
    selected.sort();
    selected.dedup();
    let mut roots = Vec::<CanonicalPath>::new();
    for path in selected {
        if roots.iter().any(|parent| path_is_within(&path, parent)) {
            continue;
        }
        roots.push(path);
    }
    if roots.iter().any(CanonicalPath::is_root) {
        return Ok(source_root);
    }

    let source_table = InodeTableRoot(source.inode_table_root);
    let mut root = destination_root;
    let mut applied = Vec::<CanonicalPath>::new();
    let mut removed = Vec::new();
    let mut desired_roots = Vec::new();
    for original in roots {
        if applied
            .iter()
            .any(|parent| path_is_within(&original, parent))
        {
            continue;
        }
        let mut path = original;
        let (parent, name) = loop {
            if path.is_root() {
                return Ok(source_root);
            }
            let mut counters = LogicalCounters::default();
            match resolve_parent(store, root, &path, &mut counters) {
                Ok(resolved) => break resolved,
                Err(CoreError::MissingObject | CoreError::InvalidRecord(_)) => {
                    path = parent_path(&path)?;
                }
                Err(error) => return Err(error),
            }
        };
        if applied.iter().any(|parent| path_is_within(&path, parent)) {
            continue;
        }
        let existing = lookup_path_inode(store, root, &path)?;
        let desired = lookup_path_inode(store, source_root, &path)?;
        let current_namespace = namespace(store, root)?;
        let (directory, namespace_counters) = apply_directory_changes(
            store,
            DirectoryStateRoot(parent.record.content_root),
            [(name, desired)],
        )?;
        let mut counters = LogicalCounters::default();
        merge_namespace_counters(&mut counters.namespace, namespace_counters)?;

        let mut table = InodeTableRoot(current_namespace.inode_table_root);
        let parent_record = store.put(&encode_inode_record(InodeRecordV1 {
            content_root: directory.0,
            ..parent.record
        })?)?;
        let (next, inode_counters) = inode_table_upsert(store, table, parent.inode, parent_record)?;
        merge_inode_counters(&mut counters.inode_table, inode_counters)?;
        table = next;
        if existing != desired {
            removed.extend(existing);
        }
        if let Some(inode) = desired {
            desired_roots.push(inode);
        }
        root = store.put(&encode_namespace_root(NamespaceRootV1 {
            inode_table_root: table.0,
            ..current_namespace
        })?)?;
        applied.push(path);
    }

    let current_namespace = namespace(store, root)?;
    let mut table = InodeTableRoot(current_namespace.inode_table_root);
    let mut active = BTreeSet::new();
    for inode in removed {
        release_namespace_reference(store, inode, &mut table, &mut active)?;
    }
    for inode in desired_roots {
        copy_snapshot_inode(store, source_table, inode, &mut table, &mut active)?;
    }
    store.put(&encode_namespace_root(NamespaceRootV1 {
        inode_table_root: table.0,
        ..current_namespace
    })?)
}

pub fn replace_conflict_from_snapshot<S: ObjectStore>(
    store: &mut S,
    destination_root: ObjectId,
    source_root: ObjectId,
    conflict: &ReconcileConflict,
) -> CoreResult<ObjectId> {
    let mut paths = conflict
        .affected_paths
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if conflict.kind == ReconcileConflictKind::HardLink {
        collect_inode_paths(store, destination_root, conflict.inode, &mut paths)?;
        collect_inode_paths(store, source_root, conflict.inode, &mut paths)?;
    }
    replace_paths_from_snapshot(
        store,
        destination_root,
        source_root,
        &paths.into_iter().collect::<Vec<_>>(),
    )
}

fn path_is_within(path: &CanonicalPath, parent: &CanonicalPath) -> bool {
    parent.is_root()
        || path == parent
        || path
            .as_bytes()
            .strip_prefix(parent.as_bytes())
            .is_some_and(|suffix| suffix.first() == Some(&b'/'))
}

fn parent_path(path: &CanonicalPath) -> CoreResult<CanonicalPath> {
    let bytes = path.as_bytes();
    CanonicalPath::from_bytes(
        bytes
            .iter()
            .rposition(|byte| *byte == b'/')
            .map_or(&[][..], |separator| &bytes[..separator]),
    )
}

fn lookup_path_inode<S: ObjectRead>(
    store: &S,
    root: ObjectId,
    path: &CanonicalPath,
) -> CoreResult<Option<InodeId>> {
    let namespace = namespace(store, root)?;
    let table = InodeTableRoot(namespace.inode_table_root);
    let mut inode = namespace.root_directory_inode;
    for component in path.components() {
        let Some((_, parent)) = record(store, TreeEntry::new(table.0, inode))? else {
            return Err(CoreError::MissingObject);
        };
        if parent.kind != InodeKind::Directory {
            return Ok(None);
        }
        let Some(child) = directory_lookup(
            store,
            DirectoryStateRoot(parent.content_root),
            &CanonicalName::from_bytes(component)?,
            &mut NamespaceCounters::default(),
        )?
        else {
            return Ok(None);
        };
        inode = child;
    }
    Ok(Some(inode))
}

fn copy_snapshot_inode<S: ObjectStore>(
    store: &mut S,
    source_table: InodeTableRoot,
    inode: InodeId,
    destination_table: &mut InodeTableRoot,
    active: &mut BTreeSet<InodeId>,
) -> CoreResult<()> {
    if !active.insert(inode) {
        return Err(CoreError::InvalidRecord("directory cycle"));
    }
    let record_id = inode_table_lookup(
        store,
        source_table,
        inode,
        &mut InodeTableCounters::default(),
    )?
    .ok_or(CoreError::MissingObject)?;
    let record = store.with_authenticated_canonical(record_id, decode_inode_record)?;
    if record.kind == InodeKind::Directory {
        let mut cursor = DirectoryCursor::new(record.content_root);
        while let Some((_, child)) = cursor.next(store)? {
            copy_snapshot_inode(store, source_table, child, destination_table, active)?;
        }
    }
    let (next, _) = inode_table_upsert(store, *destination_table, inode, record_id)?;
    *destination_table = next;
    active.remove(&inode);
    Ok(())
}

fn release_namespace_reference<S: ObjectStore>(
    store: &mut S,
    inode: InodeId,
    table: &mut InodeTableRoot,
    active: &mut BTreeSet<InodeId>,
) -> CoreResult<()> {
    if !active.insert(inode) {
        return Err(CoreError::InvalidRecord("directory cycle"));
    }
    let record_id = inode_table_lookup(store, *table, inode, &mut InodeTableCounters::default())?
        .ok_or(CoreError::MissingObject)?;
    let record = store.with_authenticated_canonical(record_id, decode_inode_record)?;
    if record.namespace_ref_count > 1 {
        let record_id = store.put(&encode_inode_record(InodeRecordV1 {
            namespace_ref_count: record.namespace_ref_count - 1,
            ..record
        })?)?;
        let (next, _) = inode_table_upsert(store, *table, inode, record_id)?;
        *table = next;
        active.remove(&inode);
        return Ok(());
    }
    if record.kind == InodeKind::Directory {
        let mut cursor = DirectoryCursor::new(record.content_root);
        while let Some((_, child)) = cursor.next(store)? {
            release_namespace_reference(store, child, table, active)?;
        }
    }
    let (next, _, _) = inode_table_remove(store, *table, inode)?;
    *table = next;
    active.remove(&inode);
    Ok(())
}

fn merge_namespace_counters(
    total: &mut NamespaceCounters,
    value: NamespaceCounters,
) -> CoreResult<()> {
    total.nodes_read = total
        .nodes_read
        .checked_add(value.nodes_read)
        .ok_or(CoreError::LengthOverflow)?;
    total.nodes_created = total
        .nodes_created
        .checked_add(value.nodes_created)
        .ok_or(CoreError::LengthOverflow)?;
    Ok(())
}

fn merge_inode_counters(
    total: &mut InodeTableCounters,
    value: InodeTableCounters,
) -> CoreResult<()> {
    total.nodes_read = total
        .nodes_read
        .checked_add(value.nodes_read)
        .ok_or(CoreError::LengthOverflow)?;
    total.nodes_created = total
        .nodes_created
        .checked_add(value.nodes_created)
        .ok_or(CoreError::LengthOverflow)?;
    Ok(())
}

pub fn reconcile<S: ObjectStore>(
    store: &mut S,
    base_root: ObjectId,
    branch_root: ObjectId,
    layer_root: ObjectId,
) -> CoreResult<ReconcileResult> {
    reconcile_with(store, base_root, branch_root, layer_root, |_| None)
}

pub fn reconcile_with<S: ObjectStore>(
    store: &mut S,
    base_root: ObjectId,
    branch_root: ObjectId,
    layer_root: ObjectId,
    mut choice: impl FnMut(&ReconcileConflict) -> Option<ReconcileChoice>,
) -> CoreResult<ReconcileResult> {
    if layer_root == base_root {
        return Ok(ReconcileResult {
            root_id: branch_root,
            conflicts: Vec::new(),
        });
    }
    if branch_root == base_root || layer_root == branch_root {
        return Ok(ReconcileResult {
            root_id: layer_root,
            conflicts: Vec::new(),
        });
    }
    let mut candidate = branch_root;
    let mut current = layer_root;
    let mut digests = BTreeMap::new();
    let mut conflicts = Vec::new();
    let mut seen = BTreeSet::new();
    loop {
        if !seen.insert((candidate, current)) {
            return Err(CoreError::DeltaConflict);
        }
        match reconcile_roots(store, base_root, candidate, current)? {
            Ok(merged) => {
                let merged = merged.root();
                let root_id = if logical_eq(store, merged, branch_root)? {
                    branch_root
                } else if logical_eq(store, merged, layer_root)? {
                    layer_root
                } else {
                    merged
                };
                return Ok(ReconcileResult { root_id, conflicts });
            }
            Err(conflict) => {
                let base_record = loaded_record(store, base_root, conflict.inode)?;
                let current_record = loaded_record(store, current, conflict.inode)?;
                let candidate_record = loaded_record(store, candidate, conflict.inode)?;
                let resolution = if semantic_eq(store, current_record, base_record, &mut digests)? {
                    Some((false, base_record))
                } else if semantic_eq(store, candidate_record, base_record, &mut digests)? {
                    Some((true, base_record))
                } else if !parallel_link_change(base_record, candidate_record, current_record)
                    && semantic_eq(store, candidate_record, current_record, &mut digests)?
                {
                    Some((true, current_record))
                } else {
                    None
                };
                let (rewrite_candidate, replacement) = if let Some(resolution) = resolution {
                    resolution
                } else {
                    let (kind, affected_paths) = conflict_details(
                        store,
                        [base_root, current, candidate],
                        conflict.inode,
                        [base_record, current_record, candidate_record],
                    )?;
                    let conflict = ReconcileConflict {
                        inode: conflict.inode,
                        kind,
                        affected_paths,
                        base: base_record.map(|(id, _)| id),
                        branch: candidate_record.map(|(id, _)| id),
                        layer: current_record.map(|(id, _)| id),
                    };
                    let selected = choice(&conflict);
                    let select_layer = matches!(selected, Some(ReconcileChoice::Layer));
                    let (target, source) = if select_layer {
                        (&mut candidate, current)
                    } else {
                        (&mut current, candidate)
                    };
                    if selected.is_none() {
                        conflicts.push(conflict.clone());
                    }
                    let previous = *target;
                    *target = replace_paths_from_snapshot(
                        store,
                        previous,
                        source,
                        &conflict.affected_paths,
                    )?;
                    if *target == previous {
                        return Ok(ReconcileResult {
                            root_id: source,
                            conflicts,
                        });
                    }
                    continue;
                };
                let mutations = match replacement {
                    Some((_, record)) => vec![InodeMutation::Upsert {
                        inode: conflict.inode,
                        record,
                    }],
                    None => vec![InodeMutation::Remove {
                        inode: conflict.inode,
                    }],
                };
                let root = if rewrite_candidate {
                    &mut candidate
                } else {
                    &mut current
                };
                let previous = *root;
                *root = apply_inode_mutations(store, previous, mutations)?.root();
                if *root == previous {
                    return Ok(ReconcileResult {
                        root_id: if rewrite_candidate {
                            current
                        } else {
                            candidate
                        },
                        conflicts,
                    });
                }
            }
        }
    }
}

fn logical_eq(store: &impl ObjectRead, left: ObjectId, right: ObjectId) -> CoreResult<bool> {
    if left == right {
        return Ok(true);
    }
    let mut different = false;
    super::diff_roots(store, left, right, |_| {
        different = true;
        Ok(())
    })?;
    Ok(!different)
}

fn conflict_kind(
    base: Option<LoadedRecord>,
    branch: Option<LoadedRecord>,
    layer: Option<LoadedRecord>,
) -> ReconcileConflictKind {
    let records = [base, branch, layer];
    let mut kinds = records.iter().flatten().map(|(_, record)| record.kind);
    let first_kind = kinds.next();
    let type_conflict = kinds.any(|kind| Some(kind) != first_kind);
    if type_conflict {
        ReconcileConflictKind::Type
    } else if records.iter().any(Option::is_none)
        || records
            .iter()
            .flatten()
            .any(|(_, record)| record.kind == InodeKind::Directory)
    {
        ReconcileConflictKind::Directory
    } else if records
        .iter()
        .flatten()
        .map(|(_, record)| record.namespace_ref_count)
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        > 1
    {
        ReconcileConflictKind::HardLink
    } else {
        ReconcileConflictKind::Content
    }
}

fn parallel_link_change(
    base: Option<LoadedRecord>,
    branch: Option<LoadedRecord>,
    layer: Option<LoadedRecord>,
) -> bool {
    matches!((base, branch, layer), (Some((_, base)), Some((_, branch)), Some((_, layer)))
        if branch.namespace_ref_count == layer.namespace_ref_count
            && branch.namespace_ref_count != base.namespace_ref_count)
}

fn loaded_record<S: ObjectRead>(
    store: &S,
    root: ObjectId,
    inode: InodeId,
) -> CoreResult<Option<LoadedRecord>> {
    let namespace = namespace(store, root)?;
    record(store, TreeEntry::new(namespace.inode_table_root, inode))
}

fn semantic_eq<S: ObjectRead>(
    store: &S,
    left: Option<LoadedRecord>,
    right: Option<LoadedRecord>,
    digests: &mut BTreeMap<ObjectId, [u8; 32]>,
) -> CoreResult<bool> {
    let (Some((left_id, left)), Some((right_id, right))) = (left, right) else {
        return Ok(left.is_none() && right.is_none());
    };
    if left_id == right_id {
        return Ok(true);
    }
    if left.kind != InodeKind::RegularFile
        || right.kind != InodeKind::RegularFile
        || left.namespace_ref_count != right.namespace_ref_count
        || left.metadata_root != right.metadata_root
    {
        return Ok(false);
    }
    let mut counters = RopeCounters::default();
    let left_state = state(store, FileStateRoot(left.content_root), &mut counters)?;
    let right_state = state(store, FileStateRoot(right.content_root), &mut counters)?;
    if left_state.logical_len != right_state.logical_len {
        return Ok(false);
    }
    Ok(file_digest(store, left.content_root, digests)?
        == file_digest(store, right.content_root, digests)?)
}

fn file_digest<S: ObjectRead>(
    store: &S,
    root: ObjectId,
    digests: &mut BTreeMap<ObjectId, [u8; 32]>,
) -> CoreResult<[u8; 32]> {
    if let Some(digest) = digests.get(&root) {
        return Ok(*digest);
    }
    let mut writer = ContentDigestWriter::new();
    read_all(store, FileStateRoot(root), &mut writer)?;
    let digest = writer.finish();
    digests.insert(root, digest);
    Ok(digest)
}

#[derive(Clone, Copy)]
struct TreeEntry {
    table: InodeTableRoot,
    inode: Option<InodeId>,
}

impl TreeEntry {
    fn new(table: ObjectId, inode: InodeId) -> Self {
        Self {
            table: InodeTableRoot(table),
            inode: Some(inode),
        }
    }

    fn child(self, inode: Option<InodeId>) -> Self {
        Self { inode, ..self }
    }
}

type LoadedRecord = (ObjectId, InodeRecordV1);

fn conflict_details<S: ObjectRead>(
    store: &S,
    roots: [ObjectId; 3],
    inode: InodeId,
    records: [Option<LoadedRecord>; 3],
) -> CoreResult<(ReconcileConflictKind, Vec<CanonicalPath>)> {
    if records
        .iter()
        .flatten()
        .all(|(_, record)| record.kind == InodeKind::Directory)
    {
        let path = CanonicalPath::new(&first_conflict_path(store, roots[0], roots[1], roots[2])?)?;
        let described = [
            path_record(store, roots[0], &path)?,
            path_record(store, roots[1], &path)?,
            path_record(store, roots[2], &path)?,
        ];
        return Ok((
            conflict_kind(described[0], described[2], described[1]),
            vec![path],
        ));
    }
    let mut paths = BTreeSet::new();
    for root in roots {
        collect_inode_paths(store, root, inode, &mut paths)?;
    }
    if paths.is_empty() {
        paths.insert(CanonicalPath::root());
    }
    Ok((
        conflict_kind(records[0], records[2], records[1]),
        paths.into_iter().collect(),
    ))
}

fn path_record<S: ObjectRead>(
    store: &S,
    root: ObjectId,
    path: &CanonicalPath,
) -> CoreResult<Option<LoadedRecord>> {
    let namespace = namespace(store, root)?;
    let table = InodeTableRoot(namespace.inode_table_root);
    let mut inode = namespace.root_directory_inode;
    for component in path.components() {
        let (_, parent) =
            record(store, TreeEntry::new(table.0, inode))?.ok_or(CoreError::MissingObject)?;
        if parent.kind != InodeKind::Directory {
            return Ok(None);
        }
        let mut counters = NamespaceCounters::default();
        let Some(child) = directory_lookup(
            store,
            DirectoryStateRoot(parent.content_root),
            &CanonicalName::from_bytes(component)?,
            &mut counters,
        )?
        else {
            return Ok(None);
        };
        inode = child;
    }
    record(store, TreeEntry::new(table.0, inode))
}

fn collect_inode_paths<S: ObjectRead>(
    store: &S,
    root: ObjectId,
    target: InodeId,
    output: &mut BTreeSet<CanonicalPath>,
) -> CoreResult<()> {
    let namespace = namespace(store, root)?;
    let table = InodeTableRoot(namespace.inode_table_root);
    if namespace.root_directory_inode == target {
        output.insert(CanonicalPath::root());
    }
    collect_directory_paths(
        store,
        table,
        namespace.root_directory_inode,
        "",
        target,
        output,
    )
}

fn collect_directory_paths<S: ObjectRead>(
    store: &S,
    table: InodeTableRoot,
    directory: InodeId,
    prefix: &str,
    target: InodeId,
    output: &mut BTreeSet<CanonicalPath>,
) -> CoreResult<()> {
    let (_, directory_record) =
        record(store, TreeEntry::new(table.0, directory))?.ok_or(CoreError::MissingObject)?;
    if directory_record.kind != InodeKind::Directory {
        return Ok(());
    }
    let mut cursor = DirectoryCursor::new(directory_record.content_root);
    while let Some((name, child)) = cursor.next(store)? {
        let path = if prefix.is_empty() {
            name.as_str().to_owned()
        } else {
            format!("{prefix}/{}", name.as_str())
        };
        if child == target {
            output.insert(CanonicalPath::new(&path)?);
        }
        let (_, child_record) =
            record(store, TreeEntry::new(table.0, child))?.ok_or(CoreError::MissingObject)?;
        if child_record.kind == InodeKind::Directory {
            collect_directory_paths(store, table, child, &path, target, output)?;
        }
    }
    Ok(())
}

fn first_conflict_path<S: ObjectRead>(
    store: &S,
    base: ObjectId,
    current: ObjectId,
    candidate: ObjectId,
) -> CoreResult<String> {
    let base = namespace(store, base)?;
    let current = namespace(store, current)?;
    let candidate = namespace(store, candidate)?;
    Ok(directory_conflict(
        store,
        TreeEntry::new(base.inode_table_root, base.root_directory_inode),
        TreeEntry::new(current.inode_table_root, current.root_directory_inode),
        TreeEntry::new(candidate.inode_table_root, candidate.root_directory_inode),
        "",
    )?
    .unwrap_or_default())
}

fn directory_conflict<S: ObjectRead>(
    store: &S,
    base: TreeEntry,
    current: TreeEntry,
    candidate: TreeEntry,
    prefix: &str,
) -> CoreResult<Option<String>> {
    let Some((_, base_record)) = record(store, base)? else {
        return Ok(Some(prefix.to_owned()));
    };
    let Some((_, current_record)) = record(store, current)? else {
        return Ok(Some(prefix.to_owned()));
    };
    let Some((_, candidate_record)) = record(store, candidate)? else {
        return Ok(Some(prefix.to_owned()));
    };
    if [base_record, current_record, candidate_record]
        .iter()
        .any(|record| record.kind != InodeKind::Directory)
    {
        return Ok(Some(prefix.to_owned()));
    }
    let mut base_cursor = DirectoryCursor::new(base_record.content_root);
    let mut current_cursor = DirectoryCursor::new(current_record.content_root);
    let mut candidate_cursor = DirectoryCursor::new(candidate_record.content_root);
    let mut base_item = base_cursor.next(store)?;
    let mut current_item = current_cursor.next(store)?;
    let mut candidate_item = candidate_cursor.next(store)?;
    loop {
        let Some(name) = [
            base_item.as_ref(),
            current_item.as_ref(),
            candidate_item.as_ref(),
        ]
        .into_iter()
        .flatten()
        .map(|(name, _)| name)
        .min()
        .cloned() else {
            return Ok(None);
        };
        let base_child = base.child(take(&mut base_item, &name, &mut base_cursor, store)?);
        let current_child =
            current.child(take(&mut current_item, &name, &mut current_cursor, store)?);
        let candidate_child = candidate.child(take(
            &mut candidate_item,
            &name,
            &mut candidate_cursor,
            store,
        )?);
        let base_record = record(store, base_child)?;
        let current_record = record(store, current_child)?;
        let candidate_record = record(store, candidate_child)?;
        if !record_eq(current_record, base_record)
            && !record_eq(candidate_record, base_record)
            && !record_eq(current_record, candidate_record)
        {
            let path = if prefix.is_empty() {
                name.as_str().to_owned()
            } else {
                format!("{prefix}/{}", name.as_str())
            };
            let all_directories = [base_record, current_record, candidate_record]
                .into_iter()
                .all(|record| {
                    record.is_some_and(|(_, record)| record.kind == InodeKind::Directory)
                });
            if all_directories {
                if let Some(nested) =
                    directory_conflict(store, base_child, current_child, candidate_child, &path)?
                {
                    return Ok(Some(nested));
                }
            }
            return Ok(Some(path));
        }
    }
}

fn record<S: ObjectRead>(store: &S, entry: TreeEntry) -> CoreResult<Option<LoadedRecord>> {
    let Some(inode) = entry.inode else {
        return Ok(None);
    };
    let Some(id) = inode_table_lookup(
        store,
        entry.table,
        inode,
        &mut InodeTableCounters::default(),
    )?
    else {
        return Ok(None);
    };
    Ok(Some((
        id,
        store.with_authenticated_canonical(id, decode_inode_record)?,
    )))
}

fn record_eq(left: Option<LoadedRecord>, right: Option<LoadedRecord>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some((left, _)), Some((right, _))) => left == right,
        _ => false,
    }
}

struct DirectoryCursor {
    root: DirectoryStateRoot,
    after: Option<CanonicalName>,
    buffered: VecDeque<(CanonicalName, InodeId)>,
    done: bool,
}

impl DirectoryCursor {
    fn new(root: ObjectId) -> Self {
        Self {
            root: DirectoryStateRoot(root),
            after: None,
            buffered: VecDeque::new(),
            done: false,
        }
    }

    fn next<S: ObjectRead>(&mut self, store: &S) -> CoreResult<Option<(CanonicalName, InodeId)>> {
        if let Some(value) = self.buffered.pop_front() {
            return Ok(Some(value));
        }
        if self.done {
            return Ok(None);
        }
        let page = directory_page_after(
            store,
            self.root,
            self.after.as_ref(),
            128,
            256 * 1024,
            &mut NamespaceCounters::default(),
        )?;
        self.after = page.continuation;
        self.done = self.after.is_none();
        self.buffered = page.entries.into();
        Ok(self.buffered.pop_front())
    }
}

fn take<S: ObjectRead>(
    item: &mut Option<(CanonicalName, InodeId)>,
    name: &CanonicalName,
    cursor: &mut DirectoryCursor,
    store: &S,
) -> CoreResult<Option<InodeId>> {
    if item
        .as_ref()
        .is_some_and(|(candidate, _)| candidate == name)
    {
        let inode = item.take().map(|(_, inode)| inode);
        *item = cursor.next(store)?;
        Ok(inode)
    } else {
        Ok(None)
    }
}
