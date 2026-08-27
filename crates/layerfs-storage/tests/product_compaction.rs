use layerfs_core::content::rope::build;
use layerfs_core::encode_bytes_object;
use layerfs_core::inode::{inode_table_from_root, InodeId, InodeKind, InodeRecordV1};
use layerfs_core::metadata::{build_metadata_tree, MetadataEntryV1, MetadataKey};
use layerfs_core::namespace::{empty_directory, NamespaceRootV1};
use layerfs_core::namespace_codec::{encode_inode_record, encode_namespace_root, profile_id};
use layerfs_core::object::access::ObjectStore;
use layerfs_core::{logical, CanonicalPath};
use layerfs_storage::integrity::IntegrityMode;
use layerfs_storage::product::{
    BranchId, BranchRollbackOutcome, LayerCandidateRequest, LayerId, LayerStackId,
    LayerStackMergeOutcome, LayerStackRollbackOutcome, LeaseId, OperationCandidate,
    OperationCommitOutcome, OperationId, RecoverableOperationState, RequestId, VersionRef,
};
use layerfs_storage::Engine;
use rusqlite::Connection;
use std::fs;
use std::io::Read;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn product_history_and_roots_survive_compaction_and_reopen() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-product-compaction-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    let source = base.join("source.sqlite");
    let compacted = base.join("compacted.sqlite");
    let engine = Engine::open_with_mode(&source, IntegrityMode::TrustedLocalDev).unwrap();
    let root = valid_empty_root(&engine);
    let layer_root = valid_empty_root_with_seed(&engine, [0x1d; 32]);
    let stack_id = LayerStackId::from_bytes([0x11; 32]);
    let layer_id = LayerId::from_bytes([0x12; 32]);
    let branch_id = BranchId::from_bytes([0x13; 32]);
    let operation_id = OperationId::from_bytes([0x14; 32]);
    let lease_id = LeaseId::from_bytes([0x15; 32]);
    let request_id = RequestId::from_bytes([0x16; 32]);

    let stack = engine
        .product_create_layer_stack(stack_id, layer_id, "main", root)
        .unwrap();
    let branch = engine
        .product_create_top_level_branch(branch_id, Some("work"), stack)
        .unwrap();
    engine
        .product_begin_operation(operation_id, branch, lease_id)
        .unwrap();
    let (accepted, record) = match engine
        .product_operation_commit(OperationCandidate {
            operation_id,
            expected: branch,
            candidate_root: root,
            normalized_transition: Vec::new(),
            request_id,
        })
        .unwrap()
    {
        OperationCommitOutcome::WorkingRecorded { head, record, .. } => (head, record),
        OperationCommitOutcome::Conflict { .. } => panic!("unexpected conflict"),
    };
    assert_eq!(
        engine
            .product_operation_commit(OperationCandidate {
                operation_id,
                expected: branch,
                candidate_root: root,
                normalized_transition: Vec::new(),
                request_id,
            })
            .unwrap(),
        OperationCommitOutcome::WorkingRecorded {
            head: accepted,
            record,
            reconciled: true,
        }
    );
    let recovery = engine
        .product_recoverable_operations_after(None, 1)
        .unwrap();
    assert_eq!(recovery.len(), 1);
    assert_eq!(recovery[0].operation_id, operation_id);
    assert_eq!(
        recovery[0].state,
        RecoverableOperationState::WorkingRecorded
    );
    assert_eq!(
        recovery[0].result_operation_version_id,
        Some(record.operation_version_id)
    );
    assert!(engine
        .product_recoverable_operations_after(Some(operation_id), 1)
        .unwrap()
        .is_empty());
    engine
        .product_acknowledge_operation(operation_id, record.operation_version_id)
        .unwrap();
    assert!(engine.product_recoverable_operations(1).unwrap().is_empty());
    let candidate = engine
        .product_prepare_layer_candidate(LayerCandidateRequest {
            source: accepted,
            expected_stack: stack,
            result_root: layer_root,
            source_transition: Vec::new(),
            applied_transition: Vec::new(),
            request_id: RequestId::from_bytes([0x18; 32]),
        })
        .unwrap();
    assert_eq!(engine.product_layer_stack_head(stack_id), Ok(Some(stack)));
    let merge_request = RequestId::from_bytes([0x19; 32]);
    let accepted_stack = match engine
        .product_accept_layer_stack_merge(candidate, stack, merge_request)
        .unwrap()
    {
        LayerStackMergeOutcome::DurablyAccepted { head, .. } => head,
        LayerStackMergeOutcome::Conflict { .. } => panic!("unexpected LayerStack conflict"),
    };
    assert_eq!(
        engine
            .product_accept_layer_stack_merge(candidate, stack, merge_request)
            .unwrap(),
        LayerStackMergeOutcome::DurablyAccepted {
            head: accepted_stack,
            reconciled: true,
        }
    );
    assert_eq!(engine.product_branch_head(branch_id), Ok(Some(accepted)));
    let rollback_blocker = engine
        .product_create_top_level_branch(
            BranchId::from_bytes([0x1a; 32]),
            Some("rollback-blocker"),
            accepted_stack,
        )
        .unwrap();
    assert_eq!(
        engine
            .product_layer_stack_rollback(
                accepted_stack,
                stack.layer_id,
                RequestId::from_bytes([0x1b; 32]),
            )
            .unwrap(),
        LayerStackRollbackOutcome::Blocked
    );
    engine
        .product_drop_branch(rollback_blocker.branch_id)
        .unwrap();
    let rollback_request = RequestId::from_bytes([0x1c; 32]);
    let rolled_stack = match engine
        .product_layer_stack_rollback(accepted_stack, stack.layer_id, rollback_request)
        .unwrap()
    {
        LayerStackRollbackOutcome::DurablyAccepted { head, .. } => head,
        other => panic!("LayerStack rollback failed: {other:?}"),
    };
    assert_eq!(
        engine
            .product_layer_stack_rollback(accepted_stack, stack.layer_id, rollback_request)
            .unwrap(),
        LayerStackRollbackOutcome::DurablyAccepted {
            head: rolled_stack,
            reconciled: true,
        }
    );
    assert_eq!(
        engine.product_layer_root(accepted_stack.layer_stack_id, accepted_stack.layer_id),
        Ok(None)
    );

    engine.compact_to(&compacted).unwrap();
    drop(engine);
    let reopened = Engine::open_with_mode(&compacted, IntegrityMode::TrustedLocalDev).unwrap();
    assert_eq!(
        reopened.product_layer_stack_head(stack_id),
        Ok(Some(rolled_stack))
    );
    assert_eq!(reopened.product_branch_head(branch_id), Ok(Some(accepted)));
    assert!(reopened
        .load_canonical_authenticated_bounded(layer_root, 1024 * 1024)
        .is_err());
    let child = reopened
        .product_create_child_branch(BranchId::from_bytes([0x17; 32]), Some("child"), record)
        .unwrap();
    assert_eq!(child.root, root);
    drop(reopened);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn verified_reopen_rejects_relationally_corrupt_product_history() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-product-integrity-{}-{}",
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
            LayerStackId::from_bytes([0x81; 32]),
            LayerId::from_bytes([0x82; 32]),
            "integrity",
            root,
        )
        .unwrap();
    let branch = engine
        .product_create_top_level_branch(BranchId::from_bytes([0x83; 32]), Some("integrity"), stack)
        .unwrap();
    drop(engine);
    let connection = Connection::open(&path).unwrap();
    connection
        .execute(
            "UPDATE layerfs_branches SET generation = 1 WHERE branch_id = ?1",
            [branch.branch_id.as_bytes().as_slice()],
        )
        .unwrap();
    drop(connection);
    assert!(Engine::open(&path).is_err());
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn compaction_preserves_in_flight_sync_object_pins() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-sync-pin-compaction-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    let source = base.join("source.sqlite");
    let compacted = base.join("compacted.sqlite");
    let engine = Engine::open_with_mode(&source, IntegrityMode::TrustedLocalDev).unwrap();
    let canonical = encode_bytes_object(b"in-flight sync object").unwrap();
    let id = layerfs_core::ObjectId::for_bytes(&canonical);
    engine
        .accept_canonical_batch_pinned(
            RequestId::from_bytes([0x90; 32]),
            RequestId::from_bytes([0x91; 32]),
            "push",
            &[(id, canonical.clone())],
        )
        .unwrap();
    engine.compact_to(&compacted).unwrap();
    drop(engine);
    let compacted = Engine::open(&compacted).unwrap();
    assert_eq!(
        compacted
            .load_canonical_authenticated_bounded(id, 1024 * 1024)
            .unwrap(),
        canonical
    );
    drop(compacted);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn hard_branch_rollback_releases_only_the_abandoned_suffix() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-hard-rollback-compaction-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    let source = base.join("source.sqlite");
    let compacted = base.join("compacted.sqlite");
    let engine = Engine::open_with_mode(&source, IntegrityMode::TrustedLocalDev).unwrap();
    let root0 = valid_empty_root_with_seed(&engine, [0xa0; 32]);
    let root1 = valid_empty_root_with_seed(&engine, [0xa1; 32]);
    let root2 = valid_empty_root_with_seed(&engine, [0xa2; 32]);
    let stack = engine
        .product_create_layer_stack(
            LayerStackId::from_bytes([0xa3; 32]),
            LayerId::from_bytes([0xa4; 32]),
            "hard-rollback",
            root0,
        )
        .unwrap();
    let branch = engine
        .product_create_top_level_branch(
            BranchId::from_bytes([0xa5; 32]),
            Some("hard-rollback"),
            stack,
        )
        .unwrap();
    let commit = |operation: u8, lease: u8, request: u8, expected, root| {
        let operation_id = OperationId::from_bytes([operation; 32]);
        engine
            .product_begin_operation(operation_id, expected, LeaseId::from_bytes([lease; 32]))
            .unwrap();
        match engine
            .product_operation_commit(OperationCandidate {
                operation_id,
                expected,
                candidate_root: root,
                normalized_transition: Vec::new(),
                request_id: RequestId::from_bytes([request; 32]),
            })
            .unwrap()
        {
            OperationCommitOutcome::WorkingRecorded { head, record, .. } => {
                engine
                    .product_acknowledge_operation(operation_id, record.operation_version_id)
                    .unwrap();
                head
            }
            OperationCommitOutcome::Conflict { .. } => panic!("unexpected conflict"),
        }
    };
    let retained = commit(0xa6, 0xa7, 0xa8, branch, root1);
    let abandoned = commit(0xa9, 0xaa, 0xab, retained, root2);
    let target = retained.operation_version_id.unwrap();
    assert!(matches!(
        engine
            .product_branch_rollback(abandoned, target, RequestId::from_bytes([0xac; 32]))
            .unwrap(),
        BranchRollbackOutcome::WorkingRecorded { head, .. } if head.root == root1
    ));
    assert!(engine
        .product_validate_version_ref(VersionRef::OperationVersion {
            branch_id: branch.branch_id,
            operation_version_id: target,
            root: root1,
        })
        .is_ok());
    assert!(engine
        .product_validate_version_ref(VersionRef::OperationVersion {
            branch_id: branch.branch_id,
            operation_version_id: abandoned.operation_version_id.unwrap(),
            root: root2,
        })
        .is_err());
    engine.compact_to(&compacted).unwrap();
    drop(engine);

    let reopened = Engine::open(&compacted).unwrap();
    assert!(reopened
        .load_canonical_authenticated_bounded(root1, 1024 * 1024)
        .is_ok());
    assert!(reopened
        .load_canonical_authenticated_bounded(root2, 1024 * 1024)
        .is_err());
    drop(reopened);
    fs::remove_dir_all(base).unwrap();
}

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

