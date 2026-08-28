#[test]
fn level_two_ranges_use_true_leftmost_leaf_minimum() {
    use layerfs_core::namespace_codec::{encode_inode_table_node, InodeTableNodeV1};

    let mut store = MemoryStore::default();
    let directory_leaf = |store: &mut MemoryStore, prefixes: Vec<String>, serial_base: u64| {
        let entries = prefixes
            .into_iter()
            .enumerate()
            .map(|(index, prefix)| {
                (
                    CanonicalName::new(&format!("{prefix}{}", "x".repeat(252))).unwrap(),
                    InodeId::allocate([0x72; 32], serial_base + index as u64),
                )
            })
            .collect::<Vec<_>>();
        let bytes = entries
            .iter()
            .map(|(name, _)| 34 + name.as_bytes().len() as u64)
            .sum::<u64>();
        let maximum = entries.last().unwrap().0.clone();
        let id = store
            .put(
                &encode_directory_node(&DirectoryNodeV1::Leaf {
                    subtree_encoded_bytes: bytes,
                    entries,
                })
                .unwrap(),
            )
            .unwrap();
        (maximum, id, bytes)
    };
    let mut left_leaves = Vec::new();
    for (leaf, prefix) in ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "m"]
        .into_iter()
        .enumerate()
    {
        left_leaves.push(directory_leaf(
            &mut store,
            (0..12).map(|index| format!("{prefix}{index:02}")).collect(),
            (leaf * 12) as u64,
        ));
    }
    let mut right_leaves = vec![directory_leaf(
        &mut store,
        std::iter::once("l00".to_owned())
            .chain((0..11).map(|index| format!("n{index:02}")))
            .collect(),
        144,
    )];
    for (leaf, prefix) in ["o", "p", "q", "r", "s", "t", "u", "v", "w", "x", "z"]
        .into_iter()
        .enumerate()
    {
        right_leaves.push(directory_leaf(
            &mut store,
            (0..12).map(|index| format!("{prefix}{index:02}")).collect(),
            (156 + leaf * 12) as u64,
        ));
    }
    let directory_branch =
        |store: &mut MemoryStore, level: u8, children: &[(CanonicalName, ObjectId, u64)]| {
            let bytes = children.iter().map(|child| child.2).sum::<u64>();
            let id = store
                .put(
                    &encode_directory_node(&DirectoryNodeV1::Branch {
                        level,
                        subtree_entry_count: children.len() as u64 * 12,
                        subtree_encoded_bytes: bytes,
                        children: children
                            .iter()
                            .map(|child| (child.0.clone(), child.1))
                            .collect(),
                    })
                    .unwrap(),
                )
                .unwrap();
            (children.last().unwrap().0.clone(), id, bytes)
        };
    let left = directory_branch(&mut store, 1, &left_leaves);
    let right = directory_branch(&mut store, 1, &right_leaves);
    let root_branch = DirectoryNodeV1::Branch {
        level: 2,
        subtree_entry_count: 288,
        subtree_encoded_bytes: left.2 + right.2,
        children: vec![(left.0, left.1), (right.0, right.1)],
    };
    let mapping_root = store
        .put(&encode_directory_node(&root_branch).unwrap())
        .unwrap();
    let root = DirectoryStateRoot(
        store
            .put(
                &encode_directory_state(DirectoryStateV1 {
                    entry_count: 288,
                    tree_level: 2,
                    profile_id: profile_id(),
                    mapping_root,
                })
                .unwrap(),
            )
            .unwrap(),
    );
    assert_eq!(
        visit_directory_entries(&store, root, &mut NamespaceCounters::default(), |_| Ok(())),
        Err(CoreError::NonCanonicalOrdering)
    );

    let inode = |value: u16| {
        let mut bytes = [0_u8; 32];
        bytes[..2].copy_from_slice(&value.to_be_bytes());
        InodeId(bytes)
    };
    let inode_leaf = |store: &mut MemoryStore, start: u16| {
        let entries = (start..start + 64)
            .map(|value| (inode(value), ObjectId::for_bytes(&value.to_be_bytes())))
            .collect::<Vec<_>>();
        let maximum = entries.last().unwrap().0;
        let id = store
            .put(&encode_inode_table_node(&InodeTableNodeV1::Leaf(entries)).unwrap())
            .unwrap();
        (maximum, id)
    };
    let mut left_leaves = Vec::new();
    for leaf in 0..64_u16 {
        left_leaves.push(inode_leaf(&mut store, leaf * 64));
    }
    let mut right_leaves = vec![inode_leaf(&mut store, 4050)];
    for leaf in 1..64_u16 {
        right_leaves.push(inode_leaf(&mut store, 4050 + leaf * 64));
    }
    let inode_branch = |store: &mut MemoryStore, children: &[(InodeId, ObjectId)]| {
        let maximum = children.last().unwrap().0;
        let id = store
            .put(
                &encode_inode_table_node(&InodeTableNodeV1::Branch {
                    level: 1,
                    subtree_entry_count: children.len() as u64 * 64,
                    children: children.to_vec(),
                })
                .unwrap(),
            )
            .unwrap();
        (maximum, id)
    };
    let left = inode_branch(&mut store, &left_leaves);
    let right = inode_branch(&mut store, &right_leaves);
    let root = layerfs_core::inode::InodeTableRoot(
        store
            .put(
                &encode_inode_table_node(&InodeTableNodeV1::Branch {
                    level: 2,
                    subtree_entry_count: 8192,
                    children: vec![left, right],
                })
                .unwrap(),
            )
            .unwrap(),
    );
    assert_eq!(
        visit_inode_table_entries(&store, root, &mut InodeTableCounters::default(), |_| Ok(())),
        Err(CoreError::NonCanonicalOrdering)
    );
}
