use layerfs_content::file::extent::{ChildDescriptorV3, ExtentNodeV3, ExtentSliceV3, FileStateV3};
use layerfs_content::file::extent_codec::{
    decode_file_state, decode_node, encode_file_state, encode_node, profile_id,
};
use layerfs_content::{CoreError, ObjectId};

#[test]
fn literal_empty_leaf_and_file_state_are_exact() {
    let leaf = ExtentNodeV3::Leaf {
        subtree_logical_bytes: 0,
        extents: Vec::new(),
    };
    let encoded_leaf = encode_node(&leaf).unwrap();
    let mut expected_leaf = Vec::from(
        &b"LFSO\x01\x00\x00\x00\x23\x00\x00\x00\x1fLFS4MAP\0\x00\x03\x08\x00\x00\x00\x00"[..],
    );
    expected_leaf.extend_from_slice(&[0; 16]);
    assert_eq!(encoded_leaf, expected_leaf);
    assert_eq!(decode_node(&encoded_leaf).unwrap(), leaf);

    let mapping_root = ObjectId::for_bytes(&encoded_leaf);
    let state = FileStateV3 {
        logical_len: 0,
        extent_count: 0,
        tree_level: 0,
        profile_id: profile_id(),
        mapping_root,
    };
    let encoded_state = encode_file_state(state).unwrap();
    let mut expected_state =
        Vec::from(&b"LFSO\x01\x00\x00\x00\x61\x00\x00\x00\x5dLFS4MAP\0\x00\x03\x0a\x00"[..]);
    expected_state.extend_from_slice(&[0; 16]);
    expected_state.push(0);
    expected_state.extend_from_slice(profile_id().as_bytes());
    expected_state.extend_from_slice(mapping_root.as_bytes());
    assert_eq!(encoded_state, expected_state);
    assert_eq!(decode_file_state(&encoded_state).unwrap(), state);
}

#[test]
fn profile_digest_is_literal_and_malformed_framing_is_rejected() {
    assert_eq!(
        profile_id().to_string(),
        "e99288f3bc4adea6901bcbb2b14c16f5f573c9cb436309a6cc73d51deb335a72"
    );
    let mut leaf = encode_node(&ExtentNodeV3::Leaf {
        subtree_logical_bytes: 0,
        extents: Vec::new(),
    })
    .unwrap();
    leaf.push(0);
    assert_eq!(decode_node(&leaf), Err(CoreError::TrailingBytes));
}

#[test]
fn selected_nonempty_boundary_ids_are_literal() {
    let leaf = |count: usize, salt: u64| ExtentNodeV3::Leaf {
        subtree_logical_bytes: count as u64,
        extents: (0..count)
            .map(|index| {
                ExtentSliceV3::new(
                    ObjectId::for_bytes(&(salt + index as u64).to_be_bytes()),
                    0,
                    1,
                )
                .unwrap()
            })
            .collect(),
    };
    let one = encode_node(&leaf(1, 1)).unwrap();
    let mut expected_one =
        Vec::from(&b"LFSO\x01\x00\x00\x00K\x00\x00\x00GLFS4MAP\0\x00\x03\x08\x00\x00\x00\x01"[..]);
    expected_one.extend_from_slice(&1_u64.to_be_bytes());
    expected_one.extend_from_slice(&1_u64.to_be_bytes());
    expected_one.extend_from_slice(ObjectId::for_bytes(&1_u64.to_be_bytes()).as_bytes());
    expected_one.extend_from_slice(&0_u32.to_be_bytes());
    expected_one.extend_from_slice(&1_u32.to_be_bytes());
    assert_eq!(one, expected_one);
    let minimum = encode_node(&leaf(64, 100)).unwrap();
    let maximum = encode_node(&leaf(128, 1_000)).unwrap();
    let left = encode_node(&leaf(64, 10_000)).unwrap();
    let right = encode_node(&leaf(65, 20_000)).unwrap();
    let split = encode_node(&ExtentNodeV3::Branch {
        level: 1,
        subtree_logical_bytes: 129,
        subtree_extent_count: 129,
        children: vec![
            ChildDescriptorV3 {
                cumulative_logical_end: 64,
                cumulative_extent_end: 64,
                child_object_id: ObjectId::for_bytes(&left),
            },
            ChildDescriptorV3 {
                cumulative_logical_end: 129,
                cumulative_extent_end: 129,
                child_object_id: ObjectId::for_bytes(&right),
            },
        ],
    })
    .unwrap();
    let level_one = |salt: u64| {
        encode_node(&ExtentNodeV3::Branch {
            level: 1,
            subtree_logical_bytes: 4096,
            subtree_extent_count: 4096,
            children: (1..=64)
                .map(|index| ChildDescriptorV3 {
                    cumulative_logical_end: index * 64,
                    cumulative_extent_end: index * 64,
                    child_object_id: ObjectId::for_bytes(&(salt + index).to_be_bytes()),
                })
                .collect(),
        })
        .unwrap()
    };
    let branch_left = level_one(30_000);
    let branch_right = level_one(40_000);
    let multilevel = encode_node(&ExtentNodeV3::Branch {
        level: 2,
        subtree_logical_bytes: 8192,
        subtree_extent_count: 8192,
        children: vec![
            ChildDescriptorV3 {
                cumulative_logical_end: 4096,
                cumulative_extent_end: 4096,
                child_object_id: ObjectId::for_bytes(&branch_left),
            },
            ChildDescriptorV3 {
                cumulative_logical_end: 8192,
                cumulative_extent_end: 8192,
                child_object_id: ObjectId::for_bytes(&branch_right),
            },
        ],
    })
    .unwrap();
    assert_eq!(
        [one, minimum, maximum, split, multilevel]
            .map(|bytes| ObjectId::for_bytes(&bytes).to_string()),
        [
            "665b3253599af6839deee14dd40da917449e74fdfd78b3c984552e65bd36f8f4",
            "15e6946d0d17d79c312abb0cd01e9f4b1434030726399c7c24f1ddff6d5ee1e1",
            "00315c8aa56d3886ee63747c6378e2415ebe046398f534dae736e4f77df954d4",
            "5dbfadd7f88cb557d05127382b0a7d7f70945f933b8d03c29a3bf8020ed8bbb0",
            "83037ae75611e4cfc19720f6796d61576ba25db6d0cba820134948289354a8cf",
        ]
        .map(str::to_owned)
    );
}
