use super::*;

#[cfg(feature = "test-hooks")]
#[test]
fn trusted_local_checkpoint_does_not_rescan_the_complete_candidate() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-local-checkpoint-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    let mut engine =
        Engine::open_with_mode(base.join("store.sqlite"), IntegrityMode::TrustedLocalDev).unwrap();
    let root = valid_empty_root(&engine);
    let stack = engine
        .product_create_layer_stack(
            LayerStackId::from_bytes([0xb1; 32]),
            LayerId::from_bytes([0xb2; 32]),
            "local-checkpoint",
            root,
        )
        .unwrap();
    let branch = engine
        .product_create_top_level_branch(
            BranchId::from_bytes([0xb3; 32]),
            Some("local-checkpoint"),
            stack,
        )
        .unwrap();
    let operation_id = OperationId::from_bytes([0xb4; 32]);
    engine
        .product_begin_operation(operation_id, branch, LeaseId::from_bytes([0xb5; 32]))
        .unwrap();
    engine.reset_counters().unwrap();
    let mut writer = engine.begin_candidate_write().unwrap();
    let metadata = logical::resolve(
        &writer,
        root,
        &CanonicalPath::root(),
        &mut logical::LogicalCounters::default(),
    )
    .unwrap()
    .record
    .metadata_root;
    let inode = writer.allocate_inode_id().unwrap();
    let candidate = writer
        .trusted_create_directory(root, &CanonicalPath::new("local").unwrap(), inode, metadata)
        .unwrap();
    let candidate_root = candidate.root();
    writer
        .commit_trusted_operation_candidate(operation_id, candidate)
        .unwrap();
    let request = OperationCandidate {
        operation_id,
        expected: branch,
        candidate_root,
        normalized_transition: Vec::new(),
        request_id: RequestId::from_bytes([0xb6; 32]),
    };
    engine.inject_lost_commit_acknowledgement();
    assert!(matches!(
        engine.product_operation_commit(request.clone()).unwrap(),
        OperationCommitOutcome::WorkingRecorded {
            reconciled: true,
            ..
        }
    ));
    let counters = engine.counters().unwrap();
    assert_eq!(counters.candidate_full_scans, 0, "{counters:?}");
    assert!(counters.candidate_shallow_bindings >= 2, "{counters:?}");
    engine.reset_counters().unwrap();
    assert!(matches!(
        engine.product_operation_commit(request).unwrap(),
        OperationCommitOutcome::WorkingRecorded {
            reconciled: true,
            ..
        }
    ));
    let replay = engine.counters().unwrap();
    assert_eq!(replay.candidate_full_scans, 0, "{replay:?}");
    assert_eq!(replay.candidate_shallow_bindings, 1, "{replay:?}");
    drop(engine);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn authenticated_fetch_closure_is_built_once_and_paged_from_disk() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-fetch-closure-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    let path = base.join("store.sqlite");
    let engine = Engine::open_with_mode(&path, IntegrityMode::TrustedLocalDev).unwrap();
    let root = valid_empty_root(&engine);
    let stack = engine
        .product_create_layer_stack(
            LayerStackId::from_bytes([0xc1; 32]),
            LayerId::from_bytes([0xc2; 32]),
            "fetch-closure",
            root,
        )
        .unwrap();
    let branch = engine
        .product_create_top_level_branch(
            BranchId::from_bytes([0xc3; 32]),
            Some("fetch-closure"),
            stack,
        )
        .unwrap();
    let operation_id = OperationId::from_bytes([0xc4; 32]);
    engine
        .product_begin_operation(operation_id, branch, LeaseId::from_bytes([0xc5; 32]))
        .unwrap();
    let mut writer = engine.begin_candidate_write().unwrap();
    let inode = writer.allocate_inode_id().unwrap();
    let (mode, _) = build(&mut writer, 0o644_u32.to_be_bytes().as_slice()).unwrap();
    let (mtime, _) = build(&mut writer, [0_u8; 12].as_slice()).unwrap();
    let metadata = build_metadata_tree(
        &mut writer,
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
    let candidate = writer
        .trusted_replace_file(
            root,
            &CanonicalPath::new("large").unwrap(),
            PatternReader {
                remaining: 10 * 1024 * 1024,
                state: 0x9e37_79b9_7f4a_7c15,
            },
            (inode, metadata),
        )
        .unwrap();
    let candidate_root = candidate.root();
    writer
        .commit_trusted_operation_candidate(operation_id, candidate)
        .unwrap();
    let head = match engine
        .product_operation_commit(OperationCandidate {
            operation_id,
            expected: branch,
            candidate_root,
            normalized_transition: Vec::new(),
            request_id: RequestId::from_bytes([0xc6; 32]),
        })
        .unwrap()
    {
        OperationCommitOutcome::WorkingRecorded { head, .. } => head,
        OperationCommitOutcome::Conflict { .. } => panic!("unexpected conflict"),
    };
    assert_eq!(head.root, candidate_root);
    engine.reset_counters().unwrap();
    let bundle = engine
        .product_export_branch_fetch_page(branch.branch_id, None, None)
        .unwrap();
    let mut after = None;
    let mut objects = 0_usize;
    loop {
        let page = engine
            .product_branch_fetch_object_page(
                branch.branch_id,
                None,
                None,
                bundle.head,
                bundle.origin_stack.head,
                after,
                64,
            )
            .unwrap();
        if page.is_empty() {
            break;
        }
        assert!(page.len() <= 64);
        objects += page.len();
        after = page.last().copied();
    }
    assert!(objects > 64);
    let counters = engine.counters().unwrap();
    assert_eq!(counters.fetch_closure_builds, 1, "{counters:?}");
    assert!(counters.fetch_closure_pages > 2, "{counters:?}");
    drop(engine);
    let connection = Connection::open(&path).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM layerfs_fetch_closure_items",
                [],
                |row| { row.get::<_, i64>(0) }
            )
            .unwrap(),
        0
    );
    drop(connection);
    fs::remove_dir_all(base).unwrap();
}