#[test]
fn raw_candidate_with_missing_child_cannot_use_the_trusted_path() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-forged-candidate-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    let engine =
        Engine::open_with_mode(base.join("store.sqlite"), IntegrityMode::TrustedLocalDev).unwrap();
    let root = valid_empty_root(&engine);
    let stack = engine
        .product_create_layer_stack(
            LayerStackId::from_bytes([0xe1; 32]),
            LayerId::from_bytes([0xe2; 32]),
            "forged-candidate",
            root,
        )
        .unwrap();
    let branch = engine
        .product_create_top_level_branch(
            BranchId::from_bytes([0xe3; 32]),
            Some("forged-candidate"),
            stack,
        )
        .unwrap();
    let operation_id = OperationId::from_bytes([0xe4; 32]);
    engine
        .product_begin_operation(operation_id, branch, LeaseId::from_bytes([0xe5; 32]))
        .unwrap();
    let mut writer = engine.begin_candidate_write().unwrap();
    let forged = writer
        .put(
            &encode_namespace_root(NamespaceRootV1 {
                profile_id: profile_id(),
                root_directory_inode: InodeId::allocate([0xe6; 32], 0),
                inode_table_root: layerfs_core::ObjectId::for_bytes(b"missing inode table"),
            })
            .unwrap(),
        )
        .unwrap();
    assert!(writer
        .commit_operation_candidate(operation_id, forged)
        .is_err());
    assert_eq!(
        engine.product_branch_head(branch.branch_id).unwrap(),
        Some(branch)
    );
    assert!(!engine.contains_authenticated_object(forged).unwrap());
    engine.product_discard_operation(operation_id).unwrap();
    drop(engine);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn sealed_candidate_cannot_cross_storage_with_only_its_top_root() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-cross-store-candidate-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    let source =
        Engine::open_with_mode(base.join("source.sqlite"), IntegrityMode::TrustedLocalDev).unwrap();
    let destination = Engine::open_with_mode(
        base.join("destination.sqlite"),
        IntegrityMode::TrustedLocalDev,
    )
    .unwrap();
    let root = valid_empty_root(&source);
    let mut after = None;
    loop {
        let ids = source.object_ids_page(after, 64).unwrap();
        if ids.is_empty() {
            break;
        }
        let objects = ids
            .iter()
            .map(|id| {
                (
                    *id,
                    source
                        .load_canonical_authenticated_bounded(*id, 1024 * 1024)
                        .unwrap(),
                )
            })
            .collect::<Vec<_>>();
        destination.accept_canonical_batch(&objects).unwrap();
        after = ids.last().copied();
    }
    let stack_id = LayerStackId::from_bytes([0xf1; 32]);
    let layer_id = LayerId::from_bytes([0xf2; 32]);
    let branch_id = BranchId::from_bytes([0xf3; 32]);
    for engine in [&source, &destination] {
        let stack = engine
            .product_create_layer_stack(stack_id, layer_id, "cross-store", root)
            .unwrap();
        engine
            .product_create_top_level_branch(branch_id, Some("cross-store"), stack)
            .unwrap();
    }
    let mut source_writer = source.begin_candidate_write().unwrap();
    let resolved = logical::resolve(
        &source_writer,
        root,
        &CanonicalPath::root(),
        &mut logical::LogicalCounters::default(),
    )
    .unwrap();
    let inode = source_writer.allocate_inode_id().unwrap();
    let candidate = source_writer
        .trusted_create_directory(
            root,
            &CanonicalPath::new("foreign").unwrap(),
            inode,
            resolved.record.metadata_root,
        )
        .unwrap();
    let candidate_root = candidate.root();
    let top = source_writer.get(candidate_root).unwrap();
    source_writer.commit_objects().unwrap();

    let branch = destination.product_branch_head(branch_id).unwrap().unwrap();
    let operation = OperationId::from_bytes([0xf4; 32]);
    destination
        .product_begin_operation(operation, branch, LeaseId::from_bytes([0xf5; 32]))
        .unwrap();
    let mut destination_writer = destination.begin_candidate_write().unwrap();
    assert_eq!(destination_writer.put(&top).unwrap(), candidate_root);
    assert!(destination_writer
        .commit_trusted_operation_candidate(operation, candidate)
        .is_err());
    assert_eq!(
        destination.product_branch_head(branch_id).unwrap(),
        Some(branch)
    );
    assert!(!destination
        .contains_authenticated_object(candidate_root)
        .unwrap());
    destination.product_discard_operation(operation).unwrap();
    drop(destination);
    drop(source);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn sealed_candidate_cannot_cross_writers_after_creator_rollback() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-cross-writer-candidate-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    let engine =
        Engine::open_with_mode(base.join("store.sqlite"), IntegrityMode::TrustedLocalDev).unwrap();
    let root = valid_empty_root(&engine);
    let stack = engine
        .product_create_layer_stack(
            LayerStackId::from_bytes([0xf6; 32]),
            LayerId::from_bytes([0xf7; 32]),
            "cross-writer",
            root,
        )
        .unwrap();
    let branch = engine
        .product_create_top_level_branch(
            BranchId::from_bytes([0xf8; 32]),
            Some("cross-writer"),
            stack,
        )
        .unwrap();
    let operation = OperationId::from_bytes([0xf9; 32]);
    engine
        .product_begin_operation(operation, branch, LeaseId::from_bytes([0xfa; 32]))
        .unwrap();
    let mut creator = engine.begin_candidate_write().unwrap();
    let resolved = logical::resolve(
        &creator,
        root,
        &CanonicalPath::root(),
        &mut logical::LogicalCounters::default(),
    )
    .unwrap();
    let inode = creator.allocate_inode_id().unwrap();
    let candidate = creator
        .trusted_create_directory(
            root,
            &CanonicalPath::new("foreign-writer").unwrap(),
            inode,
            resolved.record.metadata_root,
        )
        .unwrap();
    let candidate_root = candidate.root();
    let top = creator.get(candidate_root).unwrap();
    drop(creator);

    let mut second = engine.begin_candidate_write().unwrap();
    assert_eq!(second.put(&top).unwrap(), candidate_root);
    assert!(second
        .commit_trusted_operation_candidate(operation, candidate)
        .is_err());
    assert_eq!(
        engine.product_branch_head(branch.branch_id).unwrap(),
        Some(branch)
    );
    assert!(!engine
        .contains_authenticated_object(candidate_root)
        .unwrap());
    engine.product_discard_operation(operation).unwrap();
    drop(engine);
    fs::remove_dir_all(base).unwrap();
}

