use super::apply::{CandidateRoot, InodeMutation};
use super::resolve::{namespace, LogicalCounters};
use super::{apply_inode_mutations, ContentConflict};
use crate::file::rope::{read_all, state, FileStateRoot, RopeCounters};
use crate::object::access::{ObjectRead, ObjectStore};
use crate::object::ContentDigestWriter;
use crate::tree::directory::codec::encode_namespace_root;
use crate::tree::directory::{directory_page_after, DirectoryStateRoot, NamespaceCounters};
use crate::tree::inode::codec::decode_inode_record;
use crate::tree::inode::{
    inode_table_lookup, merge_inode_tables, InodeId, InodeKind, InodeRecordV1, InodeTableCounters,
    InodeTableDiff, InodeTableRoot,
};
use crate::tree::NamespaceRootV1;
use crate::{CanonicalName, CoreError, CoreResult, ObjectId};
use std::collections::{BTreeMap, VecDeque};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MergeConflict {
    pub inode: InodeId,
    pub source: Option<ObjectId>,
    pub destination: Option<ObjectId>,
}

/// Applies the exact three-root rule for one inode-table change. Root-level
/// traversal remains streaming in `diff`; callers never need a complete inode
/// inventory to classify a change.
pub fn merge_inode_change(
    source: InodeTableDiff,
    destination: Option<ObjectId>,
) -> CoreResult<std::result::Result<Option<ObjectId>, MergeConflict>> {
    if destination == source.before {
        Ok(Ok(source.after))
    } else if destination == source.after {
        Ok(Ok(destination))
    } else {
        Ok(Err(MergeConflict {
            inode: source.inode,
            source: source.after,
            destination,
        }))
    }
}

pub fn merge_roots<S: ObjectStore>(
    store: &mut S,
    base: ObjectId,
    source: ObjectId,
    destination: ObjectId,
) -> CoreResult<std::result::Result<CandidateRoot, MergeConflict>> {
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
    let merged = merge_inode_tables(
        store,
        InodeTableRoot(base_namespace.inode_table_root),
        InodeTableRoot(source_namespace.inode_table_root),
        InodeTableRoot(destination_namespace.inode_table_root),
    )?;
    let (table, inode_table, namespace) = match merged {
        Ok(merged) => merged,
        Err(conflict) => {
            return Ok(Err(MergeConflict {
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

pub enum ThreeWayOutcome {
    Clean(ObjectId),
    Conflict(ContentConflict),
}

pub fn three_way<S: ObjectStore>(
    store: &mut S,
    base_root: ObjectId,
    current_root: ObjectId,
    candidate_root: ObjectId,
) -> CoreResult<ThreeWayOutcome> {
    if current_root == base_root {
        return Ok(ThreeWayOutcome::Clean(candidate_root));
    }
    if candidate_root == base_root || current_root == candidate_root {
        return Ok(ThreeWayOutcome::Clean(current_root));
    }
    let mut candidate = candidate_root;
    let mut current = current_root;
    let mut digests = BTreeMap::new();
    loop {
        match merge_roots(store, base_root, candidate, current)? {
            Ok(merged) => return Ok(ThreeWayOutcome::Clean(merged.root())),
            Err(conflict) => {
                let base_record = loaded_record(store, base_root, conflict.inode)?;
                let current_record = loaded_record(store, current, conflict.inode)?;
                let candidate_record = loaded_record(store, candidate, conflict.inode)?;
                let resolution = if semantic_eq(store, current_record, base_record, &mut digests)? {
                    Some((false, base_record))
                } else if semantic_eq(store, candidate_record, base_record, &mut digests)? {
                    Some((true, base_record))
                } else if semantic_eq(store, candidate_record, current_record, &mut digests)? {
                    Some((true, current_record))
                } else {
                    None
                };
                let Some((rewrite_candidate, replacement)) = resolution else {
                    return Ok(ThreeWayOutcome::Conflict(ContentConflict {
                        path: first_conflict_path(store, base_root, current, candidate)?,
                        base: base_record.map(|(id, _)| id),
                        current: current_record.map(|(id, _)| id),
                        candidate: candidate_record.map(|(id, _)| id),
                    }));
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
                *root = apply_inode_mutations(store, *root, mutations)?.root();
            }
        }
    }
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
