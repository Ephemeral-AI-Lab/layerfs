use super::*;

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
