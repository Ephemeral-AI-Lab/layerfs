#[test]
fn three_root_merge_streams_independent_inode_changes_and_reports_overlap() {
    let (mut store, base) = rename_fixture();
    let source = logical::replace_range(
        &mut store,
        base,
        &CanonicalPath::new("left/file").unwrap(),
        0,
        7,
        Cursor::new(b"changed"),
    )
    .unwrap();
    let metadata = logical::resolve(
        &store,
        base,
        &CanonicalPath::new("left").unwrap(),
        &mut logical::LogicalCounters::default(),
    )
    .unwrap()
    .record
    .metadata_root;
    let destination = logical::rename(
        &mut store,
        base,
        &CanonicalPath::new("left/file").unwrap(),
        &CanonicalPath::new("right/moved").unwrap(),
        metadata,
        metadata,
    )
    .unwrap();
    let merged = logical::merge_roots(&mut store, base, source.root(), destination.root())
        .unwrap()
        .unwrap();
    let mut bytes = Vec::new();
    logical::stream(
        &store,
        merged.root(),
        &CanonicalPath::new("right/moved").unwrap(),
        &mut bytes,
    )
    .unwrap();
    assert_eq!(bytes, b"changed");
    assert!(merged.counters().inode_table.nodes_created > 0);

    let conflicting = logical::replace_range(
        &mut store,
        base,
        &CanonicalPath::new("left/file").unwrap(),
        0,
        7,
        Cursor::new(b"other!!"),
    )
    .unwrap();
    assert!(
        logical::merge_roots(&mut store, base, source.root(), conflicting.root())
            .unwrap()
            .is_err()
    );
}
