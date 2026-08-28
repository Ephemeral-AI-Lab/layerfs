#[test]
fn directory_and_inode_child_ranges_must_not_overlap() {
    use layerfs_core::namespace_codec::{encode_inode_table_node, InodeTableNodeV1};

    let mut store = MemoryStore::default();
    let store_id = [0x62; 32];
    let groups = [
        (0..11)
            .map(|index| format!("a{index:02}{}", "x".repeat(252)))
            .chain(std::iter::once(format!("m00{}", "x".repeat(252))))
            .collect::<Vec<_>>(),
        std::iter::once(format!("l00{}", "x".repeat(252)))
            .chain((0..11).map(|index| format!("z{index:02}{}", "x".repeat(252))))
            .collect::<Vec<_>>(),
    ];
    let mut children = Vec::new();
    let mut total_bytes = 0_u64;
    for (group_index, names) in groups.into_iter().enumerate() {
        let entries = names
            .into_iter()
            .enumerate()
            .map(|(index, name)| {
                (
                    CanonicalName::new(&name).unwrap(),
                    InodeId::allocate(store_id, (group_index * 12 + index) as u64),
                )
            })
            .collect::<Vec<_>>();
        let bytes = entries
            .iter()
            .map(|(name, _)| 34 + name.as_bytes().len() as u64)
            .sum::<u64>();
        total_bytes += bytes;
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
        children.push((maximum, id));
    }
    let mapping_root = store
        .put(
            &encode_directory_node(&DirectoryNodeV1::Branch {
                level: 1,
                subtree_entry_count: 24,
                subtree_encoded_bytes: total_bytes,
                children,
            })
            .unwrap(),
        )
        .unwrap();
    let root = DirectoryStateRoot(
        store
            .put(
                &encode_directory_state(DirectoryStateV1 {
                    entry_count: 24,
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
        Err(CoreError::NonCanonicalOrdering)
    );

    let mut ids = (0..127_u64)
        .map(|serial| InodeId::allocate([0x63; 32], serial))
        .collect::<Vec<_>>();
    ids.sort();
    let left = ids[..64]
        .iter()
        .map(|id| (*id, ObjectId::for_bytes(id.as_bytes())))
        .collect::<Vec<_>>();
    let right = ids[63..127]
        .iter()
        .map(|id| (*id, ObjectId::for_bytes(id.as_bytes())))
        .collect::<Vec<_>>();
    let left_id = store
        .put(&encode_inode_table_node(&InodeTableNodeV1::Leaf(left)).unwrap())
        .unwrap();
    let right_id = store
        .put(&encode_inode_table_node(&InodeTableNodeV1::Leaf(right)).unwrap())
        .unwrap();
    let root = layerfs_core::inode::InodeTableRoot(
        store
            .put(
                &encode_inode_table_node(&InodeTableNodeV1::Branch {
                    level: 1,
                    subtree_entry_count: 128,
                    children: vec![(ids[63], left_id), (ids[126], right_id)],
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
