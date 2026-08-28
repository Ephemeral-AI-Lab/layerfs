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
