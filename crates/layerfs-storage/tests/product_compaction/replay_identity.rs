use super::*;

fn test_store(label: &str) -> (std::path::PathBuf, Engine) {
    let base = std::env::temp_dir().join(format!(
        "layerfs-replay-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    let engine =
        Engine::open_with_mode(base.join("store.sqlite"), IntegrityMode::TrustedLocalDev).unwrap();
    (base, engine)
}

fn commit_operation(
    engine: &Engine,
    expected: BranchHead,
    root: layerfs_core::ObjectId,
    seed: u8,
) -> (BranchHead, OperationRecordRef) {
    let operation = OperationId::from_bytes([seed; 32]);
    engine
        .product_begin_operation(operation, expected, LeaseId::from_bytes([seed + 1; 32]))
        .unwrap();
    match engine
        .product_operation_commit(OperationCandidate {
            operation_id: operation,
            expected,
            candidate_root: root,
            normalized_transition: Vec::new(),
            request_id: RequestId::from_bytes([seed + 2; 32]),
        })
        .unwrap()
    {
        OperationCommitOutcome::WorkingRecorded { head, record, .. } => (head, record),
        OperationCommitOutcome::Conflict { .. } => panic!("unexpected operation conflict"),
    }
}

fn assert_identity_conflict<T: std::fmt::Debug>(result: Result<T, EngineError>) {
    assert!(
        matches!(result, Err(EngineError::InvalidRecord(_))),
        "expected identity rejection, got {result:?}"
    );
}

fn history_count(engine: &Engine, owner: &str) -> i64 {
    let sql = match owner {
        "branch" => "SELECT COUNT(*) FROM layerfs_branch_transitions",
        "layer_stack" => "SELECT COUNT(*) FROM layerfs_layer_stack_transitions",
        _ => unreachable!(),
    };
    Connection::open(engine.path())
        .unwrap()
        .query_row(sql, [], |row| row.get(0))
        .unwrap()
}

#[test]
fn layer_stack_replay_rejects_changed_expected_and_candidate_identity() {
    let (base, engine) = test_store("layer-stack");
    let root0 = valid_empty_root_with_seed(&engine, [0x10; 32]);
    let root1 = valid_empty_root_with_seed(&engine, [0x11; 32]);
    let root2 = valid_empty_root_with_seed(&engine, [0x12; 32]);
    let other = valid_empty_root_with_seed(&engine, [0x13; 32]);
    let stack = engine
        .product_create_layer_stack(
            LayerStackId::from_bytes([0x20; 32]),
            LayerId::from_bytes([0x21; 32]),
            "stack",
            root0,
        )
        .unwrap();
    let branch = engine
        .product_create_top_level_branch(BranchId::from_bytes([0x22; 32]), None, stack)
        .unwrap();
    let (source, _) = commit_operation(&engine, branch, root1, 0x30);
    let candidate = engine
        .product_prepare_layer_candidate(LayerCandidateRequest {
            source,
            expected_stack: stack,
            result_root: root2,
            source_transition: Vec::new(),
            applied_transition: Vec::new(),
            request_id: RequestId::from_bytes([0x34; 32]),
        })
        .unwrap();
    let request = RequestId::from_bytes([0x35; 32]);
    let accepted = match engine
        .product_accept_layer_stack_merge(candidate, stack, request)
        .unwrap()
    {
        LayerStackMergeOutcome::DurablyAccepted { head, .. } => head,
        other => panic!("unexpected merge result: {other:?}"),
    };
    assert_eq!(
        engine.product_accept_layer_stack_merge(candidate, stack, request),
        Ok(LayerStackMergeOutcome::DurablyAccepted {
            head: accepted,
            reconciled: true,
        })
    );
    let merge_history = history_count(&engine, "layer_stack");

    let mut wrong_expected = stack;
    wrong_expected.root = other;
    assert_identity_conflict(engine.product_accept_layer_stack_merge(
        candidate,
        wrong_expected,
        request,
    ));
    wrong_expected = stack;
    wrong_expected.generation += 1;
    assert_identity_conflict(engine.product_accept_layer_stack_merge(
        candidate,
        wrong_expected,
        request,
    ));
    let mut wrong = candidate;
    wrong.root = other;
    assert_identity_conflict(engine.product_accept_layer_stack_merge(wrong, stack, request));
    wrong = candidate;
    wrong.layer_id = LayerId::from_bytes([0x38; 32]);
    assert_identity_conflict(engine.product_accept_layer_stack_merge(wrong, stack, request));
    wrong = candidate;
    wrong.parent_layer_id = accepted.layer_id;
    assert_identity_conflict(engine.product_accept_layer_stack_merge(wrong, stack, request));
    wrong = candidate;
    wrong.source.root = other;
    assert_identity_conflict(engine.product_accept_layer_stack_merge(wrong, stack, request));
    wrong = candidate;
    wrong.source.operation_version_id = None;
    assert_identity_conflict(engine.product_accept_layer_stack_merge(wrong, stack, request));
    wrong = candidate;
    wrong.source_depth += 1;
    assert_identity_conflict(engine.product_accept_layer_stack_merge(wrong, stack, request));
    wrong = candidate;
    wrong.request_id = RequestId::from_bytes([0x36; 32]);
    assert_identity_conflict(engine.product_accept_layer_stack_merge(wrong, stack, request));
    assert_eq!(
        engine.product_layer_stack_head(stack.layer_stack_id),
        Ok(Some(accepted))
    );
    assert_eq!(history_count(&engine, "layer_stack"), merge_history);

    let rollback_request = RequestId::from_bytes([0x37; 32]);
    let rolled = match engine
        .product_layer_stack_rollback(accepted, stack.layer_id, rollback_request)
        .unwrap()
    {
        LayerStackRollbackOutcome::DurablyAccepted { head, .. } => head,
        other => panic!("unexpected rollback result: {other:?}"),
    };
    assert_eq!(
        engine.product_layer_stack_rollback(accepted, stack.layer_id, rollback_request),
        Ok(LayerStackRollbackOutcome::DurablyAccepted {
            head: rolled,
            reconciled: true,
        })
    );
    let rollback_history = history_count(&engine, "layer_stack");
    let mut wrong_before = accepted;
    wrong_before.root = other;
    assert_identity_conflict(engine.product_layer_stack_rollback(
        wrong_before,
        stack.layer_id,
        rollback_request,
    ));
    wrong_before = accepted;
    wrong_before.generation += 1;
    assert_identity_conflict(engine.product_layer_stack_rollback(
        wrong_before,
        stack.layer_id,
        rollback_request,
    ));
    assert_identity_conflict(engine.product_layer_stack_rollback(
        accepted,
        accepted.layer_id,
        rollback_request,
    ));
    assert_eq!(
        engine.product_layer_stack_head(stack.layer_stack_id),
        Ok(Some(rolled))
    );
    assert_eq!(history_count(&engine, "layer_stack"), rollback_history);

    drop(engine);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn branch_rollback_replay_rejects_changed_before_head_and_target() {
    let (base, engine) = test_store("branch-rollback");
    let root0 = valid_empty_root_with_seed(&engine, [0x40; 32]);
    let root1 = valid_empty_root_with_seed(&engine, [0x41; 32]);
    let root2 = valid_empty_root_with_seed(&engine, [0x42; 32]);
    let other = valid_empty_root_with_seed(&engine, [0x43; 32]);
    let stack = engine
        .product_create_layer_stack(
            LayerStackId::from_bytes([0x44; 32]),
            LayerId::from_bytes([0x45; 32]),
            "stack",
            root0,
        )
        .unwrap();
    let branch = engine
        .product_create_top_level_branch(BranchId::from_bytes([0x46; 32]), None, stack)
        .unwrap();
    let (first, _) = commit_operation(&engine, branch, root1, 0x50);
    let (second, _) = commit_operation(&engine, first, root2, 0x54);
    let target = first.operation_version_id.unwrap();
    let request = RequestId::from_bytes([0x58; 32]);
    let rolled = match engine
        .product_branch_rollback(second, target, request)
        .unwrap()
    {
        BranchRollbackOutcome::WorkingRecorded { head, .. } => head,
        other => panic!("unexpected rollback result: {other:?}"),
    };
    assert_eq!(
        engine.product_branch_rollback(second, target, request),
        Ok(BranchRollbackOutcome::WorkingRecorded {
            head: rolled,
            reconciled: true,
        })
    );
    let rollback_history = history_count(&engine, "branch");

    let mut wrong = second;
    wrong.root = other;
    assert_identity_conflict(engine.product_branch_rollback(wrong, target, request));
    wrong = second;
    wrong.generation -= 1;
    assert_identity_conflict(engine.product_branch_rollback(wrong, target, request));
    wrong = second;
    wrong.operation_version_id = first.operation_version_id;
    assert_identity_conflict(engine.product_branch_rollback(wrong, target, request));
    wrong = second;
    wrong.branch_id = BranchId::from_bytes([0x59; 32]);
    assert_identity_conflict(engine.product_branch_rollback(wrong, target, request));
    assert_identity_conflict(engine.product_branch_rollback(
        second,
        second.operation_version_id.unwrap(),
        request,
    ));
    assert_eq!(
        engine.product_branch_head(branch.branch_id),
        Ok(Some(rolled))
    );
    assert_eq!(history_count(&engine, "branch"), rollback_history);

    drop(engine);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn child_merge_replay_rejects_changed_parent_source_and_transition_identity() {
    let (base, engine) = test_store("child-merge");
    let root0 = valid_empty_root_with_seed(&engine, [0x60; 32]);
    let root1 = valid_empty_root_with_seed(&engine, [0x61; 32]);
    let root2 = valid_empty_root_with_seed(&engine, [0x62; 32]);
    let result_root = valid_empty_root_with_seed(&engine, [0x63; 32]);
    let other = valid_empty_root_with_seed(&engine, [0x64; 32]);
    let stack = engine
        .product_create_layer_stack(
            LayerStackId::from_bytes([0x65; 32]),
            LayerId::from_bytes([0x66; 32]),
            "stack",
            root0,
        )
        .unwrap();
    let parent = engine
        .product_create_top_level_branch(BranchId::from_bytes([0x67; 32]), None, stack)
        .unwrap();
    let (parent, fork) = commit_operation(&engine, parent, root1, 0x68);
    let child = engine
        .product_create_child_branch(BranchId::from_bytes([0x6c; 32]), None, fork)
        .unwrap();
    let (source, _) = commit_operation(&engine, child, root2, 0x70);
    let candidate = ChildMergeCandidate {
        source,
        expected_parent: parent,
        result_root,
        source_transition: b"source-transition".to_vec(),
        applied_transition: b"applied-transition".to_vec(),
        request_id: RequestId::from_bytes([0x74; 32]),
    };
    let accepted = match engine
        .product_child_branch_merge(candidate.clone())
        .unwrap()
    {
        ChildMergeOutcome::WorkingRecorded { parent_head, .. } => parent_head,
        other => panic!("unexpected child merge result: {other:?}"),
    };
    assert_eq!(
        engine.product_child_branch_merge(candidate.clone()),
        Ok(ChildMergeOutcome::WorkingRecorded {
            parent_head: accepted,
            reconciled: true,
        })
    );
    let merge_history = history_count(&engine, "branch");

    let mut wrong = candidate.clone();
    wrong.expected_parent.root = other;
    assert_identity_conflict(engine.product_child_branch_merge(wrong));
    wrong = candidate.clone();
    wrong.expected_parent.generation += 1;
    assert_identity_conflict(engine.product_child_branch_merge(wrong));
    wrong = candidate.clone();
    wrong.expected_parent.branch_id = BranchId::from_bytes([0x75; 32]);
    assert_identity_conflict(engine.product_child_branch_merge(wrong));
    wrong = candidate.clone();
    wrong.expected_parent.operation_version_id = None;
    assert_identity_conflict(engine.product_child_branch_merge(wrong));
    wrong = candidate.clone();
    wrong.source.root = other;
    assert_identity_conflict(engine.product_child_branch_merge(wrong));
    wrong = candidate.clone();
    wrong.source.generation -= 1;
    assert_identity_conflict(engine.product_child_branch_merge(wrong));
    wrong = candidate.clone();
    wrong.source.operation_version_id = None;
    assert_identity_conflict(engine.product_child_branch_merge(wrong));
    wrong = candidate.clone();
    wrong.source.branch_id = BranchId::from_bytes([0x76; 32]);
    assert_identity_conflict(engine.product_child_branch_merge(wrong));
    wrong = candidate.clone();
    wrong.result_root = other;
    assert_identity_conflict(engine.product_child_branch_merge(wrong));
    wrong = candidate.clone();
    wrong.source_transition.push(0xff);
    assert_identity_conflict(engine.product_child_branch_merge(wrong));
    wrong = candidate.clone();
    wrong.applied_transition.push(0xff);
    assert_identity_conflict(engine.product_child_branch_merge(wrong));
    assert_eq!(
        engine.product_branch_head(parent.branch_id),
        Ok(Some(accepted))
    );
    assert_eq!(history_count(&engine, "branch"), merge_history);
    assert_eq!(
        engine.product_child_branch_merge(candidate),
        Ok(ChildMergeOutcome::WorkingRecorded {
            parent_head: accepted,
            reconciled: true,
        })
    );

    drop(engine);
    fs::remove_dir_all(base).unwrap();
}
