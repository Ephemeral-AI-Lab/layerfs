use layerfs_core::content::rope::ObjectStore;
use layerfs_core::inode::{InodeId, InodeKind, InodeRecordV1};
use layerfs_core::metadata::{
    build_metadata_tree, decode_apple_acl, encode_apple_acl, metadata_tree_entries,
    AppleAclEntryV1, AppleAclTag, MetadataEntryV1, MetadataKey, MetadataTreeBuilder,
    PortableMetadataV1,
};
use layerfs_core::namespace::{DirectoryStateV1, NamespaceRootV1, SymlinkStateV1};
use layerfs_core::namespace_codec::*;
use layerfs_core::{encode_bytes_object, CoreError, ObjectId};
use std::collections::BTreeMap;

#[derive(Default)]
struct MemoryStore(BTreeMap<ObjectId, Vec<u8>>);
impl ObjectStore for MemoryStore {
    fn get(&self, id: ObjectId) -> Result<Vec<u8>, CoreError> {
        self.0.get(&id).cloned().ok_or(CoreError::MissingObject)
    }
    fn put(&mut self, bytes: &[u8]) -> Result<ObjectId, CoreError> {
        let id = ObjectId::for_bytes(bytes);
        self.0.entry(id).or_insert_with(|| bytes.to_vec());
        Ok(id)
    }
}

#[test]
fn exact_namespace_values_round_trip_and_reject_trailing_bytes() {
    assert_eq!(
        profile_id().to_string(),
        "6290e9107018d54c36ef2fc8d79ad11629010344b3d337e100d483a54f6668bd"
    );
    let mapping = ObjectId::for_bytes(b"directory mapping");
    let directory = DirectoryStateV1 {
        entry_count: 7,
        tree_level: 1,
        profile_id: profile_id(),
        mapping_root: mapping,
    };
    assert_eq!(
        decode_directory_state(&encode_directory_state(directory).unwrap()).unwrap(),
        directory
    );
    assert_eq!(
        ObjectId::for_bytes(&encode_directory_state(directory).unwrap()).to_string(),
        "0913ef4a2422a4cbf7548e2470944284909e9b23080ebf2f731901422e026436"
    );

    let record = InodeRecordV1 {
        kind: InodeKind::RegularFile,
        namespace_ref_count: 2,
        content_root: ObjectId::for_bytes(b"content"),
        metadata_root: ObjectId::for_bytes(b"metadata"),
    };
    assert_eq!(
        decode_inode_record(&encode_inode_record(record).unwrap()).unwrap(),
        record
    );
    assert_eq!(
        ObjectId::for_bytes(&encode_inode_record(record).unwrap()).to_string(),
        "b1cb4b912444022dc7ff7510de67c325273562635c8084e809579f2c78c36cb1"
    );

    let link = SymlinkStateV1::new(b"../dangling/target".to_vec()).unwrap();
    let mut encoded = encode_symlink(&link).unwrap();
    assert_eq!(
        ObjectId::for_bytes(&encoded).to_string(),
        "98bb58549889140b6e10625cf7bc3c108137a36af0df6886ccdc90d8b2f092f6"
    );
    assert_eq!(decode_symlink(&encoded).unwrap(), link);
    encoded.push(0);
    assert_eq!(decode_symlink(&encoded), Err(CoreError::TrailingBytes));

    let root = NamespaceRootV1 {
        profile_id: profile_id(),
        root_directory_inode: InodeId::allocate([3; 32], 1),
        inode_table_root: ObjectId::for_bytes(b"inode table"),
    };
    assert_eq!(
        decode_namespace_root(&encode_namespace_root(root).unwrap()).unwrap(),
        root
    );
    assert_eq!(
        ObjectId::for_bytes(&encode_namespace_root(root).unwrap()).to_string(),
        "394996076853720b4967f1399368738d3322ccce0fda2bd47eadc2f9ac6c8b11"
    );
}

