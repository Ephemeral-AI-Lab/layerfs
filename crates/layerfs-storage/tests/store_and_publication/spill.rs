use super::*;

#[test]
fn verified_publication_survives_cache_spill_without_reopening_store() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-verified-spill-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let engine = Engine::open(&path).unwrap();
    assert_eq!(engine.profile().cache_pages, 1280);
    assert_eq!(engine.profile().cache_spill_pages, 1280);
    let mut state = 0x75a1_5eed_cafe_babe_u64;
    let content_bytes = (0..6 * 1024 * 1024)
        .map(|_| {
            state ^= state << 7;
            state ^= state >> 9;
            state ^= state << 8;
            state as u8
        })
        .collect::<Vec<_>>();
    let aborted_content = {
        let mut publication = engine.begin_publication(None, "aborted").unwrap();
        let (content, _) = build(&mut publication, content_bytes.as_slice()).unwrap();
        assert!(fs::metadata(&path).unwrap().len() > 1024 * 1024);
        content
    };
    assert!(matches!(
        engine.load_object(aborted_content.0),
        Err(EngineError::MissingObject(_))
    ));
    assert_eq!(engine.read_ref("aborted").unwrap(), None);

    let mut publication = engine.begin_publication(None, "main").unwrap();
    let (mode, _) = build(&mut publication, 0o755_u32.to_be_bytes().as_slice()).unwrap();
    let mut mtime = Vec::new();
    mtime.extend_from_slice(&0_i64.to_be_bytes());
    mtime.extend_from_slice(&0_u32.to_be_bytes());
    let (mtime, _) = build(&mut publication, mtime.as_slice()).unwrap();
    let metadata = build_metadata_tree(
        &mut publication,
        &[
            MetadataEntryV1 {
                key: MetadataKey::new("portable".into(), b"mode".to_vec()).unwrap(),
                value_file_root: mode.0,
            },
            MetadataEntryV1 {
                key: MetadataKey::new("portable".into(), b"mtime".to_vec()).unwrap(),
                value_file_root: mtime.0,
            },
        ],
    )
    .unwrap();
    let (content, _) = build(&mut publication, content_bytes.as_slice()).unwrap();
    let root_inode = InodeId::allocate([0x75; 32], 0);
    let file_inode = InodeId::allocate([0x75; 32], 1);
    let file_record = publication
        .put_object(
            &encode_inode_record(InodeRecordV1 {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 1,
                content_root: content.0,
                metadata_root: metadata,
            })
            .unwrap(),
        )
        .unwrap();
    let directory = empty_directory(&mut publication).unwrap();
    let directory = directory_insert(
        &mut publication,
        directory,
        CanonicalName::new("large").unwrap(),
        file_inode,
    )
    .unwrap()
    .0;
    let root_record = publication
        .put_object(
            &encode_inode_record(InodeRecordV1 {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: directory.0,
                metadata_root: metadata,
            })
            .unwrap(),
        )
        .unwrap();
    let table = inode_table_from_root(&mut publication, root_inode, root_record).unwrap();
    let table = inode_table_upsert(&mut publication, table, file_inode, file_record)
        .unwrap()
        .0;
    publication
        .publish_namespace(
            &encode_namespace_root(NamespaceRootV1 {
                profile_id: profile_id(),
                root_directory_inode: root_inode,
                inode_table_root: table.0,
            })
            .unwrap(),
        )
        .unwrap();
    assert!(fs::metadata(&path).unwrap().len() > 5 * 1024 * 1024);
    drop(engine);
    let reopened = Engine::open(&path).unwrap();
    let mut observed = Vec::new();
    read_range(
        &reopened,
        content,
        0..content_bytes.len() as u64,
        &mut observed,
    )
    .unwrap();
    assert_eq!(observed, content_bytes);
    drop(reopened);
    fs::remove_file(path).unwrap();
}

#[test]
fn verified_publication_rejects_all_hashes_correct_unreachable_inode() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-semantic-publication-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let engine = Engine::open(&path).unwrap();
    let mut publication = engine.begin_publication(None, "main").unwrap();
    let (mode, _) = build(&mut publication, 0o755_u32.to_be_bytes().as_slice()).unwrap();
    let mut mtime = Vec::new();
    mtime.extend_from_slice(&0_i64.to_be_bytes());
    mtime.extend_from_slice(&0_u32.to_be_bytes());
    let (mtime, _) = build(&mut publication, mtime.as_slice()).unwrap();
    let metadata = build_metadata_tree(
        &mut publication,
        &[
            MetadataEntryV1 {
                key: MetadataKey::new("portable".into(), b"mode".to_vec()).unwrap(),
                value_file_root: mode.0,
            },
            MetadataEntryV1 {
                key: MetadataKey::new("portable".into(), b"mtime".to_vec()).unwrap(),
                value_file_root: mtime.0,
            },
        ],
    )
    .unwrap();
    let directory = empty_directory(&mut publication).unwrap();
    let root_inode = InodeId::allocate([0x33; 32], 0);
    let root_record = publication
        .put_object(
            &encode_inode_record(InodeRecordV1 {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: directory.0,
                metadata_root: metadata,
            })
            .unwrap(),
        )
        .unwrap();
    let mut table = inode_table_from_root(&mut publication, root_inode, root_record).unwrap();
    let extra_inode = InodeId::allocate([0x33; 32], 1);
    let (empty_file, _) = build(&mut publication, b"".as_slice()).unwrap();
    let extra_record = publication
        .put_object(
            &encode_inode_record(InodeRecordV1 {
                kind: InodeKind::RegularFile,
                namespace_ref_count: 1,
                content_root: empty_file.0,
                metadata_root: metadata,
            })
            .unwrap(),
        )
        .unwrap();
    table = inode_table_upsert(&mut publication, table, extra_inode, extra_record)
        .unwrap()
        .0;
    let namespace = encode_namespace_root(NamespaceRootV1 {
        profile_id: profile_id(),
        root_directory_inode: root_inode,
        inode_table_root: table.0,
    })
    .unwrap();
    assert_eq!(
        publication.publish_namespace(&namespace),
        Err(EngineError::Core(layerfs_core::CoreError::InvalidRecord(
            "unreachable inode table entry"
        )))
    );
    assert_eq!(engine.read_ref("main").unwrap(), None);
    drop(engine);
    let _ = fs::remove_file(path);
}
