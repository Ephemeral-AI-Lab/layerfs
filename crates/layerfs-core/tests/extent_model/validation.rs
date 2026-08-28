#[test]
fn authenticated_child_summary_mismatch_and_underfull_nonroot_are_rejected() {
    let mut store = MemoryStore::default();
    let mut extents = Vec::new();
    for index in 0..128_u16 {
        let payload = encode_bytes_object(&index.to_be_bytes()).unwrap();
        let payload_id = store.put(&payload).unwrap();
        extents.push(ExtentSliceV3::new(payload_id, 0, 1).unwrap());
    }
    let left = ExtentNodeV3::Leaf {
        subtree_logical_bytes: 64,
        extents: extents[..64].to_vec(),
    };
    let right = ExtentNodeV3::Leaf {
        subtree_logical_bytes: 64,
        extents: extents[64..].to_vec(),
    };
    let left_id = store.put(&encode_node(&left).unwrap()).unwrap();
    let right_id = store.put(&encode_node(&right).unwrap()).unwrap();
    let branch = ExtentNodeV3::Branch {
        level: 1,
        subtree_logical_bytes: 129,
        subtree_extent_count: 128,
        children: vec![
            ChildDescriptorV3 {
                cumulative_logical_end: 65,
                cumulative_extent_end: 64,
                child_object_id: left_id,
            },
            ChildDescriptorV3 {
                cumulative_logical_end: 129,
                cumulative_extent_end: 128,
                child_object_id: right_id,
            },
        ],
    };
    let branch_id = store.put(&encode_node(&branch).unwrap()).unwrap();
    let state = FileStateV3 {
        logical_len: 129,
        extent_count: 128,
        tree_level: 1,
        profile_id: profile_id(),
        mapping_root: branch_id,
    };
    let root = FileStateRoot(store.put(&encode_file_state(state).unwrap()).unwrap());
    assert_eq!(
        read_range(&store, root, 0..1, Vec::new()),
        Err(CoreError::InvalidRecord("extent summary"))
    );

    let underfull = ExtentNodeV3::Leaf {
        subtree_logical_bytes: 1,
        extents: vec![extents[0]],
    };
    let underfull_id = store.put(&encode_node(&underfull).unwrap()).unwrap();
    let valid_id = store.put(&encode_node(&left).unwrap()).unwrap();
    let branch = ExtentNodeV3::Branch {
        level: 1,
        subtree_logical_bytes: 65,
        subtree_extent_count: 65,
        children: vec![
            ChildDescriptorV3 {
                cumulative_logical_end: 1,
                cumulative_extent_end: 1,
                child_object_id: underfull_id,
            },
            ChildDescriptorV3 {
                cumulative_logical_end: 65,
                cumulative_extent_end: 65,
                child_object_id: valid_id,
            },
        ],
    };
    let branch_id = store.put(&encode_node(&branch).unwrap()).unwrap();
    let state = FileStateV3 {
        logical_len: 65,
        extent_count: 65,
        tree_level: 1,
        profile_id: profile_id(),
        mapping_root: branch_id,
    };
    let root = FileStateRoot(store.put(&encode_file_state(state).unwrap()).unwrap());
    assert_eq!(
        read_range(&store, root, 0..1, Vec::new()),
        Err(CoreError::NonCanonicalPagePartition)
    );
}

#[test]
fn fetched_payload_above_frozen_ceiling_is_rejected() {
    let mut store = MemoryStore::default();
    let payload = encode_bytes_object(&vec![7; 32_769]).unwrap();
    let payload_id = store.put(&payload).unwrap();
    let leaf = ExtentNodeV3::Leaf {
        subtree_logical_bytes: 1,
        extents: vec![ExtentSliceV3::new(payload_id, 0, 1).unwrap()],
    };
    let leaf_id = store.put(&encode_node(&leaf).unwrap()).unwrap();
    let state = FileStateV3 {
        logical_len: 1,
        extent_count: 1,
        tree_level: 0,
        profile_id: profile_id(),
        mapping_root: leaf_id,
    };
    let root = FileStateRoot(store.put(&encode_file_state(state).unwrap()).unwrap());
    assert_eq!(
        read_range(&store, root, 0..1, Vec::new()),
        Err(CoreError::ChunkLengthMismatch)
    );
}

