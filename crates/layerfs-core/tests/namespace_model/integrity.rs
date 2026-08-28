#[test]
fn repeated_child_levels_are_rejected_before_loading_grandchildren() {
    use layerfs_core::namespace_codec::{encode_inode_table_node, InodeTableNodeV1};

    let mut store = MemoryStore::default();
    let inode = |value: u16| {
        let mut bytes = [0_u8; 32];
        bytes[..2].copy_from_slice(&value.to_be_bytes());
        InodeId(bytes)
    };
    let child_entries = (0..64_u16)
        .map(|value| (inode(value), ObjectId::for_bytes(&value.to_be_bytes())))
        .collect::<Vec<_>>();
    let child_max = child_entries.last().unwrap().0;
    let child = store
        .put(
            &encode_inode_table_node(&InodeTableNodeV1::Branch {
                level: 2,
                subtree_entry_count: 64,
                children: child_entries,
            })
            .unwrap(),
        )
        .unwrap();
    let root = layerfs_core::inode::InodeTableRoot(
        store
            .put(
                &encode_inode_table_node(&InodeTableNodeV1::Branch {
                    level: 2,
                    subtree_entry_count: 128,
                    children: vec![
                        (child_max, child),
                        (inode(u16::MAX), ObjectId::for_bytes(b"unused")),
                    ],
                })
                .unwrap(),
            )
            .unwrap(),
    );
    let mut counters = InodeTableCounters::default();
    assert_eq!(
        visit_inode_table_entries(&store, root, &mut counters, |_| Ok(())),
        Err(CoreError::InvalidRecord("inode child summary"))
    );
    assert_eq!(counters.nodes_read, 2);

    let names = (0..12)
        .map(|index| CanonicalName::new(&format!("a{index:02}{}", "x".repeat(252))).unwrap())
        .collect::<Vec<_>>();
    let child_max = names.last().unwrap().clone();
    let child = store
        .put(
            &encode_directory_node(&DirectoryNodeV1::Branch {
                level: 2,
                subtree_entry_count: 12,
                subtree_encoded_bytes: 12,
                children: names
                    .iter()
                    .map(|name| (name.clone(), ObjectId::for_bytes(name.as_bytes())))
                    .collect(),
            })
            .unwrap(),
        )
        .unwrap();
    let mapping = store
        .put(
            &encode_directory_node(&DirectoryNodeV1::Branch {
                level: 2,
                subtree_entry_count: 24,
                subtree_encoded_bytes: 24,
                children: vec![
                    (child_max, child),
                    (
                        CanonicalName::new(&format!("z99{}", "x".repeat(252))).unwrap(),
                        ObjectId::for_bytes(b"unused directory child"),
                    ),
                ],
            })
            .unwrap(),
        )
        .unwrap();
    let root = DirectoryStateRoot(
        store
            .put(
                &encode_directory_state(DirectoryStateV1 {
                    entry_count: 24,
                    tree_level: 2,
                    profile_id: profile_id(),
                    mapping_root: mapping,
                })
                .unwrap(),
            )
            .unwrap(),
    );
    let mut counters = NamespaceCounters::default();
    assert_eq!(
        visit_directory_entries(&store, root, &mut counters, |_| Ok(())),
        Err(CoreError::InvalidRecord("directory child summary"))
    );
    assert_eq!(counters.nodes_read, 3);
}