#[test]
fn directory_inode_and_metadata_nodes_are_strict_and_ordered() {
    let first = InodeId::allocate([1; 32], 1);
    let second = InodeId::allocate([1; 32], 2);
    let directory = DirectoryNodeV1::Leaf {
        subtree_encoded_bytes: (34 + 1 + 34 + 2) as u64,
        entries: vec![
            ("a".try_into().unwrap(), first),
            ("bb".try_into().unwrap(), second),
        ],
    };
    assert_eq!(
        decode_directory_node(&encode_directory_node(&directory).unwrap()).unwrap(),
        directory
    );
    let reversed = DirectoryNodeV1::Leaf {
        subtree_encoded_bytes: 71,
        entries: vec![
            ("bb".try_into().unwrap(), second),
            ("a".try_into().unwrap(), first),
        ],
    };
    assert_eq!(
        encode_directory_node(&reversed),
        Err(CoreError::NonCanonicalOrdering)
    );

    let mut inode_entries = vec![
        (first, ObjectId::for_bytes(b"first")),
        (second, ObjectId::for_bytes(b"second")),
    ];
    inode_entries.sort_by_key(|entry| entry.0);
    let inode = InodeTableNodeV1::Leaf(inode_entries);
    assert_eq!(
        decode_inode_table_node(&encode_inode_table_node(&inode).unwrap()).unwrap(),
        inode
    );

    let metadata = MetadataNodeV1::Leaf {
        subtree_encoded_bytes: 49,
        entries: vec![layerfs_core::metadata::MetadataEntryV1 {
            key: layerfs_core::metadata::MetadataKey::new("portable".into(), b"mode".to_vec())
                .unwrap(),
            value_file_root: ObjectId::for_bytes(b"mode value"),
        }],
    };
    assert_eq!(
        decode_metadata_node(&encode_metadata_node(&metadata).unwrap()).unwrap(),
        metadata
    );

    let mut malformed_inode = encode_inode_table_node(&inode).unwrap();
    malformed_inode[43] ^= 1;
    assert_eq!(
        decode_inode_table_node(&malformed_inode),
        Err(CoreError::InvalidRecord("inode subtree encoded bytes"))
    );

    let mut malformed_metadata = encode_metadata_node(&metadata).unwrap();
    malformed_metadata[43] ^= 1;
    assert!(matches!(
        decode_metadata_node(&malformed_metadata),
        Err(CoreError::LengthMismatch { .. })
    ));
}

#[test]
fn portable_and_apple_metadata_are_exact_and_order_preserving() {
    let portable = PortableMetadataV1 {
        permission_mode: 0o755,
        mtime_seconds: -7,
        mtime_nanoseconds: 999_999_999,
    };
    assert_eq!(
        portable.mode_bytes(InodeKind::RegularFile).unwrap(),
        0o755_u32.to_be_bytes()
    );
    assert_eq!(
        &portable.mtime_bytes().unwrap()[..8],
        &(-7_i64).to_be_bytes()
    );
    assert_eq!(
        PortableMetadataV1 {
            permission_mode: 0o4755,
            ..portable
        }
        .validate(InodeKind::RegularFile),
        Err(CoreError::InvalidRecord("portable metadata"))
    );

    let entries = [
        AppleAclEntryV1 {
            tag: AppleAclTag::Deny,
            flags: 0x10,
            rights: 0x2,
            qualifier_uuid: [2; 16],
        },
        AppleAclEntryV1 {
            tag: AppleAclTag::Allow,
            flags: 0x20,
            rights: 0x4,
            qualifier_uuid: [1; 16],
        },
    ];
    let bytes = encode_apple_acl(&entries).unwrap();
    assert_eq!(bytes.len(), 84);
    assert_eq!(decode_apple_acl(&bytes).unwrap(), entries);
    let mut malformed = bytes;
    malformed.push(0);
    assert_eq!(decode_apple_acl(&malformed), Err(CoreError::TrailingBytes));
}

#[test]
fn persistent_metadata_tree_round_trips_multiple_levels() {
    let entries = (0..300_u16)
        .map(|index| MetadataEntryV1 {
            key: MetadataKey::new(
                "apple.xattr".into(),
                format!("user.attribute-{index:04}").into_bytes(),
            )
            .unwrap(),
            value_file_root: ObjectId::for_bytes(&index.to_be_bytes()),
        })
        .collect::<Vec<_>>();
    let mut store = MemoryStore::default();
    let root = build_metadata_tree(&mut store, &entries).unwrap();
    let mut streaming = MetadataTreeBuilder::new();
    for entry in entries.iter().cloned() {
        streaming.push(&mut store, entry).unwrap();
    }
    assert_eq!(streaming.finish(&mut store).unwrap(), root);
    assert_eq!(metadata_tree_entries(&store, root).unwrap(), entries);
}

