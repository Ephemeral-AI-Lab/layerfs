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
