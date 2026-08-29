use super::resolve::{namespace, resolve, resolve_parent, LogicalCounters};
use crate::file::rope::{build, replace, FileStateRoot};
use crate::object::access::ObjectStore;
use crate::tree::directory::codec::{encode_namespace_root, encode_symlink};
use crate::tree::directory::SymlinkStateV1;
use crate::tree::directory::{
    directory_insert, directory_lookup, directory_page_after, directory_remove, directory_rename,
    empty_directory, DeferredDirectory, DirectoryStateRoot,
};
use crate::tree::inode::codec::{decode_inode_record, encode_inode_record};
use crate::tree::inode::{
    inode_table_lookup, inode_table_remove, inode_table_upsert, DeferredInodes, InodeId, InodeKind,
    InodeRecordV1, InodeTableRoot,
};
use crate::tree::NamespaceRootV1;
use crate::{CanonicalPath, CoreError, CoreResult, ObjectId};
use std::io::Read;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateRoot {
    parent_root: ObjectId,
    root: ObjectId,
    counters: LogicalCounters,
}

impl CandidateRoot {
    pub(super) const fn new(
        parent_root: ObjectId,
        root: ObjectId,
        counters: LogicalCounters,
    ) -> Self {
        Self {
            parent_root,
            root,
            counters,
        }
    }

    pub const fn parent_root(&self) -> ObjectId {
        self.parent_root
    }

    pub const fn root(&self) -> ObjectId {
        self.root
    }

