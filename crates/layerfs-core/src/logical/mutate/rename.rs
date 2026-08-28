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
