use crate::{Conflict, CoreReader, DeferredObjectStore, ObjectBuffer, ObjectSource, Result};
use layerfs_core::content::rope::{read_all, state, FileStateRoot, RopeCounters};
use layerfs_core::identity::ContentDigestWriter;
use layerfs_core::inode::{
    inode_table_lookup, InodeId, InodeKind, InodeRecordV1, InodeTableCounters, InodeTableRoot,
};
use layerfs_core::logical;
use layerfs_core::namespace::{directory_page_after, DirectoryStateRoot, NamespaceCounters};
use layerfs_core::namespace_codec::decode_inode_record;
use layerfs_core::object::access::ObjectRead;
use layerfs_core::{CanonicalName, ObjectId};
use std::collections::{BTreeMap, VecDeque};

pub struct CleanMerge {
    pub root_id: ObjectId,
    pub objects: DeferredObjectStore,
}

pub enum ThreeWayOutcome {
    Clean(CleanMerge),
    Conflict(Conflict),
}

pub fn three_way(
    source: &dyn ObjectSource,
    base_root: ObjectId,
    current_root: ObjectId,
    candidate_root: ObjectId,
) -> Result<ThreeWayOutcome> {
    if current_root == base_root {
        return clean(candidate_root);
    }
    if candidate_root == base_root || current_root == candidate_root {
        return clean(current_root);
    }
    let mut store = ObjectBuffer::new(source)?;
    let mut candidate = candidate_root;
    let mut current = current_root;
    let mut digests = BTreeMap::new();
    loop {
        match logical::merge_roots(&mut store, base_root, candidate, current)? {
            Ok(merged) => {
                let built = store.finish(merged.root(), 0)?;
                return Ok(ThreeWayOutcome::Clean(CleanMerge {
                    root_id: built.root_id,
                    objects: built.objects,
                }));
            }
            Err(conflict) => {
                let reader = CoreReader(&store);
                let base_record = loaded_record(&reader, base_root, conflict.inode)?;
                let current_record = loaded_record(&reader, current, conflict.inode)?;
                let candidate_record = loaded_record(&reader, candidate, conflict.inode)?;
                let resolution = if semantic_eq(&reader, current_record, base_record, &mut digests)?
                {
                    Some((false, base_record))
                } else if semantic_eq(&reader, candidate_record, base_record, &mut digests)? {
                    Some((true, base_record))
                } else if semantic_eq(&reader, candidate_record, current_record, &mut digests)? {
                    Some((true, current_record))
                } else {
                    None
                };
                let Some((rewrite_candidate, replacement)) = resolution else {
                    let reader = CoreReader(&store);
                    return Ok(ThreeWayOutcome::Conflict(Conflict {
                        path: first_conflict_path(&reader, base_root, current, candidate)?,
                        base: base_record.map(|(id, _)| id),
                        current: current_record.map(|(id, _)| id),
                        candidate: candidate_record.map(|(id, _)| id),
                    }));
                };
                let mutations = match replacement {
                    Some((_, record)) => vec![logical::InodeMutation::Upsert {
                        inode: conflict.inode,
                        record,
                    }],
                    None => vec![logical::InodeMutation::Remove {
                        inode: conflict.inode,
                    }],
                };
                let root = if rewrite_candidate {
                    &mut candidate
                } else {
                    &mut current
                };
                *root = logical::apply_inode_mutations(&mut store, *root, mutations)?.root();
            }
        }
    }
}

fn loaded_record(
    store: &CoreReader<'_>,
    root: ObjectId,
    inode: InodeId,
) -> Result<Option<LoadedRecord>> {
    let namespace = logical::namespace(store, root)?;
    record(store, TreeEntry::new(namespace.inode_table_root, inode))
}