    pub const fn counters(&self) -> LogicalCounters {
        self.counters
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InodeMutation {
    Upsert {
        inode: InodeId,
        record: InodeRecordV1,
    },
    Remove {
        inode: InodeId,
    },
}

pub fn replace_range<S: ObjectStore, R: Read>(
    store: &mut S,
    root: ObjectId,
    path: &CanonicalPath,
    start: u64,
    delete_len: u64,
    replacement: R,
) -> CoreResult<CandidateRoot> {
    replace_range_with_metadata(store, root, path, start, delete_len, replacement, None)
}

pub fn replace_range_with_metadata<S: ObjectStore, R: Read>(
    store: &mut S,
    root: ObjectId,
    path: &CanonicalPath,
    start: u64,
    delete_len: u64,
    replacement: R,
    metadata_root: Option<ObjectId>,
) -> CoreResult<CandidateRoot> {
    let mut counters = LogicalCounters::default();
    let resolved = resolve(store, root, path, &mut counters)?;
    if resolved.record.kind != InodeKind::RegularFile {
        return Err(CoreError::WrongLogicalRole);
    }
    let (content, rope) = replace(
        store,
        FileStateRoot(resolved.record.content_root),
        start,
        delete_len,
        replacement,
    )?;
    counters.rope = rope;
    let NamespaceRootV1 {
        profile_id,
        root_directory_inode,
        inode_table_root,
    } = namespace(store, root)?;
    let record = store.put(&encode_inode_record(InodeRecordV1 {
        content_root: content.0,
        metadata_root: metadata_root.unwrap_or(resolved.record.metadata_root),
        ..resolved.record
    })?)?;
    let (table, inode) = inode_table_upsert(
        store,
        InodeTableRoot(inode_table_root),
        resolved.inode,
        record,
    )?;
    counters.inode_table.nodes_read = counters
        .inode_table
        .nodes_read
        .checked_add(inode.nodes_read)
        .ok_or(CoreError::LengthOverflow)?;
    counters.inode_table.nodes_created = counters
        .inode_table
        .nodes_created
        .checked_add(inode.nodes_created)
        .ok_or(CoreError::LengthOverflow)?;
    let canonical = encode_namespace_root(NamespaceRootV1 {
        profile_id,
        root_directory_inode,
        inode_table_root: table.0,
    })?;
    Ok(CandidateRoot {
        parent_root: root,
        root: store.put(&canonical)?,
        counters,
    })
}

pub fn rename(
    store: &mut impl ObjectStore,
    root: ObjectId,
    from: &CanonicalPath,
    to: &CanonicalPath,
    source_parent_metadata_root: ObjectId,
    target_parent_metadata_root: ObjectId,
) -> CoreResult<CandidateRoot> {
    let mut counters = LogicalCounters::default();
    let namespace = namespace(store, root)?;
    let (source, source_name) = resolve_parent(store, root, from, &mut counters)?;
    let (target, target_name) = resolve_parent(store, root, to, &mut counters)?;
    let mut table = InodeTableRoot(namespace.inode_table_root);
    if source.inode == target.inode {
        let (directory, visits) = directory_rename(
            store,
            DirectoryStateRoot(source.record.content_root),
            &source_name,
            target_name,
        )?;
        merge_namespace(&mut counters, visits)?;
        let record = store.put(&encode_inode_record(InodeRecordV1 {
            content_root: directory.0,
            metadata_root: source_parent_metadata_root,
            ..source.record
        })?)?;
        let (next, visits) = inode_table_upsert(store, table, source.inode, record)?;
        merge_inode(&mut counters, visits)?;
        table = next;
    } else {
        let (source_directory, moved, visits) = directory_remove(
            store,
            DirectoryStateRoot(source.record.content_root),
            &source_name,
        )?;
        merge_namespace(&mut counters, visits)?;
        let (target_directory, visits) = directory_insert(
            store,
            DirectoryStateRoot(target.record.content_root),
            target_name,
            moved,
        )?;
        merge_namespace(&mut counters, visits)?;
        let source_record = store.put(&encode_inode_record(InodeRecordV1 {
            content_root: source_directory.0,
            metadata_root: source_parent_metadata_root,
            ..source.record
        })?)?;
        let (next, visits) = inode_table_upsert(store, table, source.inode, source_record)?;
        merge_inode(&mut counters, visits)?;
        let target_record = store.put(&encode_inode_record(InodeRecordV1 {
            content_root: target_directory.0,
            metadata_root: target_parent_metadata_root,
            ..target.record
        })?)?;
        let (next, visits) = inode_table_upsert(store, next, target.inode, target_record)?;
        merge_inode(&mut counters, visits)?;
        table = next;
    }
    let candidate = store.put(&encode_namespace_root(NamespaceRootV1 {
        inode_table_root: table.0,
        ..namespace
    })?)?;
    Ok(CandidateRoot {
        parent_root: root,
        root: candidate,
        counters,
    })
}

pub fn apply_inode_mutations(
    store: &mut impl ObjectStore,
    root: ObjectId,
    mutations: impl IntoIterator<Item = InodeMutation>,
) -> CoreResult<CandidateRoot> {
    Ok(apply_inode_mutations_deferred(store, root, mutations)?.value)
}

struct DeferredBatchResult<T> {
    value: T,
    peak_bytes: usize,
    prunes: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StructuralBatchCounters {
    pub deferred_peak_bytes: u64,
    pub deferred_prunes: u64,
}

fn apply_inode_mutations_deferred(
    store: &mut impl ObjectStore,
    root: ObjectId,
    mutations: impl IntoIterator<Item = InodeMutation>,
) -> CoreResult<DeferredBatchResult<CandidateRoot>> {
    let mut counters = LogicalCounters::default();
    let namespace = namespace(store, root)?;
    let mut table = InodeTableRoot(namespace.inode_table_root);
    let mut deferred = DeferredInodes::new(store);
    for mutation in mutations {
        match mutation {
            InodeMutation::Upsert { inode, record } => {
                let record = deferred.put_persistent(&encode_inode_record(record)?)?;
                let (next, visits) = inode_table_upsert(&mut deferred, table, inode, record)?;
                merge_inode(&mut counters, visits)?;
                table = next;
            }
            InodeMutation::Remove { inode } => {
                let (next, _, visits) = inode_table_remove(&mut deferred, table, inode)?;
                merge_inode(&mut counters, visits)?;
                table = next;
            }
        }
        deferred.prune_to(table.0)?;
    }
    let peak_bytes = deferred.peak_charged_bytes();
    let prunes = deferred.prunes();
    counters.structural_deferred_peak_bytes = peak_bytes as u64;
    counters.structural_deferred_prunes = prunes;
    counters.inode_table.nodes_created = deferred.commit(table.0)?;
    let candidate = deferred.put_persistent(&encode_namespace_root(NamespaceRootV1 {
        inode_table_root: table.0,
        ..namespace
    })?)?;
    Ok(DeferredBatchResult {
        value: CandidateRoot {
            parent_root: root,
            root: candidate,
            counters,
        },
        peak_bytes,
        prunes,
    })
}

pub fn apply_directory_changes(
    store: &mut impl ObjectStore,
    root: DirectoryStateRoot,
    changes: impl IntoIterator<Item = (crate::CanonicalName, Option<InodeId>)>,
) -> CoreResult<(
    DirectoryStateRoot,
    crate::tree::directory::NamespaceCounters,
)> {
    let (root, counters, _) = apply_directory_changes_observed(store, root, changes)?;
    Ok((root, counters))
}

pub fn apply_directory_changes_observed(
    store: &mut impl ObjectStore,
    root: DirectoryStateRoot,
    changes: impl IntoIterator<Item = (crate::CanonicalName, Option<InodeId>)>,
) -> CoreResult<(
    DirectoryStateRoot,
    crate::tree::directory::NamespaceCounters,
    StructuralBatchCounters,
)> {
    let result = apply_directory_changes_deferred(store, root, changes)?;
    Ok((
        result.value.0,
        result.value.1,
        StructuralBatchCounters {
            deferred_peak_bytes: result.peak_bytes as u64,
            deferred_prunes: result.prunes,
        },
    ))
}

fn apply_directory_changes_deferred(
    store: &mut impl ObjectStore,
    mut root: DirectoryStateRoot,
    changes: impl IntoIterator<Item = (crate::CanonicalName, Option<InodeId>)>,
) -> CoreResult<
    DeferredBatchResult<(
        DirectoryStateRoot,
        crate::tree::directory::NamespaceCounters,
    )>,
> {
    let mut counters = crate::tree::directory::NamespaceCounters::default();
    let mut deferred = DeferredDirectory::new(store);
    for (name, desired) in changes {
        if directory_lookup(&deferred, root, &name, &mut counters)?.is_some() {
            let (next, _, visits) = directory_remove(&mut deferred, root, &name)?;
            merge_namespace_counters(&mut counters, visits)?;
            root = next;
        }
        if let Some(inode) = desired {
            let (next, visits) = directory_insert(&mut deferred, root, name, inode)?;
            merge_namespace_counters(&mut counters, visits)?;
            root = next;
        }
        deferred.prune_to(root)?;
    }
    let peak_bytes = deferred.peak_charged_bytes();
    let prunes = deferred.prunes();
    counters.nodes_created = deferred.commit(root)?;
    Ok(DeferredBatchResult {
        value: (root, counters),
        peak_bytes,
        prunes,
    })
}

pub fn symlink_content(store: &mut impl ObjectStore, target: Vec<u8>) -> CoreResult<ObjectId> {
    store.put(&encode_symlink(&SymlinkStateV1::new(target)?)?)
}

pub fn create_directory(
    store: &mut impl ObjectStore,
    root: ObjectId,
    path: &CanonicalPath,
    inode: InodeId,
    metadata_root: ObjectId,
) -> CoreResult<CandidateRoot> {
    let content_root = empty_directory(store)?.0;
    create_inode(
        store,
        root,
        path,
        inode,
        InodeRecordV1 {
            kind: InodeKind::Directory,
            namespace_ref_count: 1,
            content_root,
            metadata_root,
        },
    )
}

pub fn create_symlink(
    store: &mut impl ObjectStore,
    root: ObjectId,
    path: &CanonicalPath,
    inode: InodeId,
    target: Vec<u8>,
    metadata_root: ObjectId,
) -> CoreResult<CandidateRoot> {
    let content_root = symlink_content(store, target)?;
    create_inode(
        store,
        root,
        path,
        inode,
        InodeRecordV1 {
            kind: InodeKind::Symlink,
            namespace_ref_count: 1,
            content_root,
            metadata_root,
        },
    )
}

pub fn hard_link(
    store: &mut impl ObjectStore,
    root: ObjectId,
    source: &CanonicalPath,
    target: &CanonicalPath,
) -> CoreResult<CandidateRoot> {
    let mut counters = LogicalCounters::default();
    let namespace = namespace(store, root)?;
    let source = resolve(store, root, source, &mut counters)?;
    if source.record.kind == InodeKind::Directory {
        return Err(CoreError::WrongLogicalRole);
    }
    let (parent, name) = resolve_parent(store, root, target, &mut counters)?;
    if directory_lookup(
        store,
        DirectoryStateRoot(parent.record.content_root),
        &name,
        &mut counters.namespace,
    )?
    .is_some()
    {
        return Err(CoreError::NameCollision);
    }
    let (directory, visits) = directory_insert(
        store,
        DirectoryStateRoot(parent.record.content_root),
        name,
        source.inode,
    )?;
    merge_namespace(&mut counters, visits)?;
    let mut table = InodeTableRoot(namespace.inode_table_root);
    let parent_record = store.put(&encode_inode_record(InodeRecordV1 {
        content_root: directory.0,
        ..parent.record
    })?)?;
    let (next, visits) = inode_table_upsert(store, table, parent.inode, parent_record)?;
    merge_inode(&mut counters, visits)?;
    table = next;
    let source_record = store.put(&encode_inode_record(InodeRecordV1 {
        namespace_ref_count: source
            .record
            .namespace_ref_count
            .checked_add(1)
            .ok_or(CoreError::LengthOverflow)?,
        ..source.record
    })?)?;
    let (table, visits) = inode_table_upsert(store, table, source.inode, source_record)?;
    merge_inode(&mut counters, visits)?;
    finish_namespace_candidate(store, root, namespace, table, counters)
}

pub fn remove_path(
    store: &mut impl ObjectStore,
    root: ObjectId,
    path: &CanonicalPath,
) -> CoreResult<CandidateRoot> {
    let mut counters = LogicalCounters::default();
    let namespace = namespace(store, root)?;
    let (parent, name) = resolve_parent(store, root, path, &mut counters)?;
    let (directory, inode, visits) =
        directory_remove(store, DirectoryStateRoot(parent.record.content_root), &name)?;
    merge_namespace(&mut counters, visits)?;
    let table = InodeTableRoot(namespace.inode_table_root);
    let record_id = inode_table_lookup(store, table, inode, &mut counters.inode_table)?
        .ok_or(CoreError::MissingObject)?;
    let record = store.with_authenticated_canonical(record_id, decode_inode_record)?;
    if record.kind == InodeKind::Directory
        && !directory_page_after(
            store,
            DirectoryStateRoot(record.content_root),
            None,
            1,
            4096,
            &mut counters.namespace,
        )?
        .entries
        .is_empty()
    {
        return Err(CoreError::InvalidRecord("directory not empty"));
    }
    let parent_record = store.put(&encode_inode_record(InodeRecordV1 {
        content_root: directory.0,
        ..parent.record
    })?)?;
    let (mut table, visits) = inode_table_upsert(store, table, parent.inode, parent_record)?;
    merge_inode(&mut counters, visits)?;
    if record.namespace_ref_count == 1 {
        let (next, _, visits) = inode_table_remove(store, table, inode)?;
        merge_inode(&mut counters, visits)?;
        table = next;
    } else {
        let record = store.put(&encode_inode_record(InodeRecordV1 {
            namespace_ref_count: record.namespace_ref_count - 1,
            ..record
        })?)?;
        let (next, visits) = inode_table_upsert(store, table, inode, record)?;
        merge_inode(&mut counters, visits)?;
        table = next;
    }
    finish_namespace_candidate(store, root, namespace, table, counters)
}

fn create_inode(
    store: &mut impl ObjectStore,
    root: ObjectId,
    path: &CanonicalPath,
    inode: InodeId,
    record: InodeRecordV1,
) -> CoreResult<CandidateRoot> {
    let mut counters = LogicalCounters::default();
    let namespace = namespace(store, root)?;
    let (parent, name) = resolve_parent(store, root, path, &mut counters)?;
    if directory_lookup(
        store,
        DirectoryStateRoot(parent.record.content_root),
        &name,
        &mut counters.namespace,
    )?
    .is_some()
    {
        return Err(CoreError::NameCollision);
    }
    let (directory, visits) = directory_insert(
        store,
        DirectoryStateRoot(parent.record.content_root),
        name,
        inode,
    )?;
    merge_namespace(&mut counters, visits)?;
    let table = InodeTableRoot(namespace.inode_table_root);
    let parent_record = store.put(&encode_inode_record(InodeRecordV1 {
        content_root: directory.0,
        ..parent.record
    })?)?;
    let (table, visits) = inode_table_upsert(store, table, parent.inode, parent_record)?;
    merge_inode(&mut counters, visits)?;
    let record = store.put(&encode_inode_record(record)?)?;
    let (table, visits) = inode_table_upsert(store, table, inode, record)?;
    merge_inode(&mut counters, visits)?;
    finish_namespace_candidate(store, root, namespace, table, counters)
}

fn finish_namespace_candidate(
    store: &mut impl ObjectStore,
    parent_root: ObjectId,
    namespace: NamespaceRootV1,
    table: InodeTableRoot,
    counters: LogicalCounters,
) -> CoreResult<CandidateRoot> {
    let root = store.put(&encode_namespace_root(NamespaceRootV1 {
        inode_table_root: table.0,
        ..namespace
    })?)?;
    Ok(CandidateRoot {
        parent_root,
        root,
        counters,
    })
}

pub fn replace_file<S, R, F>(
    store: &mut S,
    root: ObjectId,
    path: &CanonicalPath,
    input: R,
    initialize: F,
) -> CoreResult<CandidateRoot>
where
    S: ObjectStore,
    R: Read,
    F: FnOnce(&mut S) -> CoreResult<(InodeId, ObjectId)>,
{
    let mut counters = LogicalCounters::default();
    let namespace = namespace(store, root)?;
    let (parent, name) = resolve_parent(store, root, path, &mut counters)?;
    let mut directory = directory_lookup(
        store,
        DirectoryStateRoot(parent.record.content_root),
        &name,
        &mut counters.namespace,
    )?;
    let (content, rope) = build(store, input)?;
    counters.rope = rope;
    let table = InodeTableRoot(namespace.inode_table_root);
    let (inode, record) = match directory.take() {
        Some(inode) => {
            let record_id = inode_table_lookup(store, table, inode, &mut counters.inode_table)?
                .ok_or(CoreError::MissingObject)?;
            let record = store.with_authenticated_canonical(record_id, decode_inode_record)?;
            if record.kind != InodeKind::RegularFile {
                return Err(CoreError::WrongLogicalRole);
            }
            (
                inode,
                InodeRecordV1 {
                    content_root: content.0,
                    ..record
                },
            )
        }
        None => {
            let (inode, metadata_root) = initialize(store)?;
            let (next, visits) = directory_insert(
                store,
                DirectoryStateRoot(parent.record.content_root),
                name,
                inode,
            )?;
            merge_namespace(&mut counters, visits)?;
            let parent_record = store.put(&encode_inode_record(InodeRecordV1 {
                content_root: next.0,
                ..parent.record
            })?)?;
            let (next_table, visits) =
                inode_table_upsert(store, table, parent.inode, parent_record)?;
            merge_inode(&mut counters, visits)?;
            return finish_file_candidate(
                store,
                root,
                namespace,
                next_table,
                inode,
                InodeRecordV1 {
                    kind: InodeKind::RegularFile,
                    namespace_ref_count: 1,
                    content_root: content.0,
                    metadata_root,
                },
                counters,
            );
        }
    };
    finish_file_candidate(store, root, namespace, table, inode, record, counters)
}

fn finish_file_candidate<S: ObjectStore>(
    store: &mut S,
    parent_root: ObjectId,
    namespace: NamespaceRootV1,
    table: InodeTableRoot,
    inode: InodeId,
    record: InodeRecordV1,
    mut counters: LogicalCounters,
) -> CoreResult<CandidateRoot> {
    let record = store.put(&encode_inode_record(record)?)?;
    let (table, visits) = inode_table_upsert(store, table, inode, record)?;
    merge_inode(&mut counters, visits)?;
    let root = store.put(&encode_namespace_root(NamespaceRootV1 {
        inode_table_root: table.0,
        ..namespace
    })?)?;
    Ok(CandidateRoot {
        parent_root,
        root,
        counters,
    })
}

fn merge_namespace(
    counters: &mut LogicalCounters,
    source: crate::tree::directory::NamespaceCounters,
) -> CoreResult<()> {
    counters.namespace.nodes_read = counters
        .namespace
        .nodes_read
        .checked_add(source.nodes_read)
        .ok_or(CoreError::LengthOverflow)?;
    counters.namespace.nodes_created = counters
        .namespace
        .nodes_created
        .checked_add(source.nodes_created)
        .ok_or(CoreError::LengthOverflow)?;
    Ok(())
}

fn merge_namespace_counters(
    target: &mut crate::tree::directory::NamespaceCounters,
    source: crate::tree::directory::NamespaceCounters,
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

fn merge_inode(
    counters: &mut LogicalCounters,
    source: crate::tree::inode::InodeTableCounters,
) -> CoreResult<()> {
    counters.inode_table.nodes_read = counters
        .inode_table
        .nodes_read
        .checked_add(source.nodes_read)
        .ok_or(CoreError::LengthOverflow)?;
    counters.inode_table.nodes_created = counters
        .inode_table
        .nodes_created
        .checked_add(source.nodes_created)
        .ok_or(CoreError::LengthOverflow)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::directory::codec::decode_directory_node;
    use crate::tree::directory::{directory_entries, NamespaceCounters};
    use crate::tree::inode::codec::{decode_inode_table_node, encode_inode_record};
    use crate::tree::inode::{inode_table_entries, inode_table_from_root, InodeTableCounters};
    use std::collections::BTreeMap;

    #[derive(Clone, Default)]
    struct CountingStore {
        objects: BTreeMap<ObjectId, Vec<u8>>,
        inode_node_puts: u64,
        directory_node_puts: u64,
    }

    impl CountingStore {
        fn reset_puts(&mut self) {
            self.inode_node_puts = 0;
            self.directory_node_puts = 0;
        }
    }

    impl ObjectStore for CountingStore {
        fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
            self.objects
                .get(&id)
                .cloned()
                .ok_or(CoreError::MissingObject)
        }

        fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
            let id = ObjectId::for_bytes(canonical);
            if decode_inode_table_node(canonical).is_ok() {
                self.inode_node_puts += 1;
            }
            if decode_directory_node(canonical).is_ok() {
                self.directory_node_puts += 1;
            }
            self.objects.insert(id, canonical.to_vec());
            Ok(id)
        }
    }

    fn object(byte: u8) -> ObjectId {
        ObjectId::from_bytes(&[byte; 32]).unwrap()
    }

    fn record(serial: usize) -> InodeRecordV1 {
        InodeRecordV1 {
            kind: InodeKind::RegularFile,
            namespace_ref_count: 1,
            content_root: object((serial % 251 + 1) as u8),
            metadata_root: object((serial.wrapping_mul(17) % 251 + 1) as u8),
        }
    }

    fn inode_fixture() -> (CountingStore, ObjectId, InodeId) {
        let mut store = CountingStore::default();
        let root_inode = InodeId::allocate([0x41; 32], 0);
        let root_record = store
            .put(
                &encode_inode_record(InodeRecordV1 {
                    kind: InodeKind::Directory,
                    namespace_ref_count: 0,
                    content_root: object(0x51),
                    metadata_root: object(0x52),
                })
                .unwrap(),
            )
            .unwrap();
        let table = inode_table_from_root(&mut store, root_inode, root_record).unwrap();
        let root = store
            .put(
                &encode_namespace_root(NamespaceRootV1 {
                    profile_id: crate::tree::directory::codec::profile_id(),
                    root_directory_inode: root_inode,
                    inode_table_root: table.0,
                })
                .unwrap(),
            )
            .unwrap();
        store.reset_puts();
        (store, root, root_inode)
    }

    fn sequential_inode_mutations(
        store: &mut CountingStore,
        root: ObjectId,
        mutations: impl IntoIterator<Item = InodeMutation>,
    ) -> CoreResult<CandidateRoot> {
        let mut counters = LogicalCounters::default();
        let namespace = namespace(store, root)?;
        let mut table = InodeTableRoot(namespace.inode_table_root);
        for mutation in mutations {
            match mutation {
                InodeMutation::Upsert { inode, record } => {
                    let record = store.put(&encode_inode_record(record)?)?;
                    let (next, visits) = inode_table_upsert(store, table, inode, record)?;
                    merge_inode(&mut counters, visits)?;
                    table = next;
                }
                InodeMutation::Remove { inode } => {
                    let (next, _, visits) = inode_table_remove(store, table, inode)?;
                    merge_inode(&mut counters, visits)?;
                    table = next;
                }
            }
        }
        let root = store.put(&encode_namespace_root(NamespaceRootV1 {
            inode_table_root: table.0,
            ..namespace
        })?)?;
        Ok(CandidateRoot::new(root, root, counters))
    }

    fn sequential_directory_changes(
        store: &mut CountingStore,
        mut root: DirectoryStateRoot,
        changes: impl IntoIterator<Item = (crate::CanonicalName, Option<InodeId>)>,
    ) -> CoreResult<(DirectoryStateRoot, NamespaceCounters)> {
        let mut counters = NamespaceCounters::default();
        for (name, desired) in changes {
            if directory_lookup(store, root, &name, &mut counters)?.is_some() {
                let (next, _, visits) = directory_remove(store, root, &name)?;
                merge_namespace_counters(&mut counters, visits)?;
                root = next;
            }
            if let Some(inode) = desired {
                let (next, visits) = directory_insert(store, root, name, inode)?;
                merge_namespace_counters(&mut counters, visits)?;
                root = next;
            }
        }
        Ok((root, counters))
    }

    fn orders() -> [Vec<usize>; 2] {
        [
            (0..512).collect(),
            (0..512).map(|index| index * 73 % 512).collect(),
        ]
    }

    #[test]
    fn deferred_inode_batch_matches_sequential_roots_with_local_final_puts() {
        for order in orders() {
            let (baseline, root, _) = inode_fixture();
            let mut mutations = order
                .iter()
                .copied()
                .map(|serial| InodeMutation::Upsert {
                    inode: InodeId::allocate([0x41; 32], serial as u64 + 1),
                    record: record(serial),
                })
                .collect::<Vec<_>>();
            mutations.extend(order.iter().copied().filter_map(|serial| match serial % 3 {
                0 => Some(InodeMutation::Remove {
                    inode: InodeId::allocate([0x41; 32], serial as u64 + 1),
                }),
                1 => Some(InodeMutation::Upsert {
                    inode: InodeId::allocate([0x41; 32], serial as u64 + 1),
                    record: record(serial + 1024),
                }),
                _ => None,
            }));
            let removed = order.iter().filter(|serial| **serial % 3 == 0).count();
            let mut sequential = baseline.clone();
            let expected =
                sequential_inode_mutations(&mut sequential, root, mutations.clone()).unwrap();
            let mut deferred = baseline.clone();
            let outcome = apply_inode_mutations_deferred(&mut deferred, root, mutations).unwrap();
            let actual = outcome.value;

            assert_eq!(actual.root(), expected.root());
            assert_eq!(
                actual.counters().inode_table.nodes_read,
                expected.counters().inode_table.nodes_read
            );
            let namespace = namespace(&deferred, actual.root()).unwrap();
            let entries = inode_table_entries(
                &deferred,
                InodeTableRoot(namespace.inode_table_root),
                &mut InodeTableCounters::default(),
            )
            .unwrap();
            assert_eq!(entries.len(), 513 - removed);
            assert_eq!(
                entries,
                inode_table_entries(
                    &sequential,
                    InodeTableRoot(namespace.inode_table_root),
                    &mut InodeTableCounters::default(),
                )
                .unwrap()
            );
            assert_eq!(
                actual.counters().inode_table.nodes_created,
                deferred.inode_node_puts
            );
            assert!(
                deferred.inode_node_puts * 8 < sequential.inode_node_puts,
                "deferred={} sequential={}",
                deferred.inode_node_puts,
                sequential.inode_node_puts
            );
            assert!(outcome.peak_bytes <= crate::tree::inode::DEFERRED_INODE_MAX_BYTES);
            assert!(
                outcome.prunes > 0,
                "fixture did not cross inode prune watermark"
            );
        }
    }

    #[test]
    fn deferred_directory_batch_matches_sequential_roots_with_local_final_puts() {
        for order in orders() {
            let mut baseline = CountingStore::default();
            let root = empty_directory(&mut baseline).unwrap();
            baseline.reset_puts();
            let mut changes = order
                .iter()
                .copied()
                .map(|serial| {
                    (
                        crate::CanonicalName::new(&format!("entry-{serial:04}")).unwrap(),
                        Some(InodeId::allocate([0x61; 32], serial as u64)),
                    )
                })
                .collect::<Vec<_>>();
            changes.extend(order.iter().copied().filter_map(|serial| match serial % 3 {
                0 => Some((
                    crate::CanonicalName::new(&format!("entry-{serial:04}")).unwrap(),
                    None,
                )),
                1 => Some((
                    crate::CanonicalName::new(&format!("entry-{serial:04}")).unwrap(),
                    Some(InodeId::allocate([0x62; 32], serial as u64)),
                )),
                _ => None,
            }));
            let mut sequential = baseline.clone();
            let expected =
                sequential_directory_changes(&mut sequential, root, changes.clone()).unwrap();
            let mut deferred = baseline.clone();
            let outcome = apply_directory_changes_deferred(&mut deferred, root, changes).unwrap();
            let actual = outcome.value;

            assert_eq!(actual.0, expected.0);
            assert_eq!(actual.1.nodes_read, expected.1.nodes_read);
            assert_eq!(
                directory_entries(&deferred, actual.0, &mut NamespaceCounters::default()).unwrap(),
                directory_entries(&sequential, expected.0, &mut NamespaceCounters::default())
                    .unwrap()
            );
            assert_eq!(actual.1.nodes_created, deferred.directory_node_puts);
            assert!(
                deferred.directory_node_puts * 8 < sequential.directory_node_puts,
                "deferred={} sequential={}",
                deferred.directory_node_puts,
                sequential.directory_node_puts
            );
            assert!(outcome.peak_bytes <= crate::tree::directory::DEFERRED_DIRECTORY_MAX_BYTES);
            assert!(
                outcome.prunes > 0,
                "fixture did not cross directory prune watermark"
            );
        }
    }
}
