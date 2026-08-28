#[test]
fn directory_state_and_branch_summaries_are_authoritative() {
    let mut store = MemoryStore::default();
    let empty = empty_directory(&mut store).unwrap();
    let bad_state = DirectoryStateV1 {
        entry_count: 1,
        tree_level: 0,
        profile_id: profile_id(),
        mapping_root: ObjectId::from_bytes(
            &layerfs_core::decode_bytes_object(&store.get(empty.0).unwrap()).unwrap()[53..85],
        )
        .unwrap(),
    };
    let bad_root = DirectoryStateRoot(
        store
            .put(&encode_directory_state(bad_state).unwrap())
            .unwrap(),
    );
    assert_eq!(
        directory_lookup(
            &store,
            bad_root,
            &CanonicalName::new("missing").unwrap(),
            &mut NamespaceCounters::default()
        ),
        Err(CoreError::InvalidRecord("directory state summary"))
    );

    let store_id = [0x44; 32];
    let mut leaves = Vec::new();
    for prefix in ['a', 'b'] {
        let entries = (0..12)
            .map(|index| {
                let name = format!("{prefix}{index:02}{}", "x".repeat(252));
                (
                    CanonicalName::new(&name).unwrap(),
                    InodeId::allocate(store_id, index),
                )
            })
            .collect::<Vec<_>>();
        let bytes = entries
            .iter()
            .map(|(name, _)| 34 + name.as_bytes().len() as u64)
            .sum();
        let max = entries.last().unwrap().0.clone();
        let id = store
            .put(
                &encode_directory_node(&DirectoryNodeV1::Leaf {
                    subtree_encoded_bytes: bytes,
                    entries,
                })
                .unwrap(),
            )
            .unwrap();
        leaves.push((max, id, bytes));
    }
    let branch = DirectoryNodeV1::Branch {
        level: 1,
        subtree_entry_count: 25,
        subtree_encoded_bytes: leaves[0].2 + leaves[1].2,
        children: leaves
            .iter()
            .map(|(max, id, _)| (max.clone(), *id))
            .collect(),
    };
    let mapping_root = store.put(&encode_directory_node(&branch).unwrap()).unwrap();
    let root = DirectoryStateRoot(
        store
            .put(
                &encode_directory_state(DirectoryStateV1 {
                    entry_count: 25,
                    tree_level: 1,
                    profile_id: profile_id(),
                    mapping_root,
                })
                .unwrap(),
            )
            .unwrap(),
    );
    assert_eq!(
        visit_directory_entries(&store, root, &mut NamespaceCounters::default(), |_| Ok(())),
        Err(CoreError::InvalidRecord("directory branch summary"))
    );
}

#[test]
fn inode_branch_summary_and_occupancy_are_authoritative() {
    use layerfs_core::namespace_codec::{encode_inode_table_node, InodeTableNodeV1};

    let mut store = MemoryStore::default();
    let mut entries = (0..128_u64)
        .map(|serial| {
            (
                InodeId::allocate([0x91; 32], serial),
                ObjectId::for_bytes(&serial.to_be_bytes()),
            )
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.0);
    let mut children = Vec::new();
    for group in entries.chunks_exact(64) {
        let id = store
            .put(&encode_inode_table_node(&InodeTableNodeV1::Leaf(group.to_vec())).unwrap())
            .unwrap();
        children.push((group.last().unwrap().0, id));
    }
    let branch = InodeTableNodeV1::Branch {
        level: 1,
        subtree_entry_count: 129,
        children,
    };
    let root = layerfs_core::inode::InodeTableRoot(
        store
            .put(&encode_inode_table_node(&branch).unwrap())
            .unwrap(),
    );
    assert_eq!(
        visit_inode_table_entries(&store, root, &mut InodeTableCounters::default(), |_| Ok(())),
        Err(CoreError::InvalidRecord("inode branch summary"))
    );

    let one_child = InodeTableNodeV1::Branch {
        level: 1,
        subtree_entry_count: 64,
        children: vec![(entries[63].0, root.0)],
    };
    let root = layerfs_core::inode::InodeTableRoot(
        store
            .put(&encode_inode_table_node(&one_child).unwrap())
            .unwrap(),
    );
    assert_eq!(
        inode_table_lookup(
            &store,
            root,
            entries[0].0,
            &mut InodeTableCounters::default()
        ),
        Err(CoreError::NonCanonicalPagePartition)
    );
}