fn semantic_eq(
    store: &CoreReader<'_>,
    left: Option<LoadedRecord>,
    right: Option<LoadedRecord>,
    digests: &mut BTreeMap<ObjectId, [u8; 32]>,
) -> Result<bool> {
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

fn file_digest(
    store: &CoreReader<'_>,
    root: ObjectId,
    digests: &mut BTreeMap<ObjectId, [u8; 32]>,
) -> Result<[u8; 32]> {
    if let Some(digest) = digests.get(&root) {
        return Ok(*digest);
    }
    let mut writer = ContentDigestWriter::new();
    read_all(store, FileStateRoot(root), &mut writer)?;
    let digest = writer.finish();
    digests.insert(root, digest);
    Ok(digest)
}

fn clean(root_id: ObjectId) -> Result<ThreeWayOutcome> {
    Ok(ThreeWayOutcome::Clean(CleanMerge {
        root_id,
        objects: DeferredObjectStore::new()?,
    }))
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

fn first_conflict_path(
    store: &CoreReader<'_>,
    base: ObjectId,
    current: ObjectId,
    candidate: ObjectId,
) -> Result<String> {
    let base = logical::namespace(store, base)?;
    let current = logical::namespace(store, current)?;
    let candidate = logical::namespace(store, candidate)?;
    Ok(directory_conflict(
        store,
        TreeEntry::new(base.inode_table_root, base.root_directory_inode),
        TreeEntry::new(current.inode_table_root, current.root_directory_inode),
        TreeEntry::new(candidate.inode_table_root, candidate.root_directory_inode),
        "",
    )?
    .unwrap_or_default())
}

fn directory_conflict(
    store: &CoreReader<'_>,
    base: TreeEntry,
    current: TreeEntry,
    candidate: TreeEntry,
    prefix: &str,
) -> Result<Option<String>> {
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

fn record(store: &CoreReader<'_>, entry: TreeEntry) -> Result<Option<LoadedRecord>> {
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

    fn next(&mut self, store: &CoreReader<'_>) -> Result<Option<(CanonicalName, InodeId)>> {
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

fn take(
    item: &mut Option<(CanonicalName, InodeId)>,
    name: &CanonicalName,
    cursor: &mut DirectoryCursor,
    store: &CoreReader<'_>,
) -> Result<Option<InodeId>> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{apply_changes, empty_root, Change, StorageError};
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct Memory(BTreeMap<ObjectId, Vec<u8>>);

    impl ObjectSource for Memory {
        fn read_object(&self, id: ObjectId) -> Result<Vec<u8>> {
            self.0
                .get(&id)
                .cloned()
                .ok_or(StorageError::MissingBaseData)
        }
    }

    fn admit(memory: &mut Memory, built: crate::BuiltRoot) -> ObjectId {
        built
            .objects
            .visit_batches(&mut |objects, _| {
                for object in objects {
                    memory.0.insert(object.id, object.bytes.clone());
                }
                Ok(())
            })
            .unwrap();
        built.root_id
    }

    #[test]
    fn first_conflict_is_lexicographic() {
        let mut memory = Memory::default();
        let base = admit(&mut memory, empty_root([1; 32]).unwrap());
        let left = apply_changes(
            &memory,
            base,
            &[
                Change::Write {
                    path: "b".into(),
                    bytes: b"left".to_vec(),
                    mode: 0o644,
                },
                Change::Write {
                    path: "a".into(),
                    bytes: b"left".to_vec(),
                    mode: 0o644,
                },
            ],
            [2; 32],
        )
        .unwrap();
        let left = admit(&mut memory, left);
        let right = apply_changes(
            &memory,
            base,
            &[
                Change::Write {
                    path: "b".into(),
                    bytes: b"right".to_vec(),
                    mode: 0o644,
                },
                Change::Write {
                    path: "a".into(),
                    bytes: b"right".to_vec(),
                    mode: 0o644,
                },
            ],
            [3; 32],
        )
        .unwrap();
        let right = admit(&mut memory, right);
        let ThreeWayOutcome::Conflict(conflict) = three_way(&memory, base, left, right).unwrap()
        else {
            panic!()
        };
        assert_eq!(conflict.path, "a");
    }

    #[test]
    fn equal_regular_file_bytes_with_different_roots_do_not_conflict() {
        let mut memory = Memory::default();
        let empty = admit(&mut memory, empty_root([10; 32]).unwrap());
        let built = apply_changes(
            &memory,
            empty,
            &[Change::Write {
                path: "file".into(),
                bytes: vec![b'a'; 80_000],
                mode: 0o644,
            }],
            [11; 32],
        )
        .unwrap();
        let base = admit(&mut memory, built);
        let built = apply_changes(
            &memory,
            base,
            &[
                Change::Splice {
                    path: "file".into(),
                    start: 1,
                    delete_len: 1,
                    replacement: vec![b'b'],
                },
                Change::Splice {
                    path: "file".into(),
                    start: 1,
                    delete_len: 1,
                    replacement: vec![b'a'],
                },
            ],
            [12; 32],
        )
        .unwrap();
        let current = admit(&mut memory, built);
        let built = apply_changes(
            &memory,
            base,
            &[Change::Splice {
                path: "file".into(),
                start: 79_999,
                delete_len: 1,
                replacement: vec![b'c'],
            }],
            [13; 32],
        )
        .unwrap();
        let candidate = admit(&mut memory, built);
        assert_ne!(base, current);
        let ThreeWayOutcome::Clean(merged) = three_way(&memory, base, current, candidate).unwrap()
        else {
            panic!("representation-only equality must not conflict")
        };
        assert_eq!(merged.root_id, candidate);
    }
}
