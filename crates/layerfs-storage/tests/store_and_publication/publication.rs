use super::*;

#[test]
fn canonical_rope_objects_share_the_publication_transaction() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-core-publication-test-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let engine = Engine::open_with_mode(&path, IntegrityMode::TrustedLocalDev).unwrap();
    let bytes = vec![0x5a; 200_000];
    let rolled_back_root = {
        let mut publication = engine.begin_publication(None, "aborted").unwrap();
        let (root, _) = build(&mut publication, bytes.as_slice()).unwrap();
        root
    };
    assert!(matches!(
        engine.load_object(rolled_back_root.0),
        Err(EngineError::MissingObject(_))
    ));

    let (file_root, namespace_ref) = {
        let mut publication = engine.begin_publication(None, "main").unwrap();
        let (file_root, _) = build(&mut publication, bytes.as_slice()).unwrap();
        let namespace = encode_namespace_root(NamespaceRootV1 {
            profile_id: profile_id(),
            root_directory_inode: InodeId::allocate([8; 32], 0),
            inode_table_root: ObjectId::for_bytes(b"table"),
        })
        .unwrap();
        let state = publication.publish_namespace(&namespace).unwrap();
        (file_root, state)
    };
    let mut actual = Vec::new();
    read_range(&engine, file_root, 0..bytes.len() as u64, &mut actual).unwrap();
    assert_eq!(actual, bytes);
    assert_eq!(engine.read_ref("main").unwrap().unwrap(), namespace_ref);
    drop(engine);
    let _ = fs::remove_file(path);
}

#[test]
fn publication_is_one_guarded_commit_and_fork_rollback_copy_no_objects() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-publication-test-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let engine = Engine::open_with_mode(&path, IntegrityMode::TrustedLocalDev).unwrap();
    let inode_table_a = ObjectId::for_bytes(b"table A");
    let inode_table_b = ObjectId::for_bytes(b"table B");
    let root_inode = InodeId::allocate([4; 32], 0);
    let root_a_bytes = encode_namespace_root(NamespaceRootV1 {
        profile_id: profile_id(),
        root_directory_inode: root_inode,
        inode_table_root: inode_table_a,
    })
    .unwrap();
    let root_b_bytes = encode_namespace_root(NamespaceRootV1 {
        profile_id: profile_id(),
        root_directory_inode: root_inode,
        inode_table_root: inode_table_b,
    })
    .unwrap();

    let mut aborted = engine.begin_publication(None, "aborted").unwrap();
    let allocated = aborted.allocate_inode_id().unwrap();
    drop(aborted);
    let mut retried = engine.begin_publication(None, "aborted").unwrap();
    assert_eq!(
        retried.allocate_inode_id().unwrap(),
        allocated,
        "rollback consumed durable inode serial"
    );
    drop(retried);

    engine.reset_counters().unwrap();
    let a = engine
        .begin_publication(None, "main")
        .unwrap()
        .publish_namespace(&root_a_bytes)
        .unwrap();
    let committed = engine.counters().unwrap();
    assert_eq!(committed.transactions_committed, 1);
    assert_eq!(committed.publication_transactions_started, 1);
    assert_eq!(committed.publication_transactions_rolled_back, 0);
    assert_eq!(committed.publication_commits, 1);
    assert_eq!(committed.statements, 8);
    assert_eq!(committed.publication_statements, committed.statements);
    let stale = a.clone();
    let b = engine
        .begin_publication(Some(&a), "main")
        .unwrap()
        .publish_namespace(&root_b_bytes)
        .unwrap();
    assert_eq!(b.generation, 1);
    engine.reset_counters().unwrap();
    let no_op = engine
        .begin_publication(Some(&b), "main")
        .unwrap()
        .publish_namespace(&root_b_bytes)
        .unwrap();
    assert_eq!(no_op, b);
    let no_op_counters = engine.counters().unwrap();
    assert_eq!(no_op_counters.transactions_committed, 0);
    assert_eq!(no_op_counters.publication_transactions_started, 1);
    assert_eq!(no_op_counters.publication_transactions_rolled_back, 1);
    assert_eq!(no_op_counters.publication_commits, 0);
    assert!(matches!(
        engine.begin_publication(Some(&stale), "main"),
        Err(EngineError::PublicationConflict)
    ));

    let fork = engine.fork_ref(&a, "experiment").unwrap();
    assert_eq!(fork.root, a.root);
    let rolled_back = engine.move_ref(&b, a.root).unwrap();
    assert_eq!(rolled_back.root, a.root);
    assert_eq!(engine.read_ref("experiment").unwrap().unwrap(), fork);
    let retained = engine.retained_roots().unwrap();
    assert!(retained.contains(&a.root) && retained.contains(&b.root));
    Connection::open(&path)
        .unwrap()
        .execute(
            "DELETE FROM layerfs_retained_roots WHERE root_id = ?1",
            params![b.root.as_bytes().as_slice()],
        )
        .unwrap();
    assert!(matches!(
        engine.move_ref(&rolled_back, b.root),
        Err(EngineError::MissingRoot(missing)) if missing == b.root
    ));
    assert_eq!(engine.read_ref("main").unwrap(), Some(rolled_back));
    drop(engine);
    let _ = fs::remove_file(path);
}

