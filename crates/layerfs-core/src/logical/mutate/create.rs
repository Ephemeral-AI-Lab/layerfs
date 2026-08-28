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
