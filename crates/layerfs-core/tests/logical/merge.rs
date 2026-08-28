#[test]
fn three_root_merge_combines_content_and_metadata_on_one_inode() {
    let (mut store, base) = fixture();
    let path = CanonicalPath::new("file").unwrap();
    let source =
        logical::replace_range(&mut store, base, &path, 0, 5, Cursor::new(b"HELLO")).unwrap();
    let destination_metadata = metadata_at(&mut store, InodeKind::RegularFile, 99);
    let destination = logical::replace_range_with_metadata(
        &mut store,
        base,
        &path,
        0,
        0,
        Cursor::new(Vec::<u8>::new()),
        Some(destination_metadata),
    )
    .unwrap();
    let merged = logical::merge_roots(&mut store, base, source.root(), destination.root())
        .unwrap()
        .unwrap();
    let mut bytes = Vec::new();
    logical::stream(&store, merged.root(), &path, &mut bytes).unwrap();
    assert_eq!(bytes, b"HELLO persistent world");
    assert_eq!(
        logical::resolve(
            &store,
            merged.root(),
            &path,
            &mut logical::LogicalCounters::default(),
        )
        .unwrap()
        .record
        .metadata_root,
        destination_metadata
    );
}

#[test]
fn three_root_merge_combines_disjoint_names_in_one_directory() {
    let (mut store, base) = fixture();
    let source_metadata = metadata(&mut store, InodeKind::RegularFile);
    let source = logical::replace_file(
        &mut store,
        base,
        &CanonicalPath::new("source").unwrap(),
        Cursor::new(b"source"),
        |_| Ok((InodeId::allocate([3; 32], 1), source_metadata)),
    )
    .unwrap();
    let destination_metadata = metadata(&mut store, InodeKind::RegularFile);
    let destination = logical::replace_file(
        &mut store,
        base,
        &CanonicalPath::new("destination").unwrap(),
        Cursor::new(b"destination"),
        |_| Ok((InodeId::allocate([3; 32], 2), destination_metadata)),
    )
    .unwrap();
    let merged = logical::merge_roots(&mut store, base, source.root(), destination.root())
        .unwrap()
        .unwrap();
    let mut source_bytes = Vec::new();
    logical::stream(
        &store,
        merged.root(),
        &CanonicalPath::new("source").unwrap(),
        &mut source_bytes,
    )
    .unwrap();
    let mut destination_bytes = Vec::new();
    logical::stream(
        &store,
        merged.root(),
        &CanonicalPath::new("destination").unwrap(),
        &mut destination_bytes,
    )
    .unwrap();
    assert_eq!(source_bytes, b"source");
    assert_eq!(destination_bytes, b"destination");
    assert!(merged.counters().namespace.nodes_created > 0);
}

#[test]
fn three_root_merge_never_publishes_an_undercounted_parallel_hard_link() {
    let (mut store, base) = fixture();
    let source = logical::hard_link(
        &mut store,
        base,
        &CanonicalPath::new("file").unwrap(),
        &CanonicalPath::new("source-link").unwrap(),
    )
    .unwrap();
    let destination = logical::hard_link(
        &mut store,
        base,
        &CanonicalPath::new("file").unwrap(),
        &CanonicalPath::new("destination-link").unwrap(),
    )
    .unwrap();

    assert!(
        logical::merge_roots(&mut store, base, source.root(), destination.root())
            .unwrap()
            .is_err()
    );
}

