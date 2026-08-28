#[test]
fn inode_remove_merges_collapses_and_preserves_retained_root() {
    let mut store = MemoryStore::default();
    let store_id = [0xa5; 32];
    let root_inode = InodeId::allocate(store_id, 0);
    let root_record = ObjectId::for_bytes(b"root record");
    let mut generated =
        generated_inode_table_from_root(&mut store, root_inode, root_record).unwrap();
    for serial in 1..=1_000_u64 {
        generated = generated_inode_table_upsert(
            &mut store,
            generated,
            InodeId::allocate(store_id, serial),
            ObjectId::for_bytes(&serial.to_be_bytes()),
        )
        .unwrap()
        .0;
    }
    let retained = generated.into_root();
    let mut current = retained;
    for serial in 1..=1_000_u64 {
        let (next, removed, counters) =
            inode_table_remove(&mut store, current, InodeId::allocate(store_id, serial)).unwrap();
        assert_eq!(removed, ObjectId::for_bytes(&serial.to_be_bytes()));
        assert!(
            counters.nodes_created <= 8,
            "delete copied more than a spine: {counters:?}"
        );
        current = next;
    }
    assert_eq!(
        inode_table_lookup(
            &store,
            current,
            root_inode,
            &mut InodeTableCounters::default()
        )
        .unwrap(),
        Some(root_record)
    );
    assert_eq!(
        inode_table_lookup(
            &store,
            retained,
            InodeId::allocate(store_id, 500),
            &mut InodeTableCounters::default()
        )
        .unwrap(),
        Some(ObjectId::for_bytes(&500_u64.to_be_bytes()))
    );
}