#[test]
fn metadata_empty_root_is_canonical_and_one_child_root_is_not() {
    let mut store = MemoryStore::default();
    let empty = build_metadata_tree(&mut store, &[]).unwrap();
    assert!(metadata_tree_entries(&store, empty).unwrap().is_empty());

    let entry = MetadataEntryV1 {
        key: MetadataKey::new("portable".into(), b"mode".to_vec()).unwrap(),
        value_file_root: ObjectId::for_bytes(b"mode"),
    };
    let leaf = MetadataNodeV1::Leaf {
        subtree_encoded_bytes: 49,
        entries: vec![entry.clone()],
    };
    let child = store.put(&encode_metadata_node(&leaf).unwrap()).unwrap();
    let branch = MetadataNodeV1::Branch {
        level: 1,
        subtree_entry_count: 1,
        subtree_encoded_bytes: 49,
        children: vec![(entry.key, child)],
    };
    let root = store.put(&encode_metadata_node(&branch).unwrap()).unwrap();
    assert_eq!(
        metadata_tree_entries(&store, root),
        Err(CoreError::NonCanonicalPagePartition)
    );
}

#[test]
fn metadata_child_ranges_must_not_overlap() {
    let mut store = MemoryStore::default();
    let entries = (0..127_u16)
        .map(|index| MetadataEntryV1 {
            key: MetadataKey::new(
                "apple.xattr".into(),
                format!("user.attribute-{index:04}").into_bytes(),
            )
            .unwrap(),
            value_file_root: ObjectId::for_bytes(&index.to_be_bytes()),
        })
        .collect::<Vec<_>>();
    let leaf = |entries: &[MetadataEntryV1]| MetadataNodeV1::Leaf {
        subtree_encoded_bytes: entries
            .iter()
            .map(|entry| 37 + entry.key.domain.len() as u64 + entry.key.key.len() as u64)
            .sum(),
        entries: entries.to_vec(),
    };
    let left = leaf(&entries[..64]);
    let right = leaf(&entries[63..127]);
    let left_bytes = match &left {
        MetadataNodeV1::Leaf {
            subtree_encoded_bytes,
            ..
        } => *subtree_encoded_bytes,
        _ => unreachable!(),
    };
    let right_bytes = match &right {
        MetadataNodeV1::Leaf {
            subtree_encoded_bytes,
            ..
        } => *subtree_encoded_bytes,
        _ => unreachable!(),
    };
    let left_id = store.put(&encode_metadata_node(&left).unwrap()).unwrap();
    let right_id = store.put(&encode_metadata_node(&right).unwrap()).unwrap();
    let root = store
        .put(
            &encode_metadata_node(&MetadataNodeV1::Branch {
                level: 1,
                subtree_entry_count: 128,
                subtree_encoded_bytes: left_bytes + right_bytes,
                children: vec![
                    (entries[63].key.clone(), left_id),
                    (entries[126].key.clone(), right_id),
                ],
            })
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        metadata_tree_entries(&store, root),
        Err(CoreError::NonCanonicalOrdering)
    );
}

#[test]
fn inode_decoder_rejects_size_count_and_level_before_entry_allocation() {
    let header = |magic: &[u8; 8], role: u8, level: u8, count: u16| {
        let mut value = Vec::from(magic.as_slice());
        value.extend_from_slice(&1_u16.to_be_bytes());
        value.extend_from_slice(&[role, level, 0]);
        value.extend_from_slice(&count.to_be_bytes());
        value.extend_from_slice(&0_u64.to_be_bytes());
        value.extend_from_slice(&0_u64.to_be_bytes());
        encode_bytes_object(&value).unwrap()
    };
    assert_eq!(
        decode_inode_table_node(&header(b"LFS4INT\0", 8, 1, 128)),
        Err(CoreError::NonCanonicalPagePartition)
    );
    assert_eq!(
        decode_inode_table_node(&header(b"LFS4INT\0", 8, 32, 0)),
        Err(CoreError::MappingDepthExceeded)
    );
    assert_eq!(
        decode_inode_table_node(&vec![0; 8193]),
        Err(CoreError::ObjectLimitExceeded)
    );
    assert_eq!(
        decode_directory_node(&header(b"LFS4NSP\0", 1, 0, 1000)),
        Err(CoreError::UnexpectedEof)
    );
    assert_eq!(
        decode_metadata_node(&header(b"LFS4MET\0", 9, 0, 1000)),
        Err(CoreError::UnexpectedEof)
    );
}