#[test]
fn three_root_merge_combines_disjoint_metadata_keys() {
    let (mut store, base) = fixture();
    let path = CanonicalPath::new("file").unwrap();
    let base_metadata = logical::resolve(
        &store,
        base,
        &path,
        &mut logical::LogicalCounters::default(),
    )
    .unwrap()
    .record
    .metadata_root;
    let source_metadata = metadata_at(&mut store, InodeKind::RegularFile, 99);
    let (xattr, _) = build(&mut store, Cursor::new(b"value")).unwrap();
    let mut destination_entries = metadata_tree_entries(&store, base_metadata).unwrap();
    let xattr_key = MetadataKey::new("apple.xattr".to_owned(), b"user.test".to_vec()).unwrap();
    destination_entries.push(MetadataEntryV1 {
        key: xattr_key.clone(),
        value_file_root: xattr.0,
    });
    destination_entries.sort_by(|left, right| left.key.cmp(&right.key));
    let destination_metadata = build_metadata_tree(&mut store, &destination_entries).unwrap();
    let source = logical::replace_range_with_metadata(
        &mut store,
        base,
        &path,
        0,
        0,
        Cursor::new(Vec::<u8>::new()),
        Some(source_metadata),
    )
    .unwrap();
    let destination = logical::replace_range_with_metadata(
        &mut store,
        base,
        &path,
        0,
        0,
        Cursor::new(Vec::<u8>::new()),
        Some(destination_metadata),
    )
    .unwrap();
    let merged = logical::merge_roots(&mut store, base, source.root(), destination.root())
        .unwrap()
        .unwrap();
    let merged_metadata = logical::resolve(
        &store,
        merged.root(),
        &path,
        &mut logical::LogicalCounters::default(),
    )
    .unwrap()
    .record
    .metadata_root;
    assert_eq!(
        metadata_lookup(&store, merged_metadata, &xattr_key)
            .unwrap()
            .unwrap()
            .value_file_root,
        xattr.0
    );
    assert_eq!(
        metadata_lookup(
            &store,
            merged_metadata,
            &MetadataKey::new("portable".to_owned(), b"mtime".to_vec()).unwrap(),
        )
        .unwrap()
        .unwrap()
        .value_file_root,
        metadata_lookup(
            &store,
            source_metadata,
            &MetadataKey::new("portable".to_owned(), b"mtime".to_vec()).unwrap(),
        )
        .unwrap()
        .unwrap()
        .value_file_root
    );
}

#[test]
fn inode_merge_prunes_equal_persistent_subtrees() {
    let mut store = MemoryStore::default();
    let common = store
        .put(
            &encode_inode_record(InodeRecordV1 {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 1,
                content_root: ObjectId::for_bytes(b"common-content"),
                metadata_root: ObjectId::for_bytes(b"common-metadata"),
            })
            .unwrap(),
        )
        .unwrap();
    let first = InodeId::allocate([9; 32], 0);
    let mut base = inode_table_from_root(&mut store, first, common).unwrap();
    let mut keys = vec![first];
    for serial in 1..512 {
        let key = InodeId::allocate([9; 32], serial);
        keys.push(key);
        base = inode_table_upsert(&mut store, base, key, common).unwrap().0;
    }
    keys.sort();
    let source_record = store
        .put(
            &encode_inode_record(InodeRecordV1 {
                content_root: ObjectId::for_bytes(b"source-content"),
                ..store
                    .with_authenticated_canonical(
                        common,
                        layerfs_core::namespace_codec::decode_inode_record,
                    )
                    .unwrap()
            })
            .unwrap(),
        )
        .unwrap();
    let destination_record = store
        .put(
            &encode_inode_record(InodeRecordV1 {
                metadata_root: ObjectId::for_bytes(b"destination-metadata"),
                ..store
                    .with_authenticated_canonical(
                        common,
                        layerfs_core::namespace_codec::decode_inode_record,
                    )
                    .unwrap()
            })
            .unwrap(),
        )
        .unwrap();
    let source = inode_table_upsert(&mut store, base, keys[1], source_record)
        .unwrap()
        .0;
    let destination = inode_table_upsert(&mut store, base, keys[510], destination_record)
        .unwrap()
        .0;
    let (merged, counters, _) = merge_inode_tables(&mut store, base, source, destination)
        .unwrap()
        .unwrap();
    assert!(counters.nodes_read < 12, "{counters:?}");
    assert_eq!(
        inode_table_lookup(&store, merged, keys[1], &mut InodeTableCounters::default(),).unwrap(),
        Some(source_record)
    );
    assert_eq!(
        inode_table_lookup(
            &store,
            merged,
            keys[510],
            &mut InodeTableCounters::default(),
        )
        .unwrap(),
        Some(destination_record)
    );
}
