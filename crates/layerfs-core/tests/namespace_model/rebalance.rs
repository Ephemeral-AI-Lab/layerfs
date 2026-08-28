#[test]
fn underfull_middle_directory_borrows_right_before_merging_left() {
    let mut store = MemoryStore::default();
    let mut entries = Vec::new();
    for serial in 0..193_u64 {
        let text = format!("{serial:017}");
        let name = CanonicalName::new(&text).unwrap();
        entries.push((name, InodeId::allocate([0x31; 32], serial)));
    }
    let mut children = Vec::new();
    for entries in [&entries[..64], &entries[64..128], &entries[128..]] {
        let bytes = entries
            .iter()
            .map(|(name, _)| 34 + name.as_bytes().len() as u64)
            .sum();
        let id = store
            .put(
                &encode_directory_node(&DirectoryNodeV1::Leaf {
                    subtree_encoded_bytes: bytes,
                    entries: entries.to_vec(),
                })
                .unwrap(),
            )
            .unwrap();
        children.push((entries.last().unwrap().0.clone(), id));
    }
    let mapping = store
        .put(
            &encode_directory_node(&DirectoryNodeV1::Branch {
                level: 1,
                subtree_entry_count: 193,
                subtree_encoded_bytes: entries
                    .iter()
                    .map(|(name, _)| 34 + name.as_bytes().len() as u64)
                    .sum(),
                children,
            })
            .unwrap(),
        )
        .unwrap();
    let root = DirectoryStateRoot(
        store
            .put(
                &encode_directory_state(DirectoryStateV1 {
                    entry_count: 193,
                    tree_level: 1,
                    profile_id: profile_id(),
                    mapping_root: mapping,
                })
                .unwrap(),
            )
            .unwrap(),
    );
    assert_eq!(directory_leaf_counts(&store, root), [64, 64, 65]);
    let retained = root;
    let removed_name = entries[64].0.clone();
    let root = directory_remove(&mut store, root, &removed_name).unwrap().0;
    assert_eq!(directory_leaf_counts(&store, root), [64, 64, 64]);
    assert!(directory_lookup(
        &store,
        retained,
        &removed_name,
        &mut NamespaceCounters::default()
    )
    .unwrap()
    .is_some());
    assert_eq!(
        directory_lookup(
            &store,
            root,
            &removed_name,
            &mut NamespaceCounters::default()
        )
        .unwrap(),
        None
    );
}

#[test]
fn variable_width_directory_borrows_until_both_leaves_are_filled() {
    let mut store = MemoryStore::default();
    let mut groups = vec![Vec::new(), Vec::new(), Vec::new()];
    let mut serial = 0_u64;
    for index in 0..131 {
        groups[0].push((
            CanonicalName::new(&format!("a{index:03}")).unwrap(),
            InodeId::allocate([0x41; 32], serial),
        ));
        serial += 1;
    }
    for index in 0..12 {
        groups[1].push((
            CanonicalName::new(&format!("m{}{index:04}", "m".repeat(250))).unwrap(),
            InodeId::allocate([0x41; 32], serial),
        ));
        serial += 1;
    }
    for index in 0..131 {
        groups[2].push((
            CanonicalName::new(&format!("z{index:03}")).unwrap(),
            InodeId::allocate([0x41; 32], serial),
        ));
        serial += 1;
    }
    let mut children = Vec::new();
    for entries in &groups {
        let id = store
            .put(
                &encode_directory_node(&DirectoryNodeV1::Leaf {
                    subtree_encoded_bytes: entries
                        .iter()
                        .map(|(name, _)| 34 + name.as_bytes().len() as u64)
                        .sum(),
                    entries: entries.clone(),
                })
                .unwrap(),
            )
            .unwrap();
        children.push((entries.last().unwrap().0.clone(), id));
    }
    let mapping = store
        .put(
            &encode_directory_node(&DirectoryNodeV1::Branch {
                level: 1,
                subtree_entry_count: serial,
                subtree_encoded_bytes: groups
                    .iter()
                    .flatten()
                    .map(|(name, _)| 34 + name.as_bytes().len() as u64)
                    .sum(),
                children,
            })
            .unwrap(),
        )
        .unwrap();
    let root = DirectoryStateRoot(
        store
            .put(
                &encode_directory_state(DirectoryStateV1 {
                    entry_count: serial,
                    tree_level: 1,
                    profile_id: profile_id(),
                    mapping_root: mapping,
                })
                .unwrap(),
            )
            .unwrap(),
    );
    let removed = groups[1][0].0.clone();
    let next = directory_remove(&mut store, root, &removed).unwrap().0;
    assert_eq!(directory_leaf_counts(&store, next), [129, 13, 131]);
    assert!(
        directory_lookup(&store, root, &removed, &mut NamespaceCounters::default())
            .unwrap()
            .is_some()
    );
}

#[test]
fn underfull_middle_inode_leaf_borrows_right_before_merging_left() {
    let mut store = MemoryStore::default();
    let store_id = [0x41; 32];
    let mut entries = (0..193_u64)
        .map(|serial| {
            (
                InodeId::allocate(store_id, serial),
                ObjectId::for_bytes(&serial.to_be_bytes()),
            )
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.0);
    let mut children = Vec::new();
    for entries in [&entries[..64], &entries[64..128], &entries[128..]] {
        let id = store
            .put(&encode_inode_table_node(&InodeTableNodeV1::Leaf(entries.to_vec())).unwrap())
            .unwrap();
        children.push((entries.last().unwrap().0, id));
    }
    let retained = InodeTableRoot(
        store
            .put(
                &encode_inode_table_node(&InodeTableNodeV1::Branch {
                    level: 1,
                    subtree_entry_count: 193,
                    children,
                })
                .unwrap(),
            )
            .unwrap(),
    );
    assert_eq!(inode_leaf_counts(&store, retained), [64, 64, 65]);
    let InodeTableNodeV1::Branch { children, .. } =
        decode_inode_table_node(store.0.get(&retained.0).unwrap()).unwrap()
    else {
        unreachable!()
    };
    let InodeTableNodeV1::Leaf(middle) =
        decode_inode_table_node(store.0.get(&children[1].1).unwrap()).unwrap()
    else {
        unreachable!()
    };
    let removed = middle[0].0;
    let current = inode_table_remove(&mut store, retained, removed).unwrap().0;
    assert_eq!(inode_leaf_counts(&store, current), [64, 64, 64]);
    assert!(inode_table_lookup(
        &store,
        retained,
        removed,
        &mut InodeTableCounters::default()
    )
    .unwrap()
    .is_some());
    assert_eq!(
        inode_table_lookup(&store, current, removed, &mut InodeTableCounters::default()).unwrap(),
        None
    );
}