#[test]
fn one_thousand_tiny_revisions_remain_directly_readable_after_reopen() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-thousand-revisions-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let engine = Engine::open_with_mode(&path, IntegrityMode::TrustedLocalDev).unwrap();
    let root_inode = InodeId::allocate([0x44; 32], 0);
    let file_inode = InodeId::allocate([0x44; 32], 1);
    let mut expected = None;
    let mut roots = Vec::new();
    for serial in 0..1_000_u64 {
        let payload = serial.to_be_bytes();
        let mut publication = engine.begin_publication(expected.as_ref(), "main").unwrap();
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
        let (content, _) = build(&mut publication, payload.as_slice()).unwrap();
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
            CanonicalName::new("entry").unwrap(),
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
        let canonical = encode_namespace_root(NamespaceRootV1 {
            profile_id: profile_id(),
            root_directory_inode: root_inode,
            inode_table_root: table.0,
        })
        .unwrap();
        let next = publication.publish_namespace(&canonical).unwrap();
        roots.push((next.clone(), canonical, content, payload));
        expected = Some(next);
    }
    drop(engine);
    let reopened = Engine::open(&path).unwrap();
    let scrub = reopened.counters().unwrap();
    assert_eq!(scrub.retained_union_scrubs, 1);
    assert!(scrub.fetched_rows > 0);
    assert_eq!(scrub.fetched_rows, scrub.fetched_row_authentication_passes);
    assert_eq!(scrub.fetched_rows, scrub.fetched_row_role_decode_passes);
    assert_eq!(reopened.read_ref("main").unwrap(), expected);
    assert_eq!(reopened.retained_roots().unwrap().len(), 1_000);
    for index in [0, 499, 999] {
        let (state, canonical, content, payload) = &roots[index];
        assert_eq!(
            reopened.load_object(state.root).unwrap().canonical_bytes,
            *canonical
        );
        let mut observed = Vec::new();
        read_range(&reopened, *content, 0..payload.len() as u64, &mut observed).unwrap();
        assert_eq!(observed, *payload);
    }
    drop(reopened);
    fs::remove_file(path).unwrap();
}

#[test]
fn verified_publication_rejects_missing_reachable_inode_table_before_visibility() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-verified-publication-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let engine = Engine::open(&path).unwrap();
    let root = encode_namespace_root(NamespaceRootV1 {
        profile_id: profile_id(),
        root_directory_inode: InodeId::allocate([0x73; 32], 0),
        inode_table_root: ObjectId::for_bytes(b"missing inode table"),
    })
    .unwrap();
    engine.reset_counters().unwrap();
    let publication = engine.begin_publication(None, "main").unwrap();
    let before = engine.counters().unwrap();
    assert!(matches!(
        publication.publish_namespace(&root),
        Err(EngineError::MissingObject(_))
    ));
    let after = engine.counters().unwrap();
    assert_eq!(after.statements - before.statements, 5);
    assert_eq!(
        after.publication_statements - before.publication_statements,
        5
    );
    assert_eq!(after.live_verified_integrity_statements, 0);
    assert_eq!(after.fetched_rows - before.fetched_rows, 1);
    assert_eq!(
        after.fetched_row_authentication_passes - before.fetched_row_authentication_passes,
        1
    );
    assert_eq!(
        after.fetched_row_role_decode_passes - before.fetched_row_role_decode_passes,
        1
    );
    assert_eq!(after.scratch_tables - before.scratch_tables, 2);
    assert_eq!(after.scratch_statements - before.scratch_statements, 44);
    assert_eq!(engine.read_ref("main").unwrap(), None);
    drop(engine);
    let _ = fs::remove_file(path);
}

#[test]
fn verified_publication_counts_every_integrity_fetch_authentication_and_role_decode() {
    let path = std::env::temp_dir().join(format!(
        "layerfs-verified-accounting-{}-{}.sqlite",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let engine = Engine::open(&path).unwrap();
    engine.reset_counters().unwrap();
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
    let root_inode = InodeId::allocate([0x74; 32], 0);
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
    publication.publish_namespace(&namespace).unwrap();

    let counters = engine.counters().unwrap();
    assert_eq!(counters.transactions_started, 1);
    assert_eq!(counters.transactions_committed, 1);
    assert_eq!(counters.publication_transactions_started, 1);
    assert_eq!(counters.publication_transactions_rolled_back, 0);
    assert_eq!(counters.publication_commits, 1);
    assert_eq!(counters.integrity_transactions_started, 0);
    assert_eq!(counters.integrity_transactions_committed, 0);
    assert_eq!(counters.integrity_transactions_rolled_back, 0);
    assert_eq!(counters.integrity_statements, 0);
    assert!(counters.fetched_rows > 0);
    assert_eq!(
        counters.fetched_rows,
        counters.fetched_row_authentication_passes
    );
    assert_eq!(
        counters.fetched_rows,
        counters.fetched_row_role_decode_passes
    );
    assert_eq!(counters.publication_closure_passes, 1);
    assert_eq!(counters.namespace_graph_verification_passes, 1);
    assert_eq!(counters.scratch_tables, 2);
    assert!(counters.scratch_statements > 0);
    assert!(counters.scratch_rows > 0);
    assert!(counters.scratch_high_water_bytes > 0);
    drop(engine);
    fs::remove_file(path).unwrap();
}
