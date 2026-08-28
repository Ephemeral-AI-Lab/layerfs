#[test]
fn paired_diff_skips_equal_roots_and_length_mismatch_mappings() {
    let mut store = MemoryStore::default();
    let (old, _) = build(&mut store, [1_u8; 4096].as_slice()).unwrap();
    let (shorter, _) = build(&mut store, [2_u8; 2048].as_slice()).unwrap();
    let reader = CountRead {
        store: &store,
        reads: Cell::new(0),
    };
    let mut ranges = Vec::new();
    assert_eq!(
        diff_ranges(&reader, old, old, |range| {
            ranges.push(range);
            Ok(())
        })
        .unwrap(),
        (true, Default::default())
    );
    assert_eq!(reader.reads.get(), 0);
    let (same_length, counters) = diff_ranges(&reader, old, shorter, |_| Ok(())).unwrap();
    assert!(!same_length);
    assert_eq!(reader.reads.get(), 2);
    assert_eq!(counters.nodes_read, 2);
    assert_eq!(counters.payload_bytes_read, 0);
    assert!(ranges.is_empty());
}

#[test]
fn paired_diff_handles_unequal_height_and_partition_without_payload_reads() {
    let mut store = MemoryStore::default();
    let payload = store.put(&encode_bytes_object(&[7]).unwrap()).unwrap();
    let extents = (0..128)
        .map(|_| ExtentSliceV3::new(payload, 0, 1).unwrap())
        .collect::<Vec<_>>();
    let leaf_root = store
        .put(
            &encode_node(&ExtentNodeV3::Leaf {
                subtree_logical_bytes: 128,
                extents: extents.clone(),
            })
            .unwrap(),
        )
        .unwrap();
    let left = store
        .put(
            &encode_node(&ExtentNodeV3::Leaf {
                subtree_logical_bytes: 64,
                extents: extents[..64].to_vec(),
            })
            .unwrap(),
        )
        .unwrap();
    let right = store
        .put(
            &encode_node(&ExtentNodeV3::Leaf {
                subtree_logical_bytes: 64,
                extents: extents[64..].to_vec(),
            })
            .unwrap(),
        )
        .unwrap();
    let branch_root = store
        .put(
            &encode_node(&ExtentNodeV3::Branch {
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
            })
            .unwrap(),
        )
        .unwrap();
    let state = |mapping_root, tree_level| FileStateV3 {
        logical_len: 128,
        extent_count: 128,
        tree_level,
        profile_id: profile_id(),
        mapping_root,
    };
    let old = FileStateRoot(
        store
            .put(&encode_file_state(state(leaf_root, 0)).unwrap())
            .unwrap(),
    );
    let new = FileStateRoot(
        store
            .put(&encode_file_state(state(branch_root, 1)).unwrap())
            .unwrap(),
    );
    let mut ranges = Vec::new();
    let (same_length, counters) = diff_ranges(&store, old, new, |range| {
        ranges.push(range);
        Ok(())
    })
    .unwrap();
    assert!(same_length);
    assert!(ranges.is_empty());
    assert_eq!(counters.payload_bytes_read, 0);
    assert!(counters.nodes_read <= 6);
}