#[cfg(feature = "test-hooks")]
#[test]
fn non_request_product_commit_reconciles_a_lost_acknowledgement() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-product-lost-ack-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    let path = base.join("store.sqlite");
    let mut engine = Engine::open_with_mode(&path, IntegrityMode::TrustedLocalDev).unwrap();
    let root = valid_empty_root(&engine);
    engine.inject_lost_commit_acknowledgement();
    let stack_id = LayerStackId::from_bytes([0x51; 32]);
    let layer_id = LayerId::from_bytes([0x52; 32]);
    let head = engine
        .product_create_layer_stack(stack_id, layer_id, "lost-ack", root)
        .unwrap();
    assert_eq!(engine.product_layer_stack_head(stack_id), Ok(Some(head)));
    drop(engine);
    let reopened = Engine::open_with_mode(&path, IntegrityMode::TrustedLocalDev).unwrap();
    assert_eq!(reopened.product_layer_stack_head(stack_id), Ok(Some(head)));
    drop(reopened);
    fs::remove_dir_all(base).unwrap();
}

fn valid_empty_root(engine: &Engine) -> layerfs_core::ObjectId {
    valid_empty_root_with_seed(engine, [0x18; 32])
}

struct PatternReader {
    remaining: u64,
    state: u64,
}

impl Read for PatternReader {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        let count = output.len().min(self.remaining as usize);
        for byte in &mut output[..count] {
            self.state ^= self.state << 13;
            self.state ^= self.state >> 7;
            self.state ^= self.state << 17;
            *byte = self.state as u8;
        }
        self.remaining -= count as u64;
        Ok(count)
    }
}

fn valid_empty_root_with_seed(engine: &Engine, seed: [u8; 32]) -> layerfs_core::ObjectId {
    let mut publication = engine.begin_candidate_write().unwrap();
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
    let inode = InodeId::allocate(seed, 0);
    let record = publication
        .put(
            &encode_inode_record(InodeRecordV1 {
                kind: InodeKind::Directory,
                namespace_ref_count: 0,
                content_root: directory.0,
                metadata_root: metadata,
            })
            .unwrap(),
        )
        .unwrap();
    let table = inode_table_from_root(&mut publication, inode, record).unwrap();
    let root = publication
        .put(
            &encode_namespace_root(NamespaceRootV1 {
                profile_id: profile_id(),
                root_directory_inode: inode,
                inode_table_root: table.0,
            })
            .unwrap(),
        )
        .unwrap();
    publication.commit_candidate(root).unwrap()
}