#[test]
fn deep_splice_rejoins_boundary_fragments_before_they_become_nonroots() {
    let mut store = MemoryStore::default();
    let payload_a = store.put(&encode_bytes_object(&[0xaa]).unwrap()).unwrap();
    let payload_b = store.put(&encode_bytes_object(&[0xbb]).unwrap()).unwrap();
    let mut leaves = Vec::new();
    for leaf_index in 0..128 {
        let extents = (0..64)
            .map(|index| {
                ExtentSliceV3::new(
                    if (leaf_index + index) % 2 == 0 {
                        payload_a
                    } else {
                        payload_b
                    },
                    0,
                    1,
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        let node = ExtentNodeV3::Leaf {
            subtree_logical_bytes: 64,
            extents,
        };
        leaves.push(store.put(&encode_node(&node).unwrap()).unwrap());
    }
    let mut branches = Vec::new();
    for group in leaves.chunks_exact(64) {
        let children = group
            .iter()
            .enumerate()
            .map(|(index, id)| ChildDescriptorV3 {
                cumulative_logical_end: (index as u64 + 1) * 64,
                cumulative_extent_end: (index as u64 + 1) * 64,
                child_object_id: *id,
            })
            .collect();
        let branch = ExtentNodeV3::Branch {
            level: 1,
            subtree_logical_bytes: 4096,
            subtree_extent_count: 4096,
            children,
        };
        branches.push(store.put(&encode_node(&branch).unwrap()).unwrap());
    }
    let root_node = ExtentNodeV3::Branch {
        level: 2,
        subtree_logical_bytes: 8192,
        subtree_extent_count: 8192,
        children: vec![
            ChildDescriptorV3 {
                cumulative_logical_end: 4096,
                cumulative_extent_end: 4096,
                child_object_id: branches[0],
            },
            ChildDescriptorV3 {
                cumulative_logical_end: 8192,
                cumulative_extent_end: 8192,
                child_object_id: branches[1],
            },
        ],
    };
    let mapping_root = store.put(&encode_node(&root_node).unwrap()).unwrap();
    let state = FileStateV3 {
        logical_len: 8192,
        extent_count: 8192,
        tree_level: 2,
        profile_id: profile_id(),
        mapping_root,
    };
    let root = FileStateRoot(store.put(&encode_file_state(state).unwrap()).unwrap());
    let (next, counters) = replace(&mut store, root, 4095, 2, [0xcc].as_slice()).unwrap();
    assert_eq!(counters.payload_bytes_read, 0);
    let mut bytes = Vec::new();
    read_range(&store, next, 0..8191, &mut bytes).unwrap();
    assert_eq!(bytes.len(), 8191);
    assert_eq!(bytes[4095], 0xcc);
}

#[test]
fn validation_binds_empty_state_and_visits_unread_children() {
    let mut store = MemoryStore::default();
    let payload = store.put(&encode_bytes_object(&[7]).unwrap()).unwrap();
    let extent = ExtentSliceV3::new(payload, 0, 1).unwrap();
    let nonempty = ExtentNodeV3::Leaf {
        subtree_logical_bytes: 1,
        extents: vec![extent],
    };
    let mapping = store.put(&encode_node(&nonempty).unwrap()).unwrap();
    let empty_state = FileStateV3 {
        logical_len: 0,
        extent_count: 0,
        tree_level: 0,
        profile_id: profile_id(),
        mapping_root: mapping,
    };
    let root = FileStateRoot(store.put(&encode_file_state(empty_state).unwrap()).unwrap());
    assert_eq!(
        validate_file(&store, root),
        Err(CoreError::InvalidRecord("extent summary"))
    );

    let leaf = ExtentNodeV3::Leaf {
        subtree_logical_bytes: 64,
        extents: vec![extent; 64],
    };
    let payload_right = store.put(&encode_bytes_object(&[8]).unwrap()).unwrap();
    let right_leaf = ExtentNodeV3::Leaf {
        subtree_logical_bytes: 64,
        extents: vec![ExtentSliceV3::new(payload_right, 0, 1).unwrap(); 64],
    };
    let left = store.put(&encode_node(&leaf).unwrap()).unwrap();
    let right = store.put(&encode_node(&right_leaf).unwrap()).unwrap();
    let branch = ExtentNodeV3::Branch {
        level: 1,
        subtree_logical_bytes: 128,
        subtree_extent_count: 128,
        children: vec![
            ChildDescriptorV3 {
                cumulative_logical_end: 64,
                cumulative_extent_end: 64,
                child_object_id: left,
            },
            ChildDescriptorV3 {
                cumulative_logical_end: 128,
                cumulative_extent_end: 128,
                child_object_id: right,
            },
        ],
    };
    let mapping = store.put(&encode_node(&branch).unwrap()).unwrap();
    let state = FileStateV3 {
        logical_len: 128,
        extent_count: 128,
        tree_level: 1,
        profile_id: profile_id(),
        mapping_root: mapping,
    };
    let root = FileStateRoot(store.put(&encode_file_state(state).unwrap()).unwrap());
    store.0.get_mut(&right).unwrap()[13] ^= 1;
    assert_eq!(
        validate_file(&store, root),
        Err(CoreError::IdentityMismatch)
    );
}

#[test]
fn full_empty_read_authenticates_and_binds_its_mapping_root() {
    let mut store = MemoryStore::default();
    let (valid, _) = build(&mut store, [].as_slice()).unwrap();
    let valid_state = decode_file_state(store.0.get(&valid.0).unwrap()).unwrap();
    assert_eq!(read_all(&store, valid, Vec::new()).unwrap().nodes_read, 2);

    let missing_mapping = valid_state.mapping_root;
    let missing_bytes = store.0.remove(&missing_mapping).unwrap();
    assert_eq!(
        read_all(&store, valid, Vec::new()),
        Err(CoreError::MissingObject)
    );
    store.0.insert(missing_mapping, missing_bytes);

    let wrong_state = FileStateV3 {
        mapping_root: valid.0,
        ..valid_state
    };
    let wrong = FileStateRoot(store.put(&encode_file_state(wrong_state).unwrap()).unwrap());
    assert_eq!(
        read_all(&store, wrong, Vec::new()),
        Err(CoreError::InvalidMappingTag { tag: 0x0a })
    );

    let payload = store.put(&encode_bytes_object(&[7]).unwrap()).unwrap();
    let nonempty_mapping = store
        .put(
            &encode_node(&ExtentNodeV3::Leaf {
                subtree_logical_bytes: 1,
                extents: vec![ExtentSliceV3::new(payload, 0, 1).unwrap()],
            })
            .unwrap(),
        )
        .unwrap();
    let nonempty_state = FileStateV3 {
        mapping_root: nonempty_mapping,
        ..valid_state
    };
    let nonempty = FileStateRoot(
        store
            .put(&encode_file_state(nonempty_state).unwrap())
            .unwrap(),
    );
    assert_eq!(
        read_all(&store, nonempty, Vec::new()),
        Err(CoreError::InvalidRecord("extent summary"))
    );
}
