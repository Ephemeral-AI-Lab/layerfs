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

#[test]
fn metadata_builder_keeps_only_bounded_tail_summaries() {
    let mut store = MemoryStore::default();
    let mut builder = MetadataTreeBuilder::new();
    for index in 0..20_000_u32 {
        builder
            .push(
                &mut store,
                MetadataEntryV1 {
                    key: MetadataKey::new(
                        "apple.xattr".to_owned(),
                        format!("x{index:08}").into_bytes(),
                    )
                    .unwrap(),
                    value_file_root: ObjectId::for_bytes(&index.to_be_bytes()),
                },
            )
            .unwrap();
        assert!(builder.peak_pending_entries() < 512);
        assert!(builder.peak_pending_summaries() < 512);
    }
    let root = builder.finish(&mut store).unwrap();
    let mut observed = 0_usize;
    layerfs_core::metadata::visit_metadata_entries(&store, root, |entries| {
        observed += entries.len();
        Ok(())
    })
    .unwrap();
    assert_eq!(observed, 20_000);
}
