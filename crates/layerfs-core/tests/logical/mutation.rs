#[test]
fn exact_reads_and_local_mutation_share_one_logical_owner() {
    let (mut store, root) = fixture();
    let path = CanonicalPath::new("file").unwrap();
    let (stat, _) = logical::stat(&store, root, &path).unwrap();
    assert_eq!(stat.kind, InodeKind::RegularFile);
    let (page, _) =
        logical::list(&store, root, &CanonicalPath::new("").unwrap(), None, 1, 128).unwrap();
    assert_eq!(page.entries[0].0.as_bytes(), b"file");
    let mut before = Vec::new();
    logical::read_range(&store, root, &path, 6..16, &mut before).unwrap();
    assert_eq!(before, b"persistent");

    let candidate =
        logical::replace_range(&mut store, root, &path, 6, 10, Cursor::new(b"logical")).unwrap();
    assert_eq!(candidate.parent_root(), root);
    assert_eq!(candidate.counters().rope.cdc_bytes_scanned, 7);
    let mut old = Vec::new();
    logical::stream(&store, root, &path, &mut old).unwrap();
    assert_eq!(old, b"hello persistent world");
    let mut new = Vec::new();
    logical::stream(&store, candidate.root(), &path, &mut new).unwrap();
    assert_eq!(new, b"hello logical world");
}

#[test]
fn rename_keeps_old_roots_readable_and_handles_same_and_cross_directory_moves() {
    let (mut store, root) = rename_fixture();
    let metadata = logical::resolve(
        &store,
        root,
        &CanonicalPath::new("left").unwrap(),
        &mut logical::LogicalCounters::default(),
    )
    .unwrap()
    .record
    .metadata_root;
    let moved = logical::rename(
        &mut store,
        root,
        &CanonicalPath::new("left/file").unwrap(),
        &CanonicalPath::new("right/moved").unwrap(),
        metadata,
        metadata,
    )
    .unwrap();
    let mut old = Vec::new();
    logical::stream(
        &store,
        root,
        &CanonicalPath::new("left/file").unwrap(),
        &mut old,
    )
    .unwrap();
    assert_eq!(old, b"move me");
    let mut current = Vec::new();
    logical::stream(
        &store,
        moved.root(),
        &CanonicalPath::new("right/moved").unwrap(),
        &mut current,
    )
    .unwrap();
    assert_eq!(current, old);
    assert!(logical::stat(
        &store,
        moved.root(),
        &CanonicalPath::new("left/file").unwrap()
    )
    .is_err());

    let renamed = logical::rename(
        &mut store,
        moved.root(),
        &CanonicalPath::new("right/moved").unwrap(),
        &CanonicalPath::new("right/final").unwrap(),
        metadata,
        metadata,
    )
    .unwrap();
    let mut final_bytes = Vec::new();
    logical::stream(
        &store,
        renamed.root(),
        &CanonicalPath::new("right/final").unwrap(),
        &mut final_bytes,
    )
    .unwrap();
    assert_eq!(final_bytes, b"move me");
}
