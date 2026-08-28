use super::*;

#[test]
fn verified_publication_rejects_present_zero_bsd_flags() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-zero-bsd-flags-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let engine = Engine::open(&path).unwrap();
    let mut publication = engine.begin_publication(None, "main").unwrap();
    let (flags, _) = build(&mut publication, 0_u32.to_be_bytes().as_slice()).unwrap();
    let (mode, _) = build(&mut publication, 0o755_u32.to_be_bytes().as_slice()).unwrap();
    let mut mtime = Vec::new();
    mtime.extend_from_slice(&0_i64.to_be_bytes());
    mtime.extend_from_slice(&0_u32.to_be_bytes());
    let (mtime, _) = build(&mut publication, mtime.as_slice()).unwrap();
    let metadata = build_metadata_tree(
        &mut publication,
        &[
            MetadataEntryV1 {
                key: MetadataKey::new("apple.bsd-flags".into(), Vec::new()).unwrap(),
                value_file_root: flags.0,
            },
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
    let root_inode = InodeId::allocate([0x44; 32], 0);
    let record = publication
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
    let table = inode_table_from_root(&mut publication, root_inode, record).unwrap();
    let namespace = encode_namespace_root(NamespaceRootV1 {
        profile_id: profile_id(),
        root_directory_inode: root_inode,
        inode_table_root: table.0,
    })
    .unwrap();
    assert_eq!(
        publication.publish_namespace(&namespace),
        Err(EngineError::Core(layerfs_core::CoreError::InvalidRecord(
            "BSD flags"
        )))
    );
    assert_eq!(engine.read_ref("main").unwrap(), None);
    drop(engine);
    fs::remove_file(path).unwrap();
}

#[test]
fn verified_fork_rejects_unretained_orphan_root() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-invalid-fork-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let engine = Engine::open(&path).unwrap();
    let canonical = encode_namespace_root(NamespaceRootV1 {
        profile_id: profile_id(),
        root_directory_inode: InodeId::allocate([0x92; 32], 0),
        inode_table_root: ObjectId::for_bytes(b"orphan missing table"),
    })
    .unwrap();
    let root = ObjectId::for_bytes(&canonical);
    Connection::open(&path)
        .unwrap()
        .execute(
            "INSERT INTO layerfs_objects (object_id, kind, canonical_length, canonical_bytes) VALUES (?1, 1, ?2, ?3)",
            params![root.as_bytes().as_slice(), canonical.len() as i64, canonical],
        )
        .unwrap();
    let source = RefState {
        name: "orphan".into(),
        generation: 0,
        root,
    };
    assert!(matches!(
        engine.fork_ref(&source, "invalid-fork"),
        Err(EngineError::MissingRoot(missing)) if missing == root
    ));
    assert_eq!(engine.read_ref("invalid-fork").unwrap(), None);
    drop(engine);
    let _ = fs::remove_file(path);
}
