use layerfs_core::content::rope::build;
use layerfs_core::inode::{inode_table_from_root, InodeKind, InodeRecordV1};
use layerfs_core::metadata::{build_metadata_tree, MetadataEntryV1, MetadataKey};
use layerfs_core::namespace::{empty_directory, NamespaceRootV1};
use layerfs_core::namespace_codec::{encode_inode_record, encode_namespace_root, profile_id};
use layerfs_core::object::access::ObjectStore;
use layerfs_durable_store::DurableStore;
use layerfs_storage::integrity::IntegrityMode;
use layerfs_storage::product::{
    BranchId, BranchPushOutcome, BranchRollbackOutcome, ChildMergeOutcome, LayerId, LayerStackId,
    LayerStackMergeOutcome, LayerStackRollbackOutcome, RequestId,
};
use layerfs_sync::client::{
    abort_push_transfer, fetch_branch, push_branch, push_branch_rollback, push_child_branch_merge,
    push_layer_stack_genesis, push_objects,
};
use layerfs_sync::server::LocalDurable;
use layerfs_sync::ResumeToken;
use layerfs_sync::{DurableControlEndpoint, DurableEndpoint, SyncError};
use layerfs_working_store::{
    BeginOperation, BranchRollbackResult, ChildMergeResult, CommitResult, LayerPreparationResult,
    WorkingCandidate, WorkingStore,
};
use std::cell::Cell;
use std::fs;
use std::io::Cursor;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn object_transfer_has_no_visibility_then_new_branch_push_is_one_exact_action() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-sync-branch-push-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    let mut working =
        WorkingStore::open(&base.join("working"), IntegrityMode::TrustedLocalDev).unwrap();
    let root = valid_empty_root(&mut working);
    let stack_id = LayerStackId::from_bytes([0x41; 32]);
    let layer_id = LayerId::from_bytes([0x42; 32]);
    let branch_id = BranchId::from_bytes([0x43; 32]);
    let stack = working
        .create_layer_stack(stack_id, layer_id, "main", root)
        .unwrap();
    let branch = working
        .create_top_level_branch(branch_id, Some("work"), stack)
        .unwrap();
    let begin = working.begin_operation(branch).unwrap();
    let candidate = no_change(&begin, root);
    let accepted = match working.operation_commit(begin, candidate).unwrap() {
        CommitResult::WorkingRecorded { head, .. } => head,
        CommitResult::Conflict { .. } => panic!("unexpected Working conflict"),
    };

    let durable = DurableStore::open(&base.join("durable")).unwrap();
    let endpoint = LocalDurable::new(&durable);
    let mut object_ids = Vec::new();
    let mut after = None;
    loop {
        let page = working.object_ids_page(after, 16).unwrap();
        if page.is_empty() {
            break;
        }
        after = page.last().copied();
        object_ids.extend(page);
    }
    let first_push = [0x44; 32];
    push_objects(
        &working,
        &endpoint,
        first_push,
        object_ids.iter().copied(),
        ResumeToken::default(),
    )
    .unwrap();
    assert_eq!(durable.branch_head(branch_id).unwrap(), None);
    durable
        .bootstrap_layer_stack(stack_id, layer_id, "main", root)
        .unwrap();
    let abandoned = [0x46; 32];
    endpoint
        .stage_branch_push_page(
            RequestId::from_bytes(abandoned),
            0,
            RequestId::from_bytes([0x60; 32]),
            &working.export_branch_push(branch_id, None).unwrap(),
            layerfs_storage::product::SyncTransferCounters::default(),
        )
        .unwrap();
    assert_eq!(durable.branch_head(branch_id).unwrap(), None);
    assert_eq!(
        durable
            .sync_custody_rows(RequestId::from_bytes(abandoned), "push")
            .unwrap(),
        1
    );
    assert_eq!(abort_push_transfer(&endpoint, abandoned).unwrap(), 1);
    assert_eq!(
        durable
            .sync_custody_rows(RequestId::from_bytes(abandoned), "push")
            .unwrap(),
        0
    );
    let misreported = RequestId::from_bytes([0x63; 32]);
    let misreported_data = RequestId::from_bytes([0x64; 32]);
    let transferred_id = object_ids[0];
    durable
        .sync_accept_objects(
            misreported,
            misreported_data,
            "push",
            &[(
                transferred_id,
                working
                    .sync_read_object(transferred_id, 1024 * 1024)
                    .unwrap(),
            )],
        )
        .unwrap();
    assert!(endpoint
        .stage_branch_push_page(
            misreported,
            0,
            misreported_data,
            &working.export_branch_push(branch_id, None).unwrap(),
            layerfs_storage::product::SyncTransferCounters::default(),
        )
        .is_err());
    assert_eq!(durable.abort_sync_transfer(misreported, "push").unwrap(), 2);
    let fabricated = RequestId::from_bytes([0x61; 32]);
    endpoint
        .stage_branch_push_page(
            fabricated,
            0,
            RequestId::from_bytes([0x62; 32]),
            &working.export_branch_push(branch_id, None).unwrap(),
            layerfs_storage::product::SyncTransferCounters::default(),
        )
        .unwrap();
    assert!(durable
        .commit_staged_branch_push(
            layerfs_storage::product::BranchPushRequest {
                request_id: fabricated,
                expected: None,
                counters: layerfs_storage::product::SyncTransferCounters {
                    unique_bytes: 1,
                    ..layerfs_storage::product::SyncTransferCounters::default()
                },
            },
            branch_id,
        )
        .is_err());
    assert_eq!(durable.branch_head(branch_id).unwrap(), None);
    assert_eq!(durable.abort_sync_transfer(fabricated, "push").unwrap(), 1);
    let lossy = LoseFirstPushAcknowledgement {
        durable: &durable,
        lose: Cell::new(true),
        unique_bytes: Cell::new(0),
    };
    assert!(matches!(
        push_branch(
            &working,
            &lossy,
            first_push,
            branch_id,
            None,
            ResumeToken::default(),
        ),
        Err(SyncError::Destination(_))
    ));
    assert_eq!(
        working
            .push_outbox_state(RequestId::from_bytes(first_push))
            .unwrap()
            .as_deref(),
        Some("indeterminate")
    );
    assert_eq!(durable.branch_head(branch_id).unwrap(), Some(accepted));
    assert_eq!(
        push_branch(
            &working,
            &endpoint,
            first_push,
            branch_id,
            None,
            ResumeToken::default(),
        )
        .unwrap()
        .outcome,
        BranchPushOutcome::DurablyAccepted {
            head: accepted,
            reconciled: true,
        }
    );
    assert_eq!(
        working
            .push_outbox_state(RequestId::from_bytes(first_push))
            .unwrap()
            .as_deref(),
        Some("accepted")
    );

    let next_begin = working.begin_operation(accepted).unwrap();
    let next_candidate = no_change(&next_begin, root);
    let next = match working
        .operation_commit(next_begin, next_candidate)
        .unwrap()
    {
        CommitResult::WorkingRecorded { head, .. } => head,
        CommitResult::Conflict { .. } => panic!("unexpected Working conflict"),
    };
    let second_push = [0x47; 32];
    push_objects(
        &working,
        &endpoint,
        second_push,
        object_ids.iter().copied(),
        ResumeToken::default(),
    )
    .unwrap();
    match push_branch(
        &working,
        &endpoint,
        second_push,
        branch_id,
        Some(accepted),
        ResumeToken::default(),
    )
    .unwrap()
    .outcome
    {
        BranchPushOutcome::DurablyAccepted { head, .. } => assert_eq!(head, next),
        BranchPushOutcome::Conflict { .. } => panic!("Branch advance conflicted"),
    }
    assert_eq!(durable.branch_head(branch_id).unwrap(), Some(next));
    let stale_push = [0x48; 32];
    push_objects(
        &working,
        &endpoint,
        stale_push,
        object_ids.iter().copied(),
        ResumeToken::default(),
    )
    .unwrap();
    assert_eq!(
        push_branch(
            &working,
            &endpoint,
            stale_push,
            branch_id,
            Some(accepted),
            ResumeToken::default(),
        )
        .unwrap()
        .outcome,
        BranchPushOutcome::Conflict { actual: Some(next) }
    );
    assert_eq!(
        working
            .push_outbox_state(RequestId::from_bytes(stale_push))
            .unwrap()
            .as_deref(),
        Some("conflict")
    );

    let working_b_path = base.join("working-b");
    let mut working_b = WorkingStore::open(&working_b_path, IntegrityMode::Verified).unwrap();
    working_b.inject_fetch_boundary_failure_for_test();
    match fetch_branch(
        &endpoint,
        &working_b,
        [0x45; 32],
        branch_id,
        ResumeToken::default(),
    ) {
        Err(SyncError::Destination(_)) => {}
        other => panic!("expected injected Fetch publication failure, got {other:?}"),
    }
    assert_eq!(working_b.branch_head(branch_id).unwrap(), None);
    assert!(!working_b
        .has_verified_branch_tracking(durable.storage_id(), next)
        .unwrap());
    drop(working_b);
    let mut working_b = WorkingStore::open(&working_b_path, IntegrityMode::Verified).unwrap();
    let fetched = fetch_branch(
        &endpoint,
        &working_b,
        [0x45; 32],
        branch_id,
        ResumeToken::default(),
    )
    .unwrap();
    assert_eq!(fetched.head, next);
    assert_eq!(fetched.terminal_object_page_entries, 0);
    assert_eq!(fetched.transfer.terminal_buffer_bytes, 0);
    assert_eq!(fetched.transfer.terminal_queued_batches, 0);
    assert!(fetched.complete_wall_ns >= fetched.head_transaction_ns);
    assert!(working_b
        .has_verified_branch_tracking(durable.storage_id(), next)
        .unwrap());
    assert!(!working_b
        .has_verified_branch_tracking(
            durable.storage_id(),
            layerfs_storage::product::BranchHead {
                operation_version_id: accepted.operation_version_id,
                ..next
            },
        )
        .unwrap());
    let continued_begin = working_b.begin_operation(next).unwrap();
    let continued_candidate = no_change(&continued_begin, root);
    let (continued, continued_record) = match working_b
        .operation_commit(continued_begin, continued_candidate)
        .unwrap()
    {
        CommitResult::WorkingRecorded { head, record, .. } => (head, record),
        CommitResult::Conflict { .. } => panic!("unexpected fetched Working conflict"),
    };
    let continued_push = [0x49; 32];
    push_objects(
        &working_b,
        &endpoint,
        continued_push,
        object_ids.iter().copied(),
        ResumeToken::default(),
    )
    .unwrap();
    match push_branch(
        &working_b,
        &endpoint,
        continued_push,
        branch_id,
        Some(next),
        ResumeToken::default(),
    )
    .unwrap()
    .outcome
    {
        BranchPushOutcome::DurablyAccepted { head, .. } => assert_eq!(head, continued),
        BranchPushOutcome::Conflict { .. } => panic!("continued Branch Push conflicted"),
    }
    assert_eq!(durable.branch_head(branch_id).unwrap(), Some(continued));
    let candidate = match working_b
        .prepare_layer_stack_merge(continued, stack)
        .unwrap()
    {
        LayerPreparationResult::Prepared(candidate) => candidate,
        other => panic!("Layer candidate failed: {other:?}"),
    };
    let alternate_root = valid_empty_root(&mut working_b);
    let mut working_b_ids = Vec::new();
    let mut working_b_after = None;
    loop {
        let page = working_b.object_ids_page(working_b_after, 16).unwrap();
        if page.is_empty() {
            break;
        }
        working_b_after = page.last().copied();
        working_b_ids.extend(page);
    }
    push_objects(
        &working_b,
        &endpoint,
        [0x4a; 32],
        working_b_ids.iter().copied(),
        ResumeToken::default(),
    )
    .unwrap();
    let mut malicious_layer = candidate;
    malicious_layer.root = alternate_root;
    malicious_layer.layer_id = LayerId::from_bytes(layerfs_storage::product::derive_id(
        b"candidate-layer",
        &[
            stack.layer_stack_id.as_bytes(),
            malicious_layer.request_id.as_bytes(),
            alternate_root.as_bytes(),
        ],
    ));
    assert!(durable
        .accept_layer_stack_merge(malicious_layer, stack)
        .is_err());
    assert_eq!(
        durable.layer_stack_head(stack.layer_stack_id).unwrap(),
        Some(stack)
    );
    let merged_stack = match durable.accept_layer_stack_merge(candidate, stack).unwrap() {
        LayerStackMergeOutcome::DurablyAccepted { head, .. } => head,
        other => panic!("Durable LayerStack merge failed: {other:?}"),
    };
    assert_eq!(merged_stack.layer_id, candidate.layer_id);
    assert_eq!(durable.branch_head(branch_id).unwrap(), Some(continued));
    match durable
        .layer_stack_rollback(merged_stack, stack.layer_id)
        .unwrap()
    {
        LayerStackRollbackOutcome::DurablyAccepted { head, .. } => {
            assert_eq!(head.layer_id, stack.layer_id)
        }
        other => panic!("Durable LayerStack rollback failed: {other:?}"),
    }
    assert_eq!(durable.branch_head(branch_id).unwrap(), Some(continued));

    let child = working_b
        .create_child_branch(
            BranchId::from_bytes([0x50; 32]),
            Some("child"),
            continued_record,
        )
        .unwrap();
    let child_begin = working_b.begin_operation(child).unwrap();
    let child_candidate = no_change(&child_begin, root);
    let child_head = match working_b
        .operation_commit(child_begin, child_candidate)
        .unwrap()
    {
        CommitResult::WorkingRecorded { head, .. } => head,
        CommitResult::Conflict { .. } => panic!("child operation conflicted"),
    };
    let child_push = [0x51; 32];
    push_objects(
        &working_b,
        &endpoint,
        child_push,
        object_ids.iter().copied(),
        ResumeToken::default(),
    )
    .unwrap();
    assert!(matches!(
        push_branch(
            &working_b,
            &endpoint,
            child_push,
            child.branch_id,
            None,
            ResumeToken::default(),
        )
        .unwrap()
        .outcome,
        BranchPushOutcome::DurablyAccepted { head, .. } if head == child_head
    ));

    let parent_begin = working_b.begin_operation(continued).unwrap();
    let parent_candidate = no_change(&parent_begin, root);
    let parent_next = match working_b
        .operation_commit(parent_begin, parent_candidate)
        .unwrap()
    {
        CommitResult::WorkingRecorded { head, .. } => head,
        CommitResult::Conflict { .. } => panic!("parent operation conflicted"),
    };
    let parent_push = [0x52; 32];
    push_objects(
        &working_b,
        &endpoint,
        parent_push,
        object_ids.iter().copied(),
        ResumeToken::default(),
    )
    .unwrap();
    assert!(matches!(
        push_branch(
            &working_b,
            &endpoint,
            parent_push,
            branch_id,
            Some(continued),
            ResumeToken::default(),
        )
        .unwrap()
        .outcome,
        BranchPushOutcome::DurablyAccepted { head, .. } if head == parent_next
    ));

    let (merged_parent, publication) = match working_b
        .child_branch_merge(child_head, parent_next)
        .unwrap()
    {
        ChildMergeResult::WorkingRecorded {
            parent_head,
            publication,
            ..
        } => (parent_head, publication),
        other => panic!("Working ChildBranchMerge failed: {other:?}"),
    };
    assert!(matches!(
        push_branch(
            &working_b,
            &endpoint,
            [0x59; 32],
            branch_id,
            Some(parent_next),
            ResumeToken::default(),
        )
        .unwrap()
        .outcome,
        BranchPushOutcome::DurablyAccepted { head, .. } if head == merged_parent
    ));
    assert_eq!(durable.branch_head(branch_id).unwrap(), Some(merged_parent));
    let mut malicious_merge = publication.clone();
    malicious_merge.candidate.result_root = alternate_root;
    malicious_merge.accepted_parent.root = alternate_root;
    malicious_merge.accepted_parent.operation_version_id =
        Some(layerfs_storage::product::OperationVersionId::from_bytes(
            layerfs_storage::product::derive_id(
                b"child-merge-operation-version",
                &[
                    parent_next.branch_id.as_bytes(),
                    malicious_merge.candidate.request_id.as_bytes(),
                    alternate_root.as_bytes(),
                ],
            ),
        ));
    assert!(push_child_branch_merge(&endpoint, malicious_merge).is_err());
    assert_eq!(durable.branch_head(branch_id).unwrap(), Some(merged_parent));
    assert!(matches!(
        push_child_branch_merge(&endpoint, publication.clone()).unwrap(),
        ChildMergeOutcome::WorkingRecorded { parent_head, .. } if parent_head == merged_parent
    ));
    assert!(matches!(
        push_child_branch_merge(&endpoint, publication).unwrap(),
        ChildMergeOutcome::WorkingRecorded {
            parent_head,
            reconciled: true,
        } if parent_head == merged_parent
    ));
    assert_eq!(
        durable.branch_head(child.branch_id).unwrap(),
        Some(child_head)
    );

    let after_merge_begin = working_b.begin_operation(merged_parent).unwrap();
    let after_merge_candidate = no_change(&after_merge_begin, root);
    let after_merge = match working_b
        .operation_commit(after_merge_begin, after_merge_candidate)
        .unwrap()
    {
        CommitResult::WorkingRecorded { head, .. } => head,
        CommitResult::Conflict { .. } => panic!("post-merge operation conflicted"),
    };
    let after_merge_push = [0x53; 32];
    push_objects(
        &working_b,
        &endpoint,
        after_merge_push,
        object_ids.iter().copied(),
        ResumeToken::default(),
    )
    .unwrap();
    assert!(matches!(
        push_branch(
            &working_b,
            &endpoint,
            after_merge_push,
            branch_id,
            Some(merged_parent),
            ResumeToken::default(),
        )
        .unwrap()
        .outcome,
        BranchPushOutcome::DurablyAccepted { head, .. } if head == after_merge
    ));

    let rollback_source_begin = working_b.begin_operation(after_merge).unwrap();
    let rollback_source_candidate = no_change(&rollback_source_begin, root);
    let rollback_source = match working_b
        .operation_commit(rollback_source_begin, rollback_source_candidate)
        .unwrap()
    {
        CommitResult::WorkingRecorded { head, .. } => head,
        CommitResult::Conflict { .. } => panic!("pre-rollback operation conflicted"),
    };
    let rollback_source_push = [0x54; 32];
    push_objects(
        &working_b,
        &endpoint,
        rollback_source_push,
        object_ids.iter().copied(),
        ResumeToken::default(),
    )
    .unwrap();
    assert!(matches!(
        push_branch(
            &working_b,
            &endpoint,
            rollback_source_push,
            branch_id,
            Some(after_merge),
            ResumeToken::default(),
        )
        .unwrap()
        .outcome,
        BranchPushOutcome::DurablyAccepted { head, .. } if head == rollback_source
    ));
    let (rolled_back, rollback_publication) = match working_b
        .branch_rollback(rollback_source, after_merge.operation_version_id.unwrap())
        .unwrap()
    {
        BranchRollbackResult::WorkingRecorded {
            head, publication, ..
        } => (head, publication),
        other => panic!("Working BranchRollback failed: {other:?}"),
    };
    assert!(matches!(
        push_branch_rollback(&endpoint, rollback_publication).unwrap(),
        BranchRollbackOutcome::WorkingRecorded { head, .. } if head == rolled_back
    ));
    assert!(matches!(
        push_branch_rollback(&endpoint, rollback_publication).unwrap(),
        BranchRollbackOutcome::WorkingRecorded {
            head,
            reconciled: true,
        } if head == rolled_back
    ));
    assert_eq!(durable.branch_head(branch_id).unwrap(), Some(rolled_back));
    let durable_stack = durable
        .layer_stack_head(stack.layer_stack_id)
        .unwrap()
        .unwrap();
    let resumed_begin = working_b.begin_operation(rolled_back).unwrap();
    let resumed_candidate = no_change(&resumed_begin, root);
    let resumed_head = match working_b
        .operation_commit(resumed_begin, resumed_candidate)
        .unwrap()
    {
        CommitResult::WorkingRecorded { head, .. } => head,
        CommitResult::Conflict { .. } => panic!("post-rollback operation conflicted"),
    };
    let resumed_push = [0x55; 32];
    push_objects(
        &working_b,
        &endpoint,
        resumed_push,
        object_ids.iter().copied(),
        ResumeToken::default(),
    )
    .unwrap();
    assert!(matches!(
        push_branch(
            &working_b,
            &endpoint,
            resumed_push,
            branch_id,
            Some(rolled_back),
            ResumeToken::default(),
        )
        .unwrap()
        .outcome,
        BranchPushOutcome::DurablyAccepted { head, .. } if head == resumed_head
    ));
    assert!(matches!(
        push_branch(
            &working,
            &endpoint,
            first_push,
            branch_id,
            None,
            ResumeToken::default(),
        )
        .unwrap()
        .outcome,
        BranchPushOutcome::DurablyAccepted {
            head,
            reconciled: true,
        } if head == accepted
    ));
    assert_eq!(durable.branch_head(branch_id).unwrap(), Some(resumed_head));

    let working_c_path = base.join("working-c");
    let working_c = WorkingStore::open(&working_c_path, IntegrityMode::Verified).unwrap();
    let reconstructed = fetch_branch(
        &endpoint,
        &working_c,
        [0x56; 32],
        branch_id,
        ResumeToken::default(),
    )
    .unwrap();
    assert_eq!(reconstructed.head, resumed_head);
    assert_eq!(
        working_c.branch_head(child.branch_id).unwrap(),
        Some(child_head)
    );
    assert_eq!(
        working_c.branch_head(branch_id).unwrap(),
        Some(resumed_head)
    );
    assert_eq!(
        working_c.layer_stack_head(stack.layer_stack_id).unwrap(),
        Some(durable_stack)
    );
    let working_child =
        WorkingStore::open(&base.join("working-child-only"), IntegrityMode::Verified).unwrap();
    let child_reconstructed = fetch_branch(
        &endpoint,
        &working_child,
        [0x57; 32],
        child.branch_id,
        ResumeToken::default(),
    )
    .unwrap();
    assert_eq!(child_reconstructed.head, child_head);
    assert_eq!(
        working_child.branch_head(branch_id).unwrap(),
        Some(resumed_head)
    );
    drop(working_child);
    let reconstructed_begin = working_c.begin_operation(resumed_head).unwrap();
    let reconstructed_candidate = no_change(&reconstructed_begin, root);
    assert!(matches!(
        working_c
            .operation_commit(reconstructed_begin, reconstructed_candidate)
            .unwrap(),
        CommitResult::WorkingRecorded { .. }
    ));

    let backup = base.join("durable-backup.sqlite");
    durable.backup(&backup).unwrap();
    let restored = DurableStore::restore(&backup, &base.join("durable-restored")).unwrap();
    assert_eq!(restored.storage_id(), durable.storage_id());
    assert_eq!(restored.branch_head(branch_id).unwrap(), Some(resumed_head));
    assert_eq!(
        restored.branch_head(child.branch_id).unwrap(),
        Some(child_head)
    );
    assert_eq!(
        restored.layer_stack_head(stack.layer_stack_id).unwrap(),
        Some(durable_stack)
    );
    let restored_endpoint = LocalDurable::new(&restored);
    let working_d = WorkingStore::open(&base.join("working-d"), IntegrityMode::Verified).unwrap();
    let restored_fetch = fetch_branch(
        &restored_endpoint,
        &working_d,
        [0x58; 32],
        branch_id,
        ResumeToken::default(),
    )
    .unwrap();
    assert_eq!(restored_fetch.head, resumed_head);
    assert_eq!(
        working_d.branch_head(child.branch_id).unwrap(),
        Some(child_head)
    );
    assert_eq!(
        working_d.layer_stack_head(stack.layer_stack_id).unwrap(),
        Some(durable_stack)
    );

    drop(working_d);
    let restored_id = restored.storage_id();
    let restored = restored.compact().unwrap();
    assert_eq!(restored.storage_id(), restored_id);
    assert_eq!(restored.branch_head(branch_id).unwrap(), Some(resumed_head));
    assert!(restored
        .database_path()
        .ends_with("generation-0000000000000001.sqlite"));
    drop(restored);
    drop(working_c);
    drop(working_b);
    drop(durable);
    drop(working);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn nonzero_lost_ack_reconciles_after_the_durable_branch_advances() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-sync-nonzero-lost-ack-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    let mut working =
        WorkingStore::open(&base.join("working"), IntegrityMode::TrustedLocalDev).unwrap();
    let root = valid_empty_root(&mut working);
    let stack = working
        .create_layer_stack(
            LayerStackId::from_bytes([0xb1; 32]),
            LayerId::from_bytes([0xb2; 32]),
            "lost-ack",
            root,
        )
        .unwrap();
    let branch = working
        .create_top_level_branch(BranchId::from_bytes([0xb3; 32]), Some("lost-ack"), stack)
        .unwrap();
    let durable = DurableStore::open(&base.join("durable")).unwrap();
    let endpoint = LocalDurable::new(&durable);
    let mut origin_objects = Vec::new();
    let mut after = None;
    loop {
        let page = working.object_ids_page(after, 16).unwrap();
        if page.is_empty() {
            break;
        }
        after = page.last().copied();
        origin_objects.extend(page);
    }
    push_objects(
        &working,
        &endpoint,
        [0xb4; 32],
        origin_objects,
        ResumeToken::default(),
    )
    .unwrap();
    durable
        .bootstrap_layer_stack(stack.layer_stack_id, stack.layer_id, "lost-ack", root)
        .unwrap();

    let first_begin = working.begin_operation(branch).unwrap();
    let first_candidate = new_file_candidate(&working, &first_begin, b"nonzero lost ack");
    let first = match working
        .operation_commit(first_begin, first_candidate)
        .unwrap()
    {
        CommitResult::WorkingRecorded { head, .. } => head,
        CommitResult::Conflict { .. } => panic!("first operation conflicted"),
    };
    let first_request = [0xb5; 32];
    let lossy = LoseFirstPushAcknowledgement {
        durable: &durable,
        lose: Cell::new(true),
        unique_bytes: Cell::new(0),
    };
    assert!(matches!(
        push_branch(
            &working,
            &lossy,
            first_request,
            branch.branch_id,
            None,
            ResumeToken::default(),
        ),
        Err(SyncError::Destination(_))
    ));
    assert!(lossy.unique_bytes.get() > 0);
    assert_eq!(durable.branch_head(branch.branch_id).unwrap(), Some(first));
    assert_eq!(
        durable
            .sync_custody_rows(RequestId::from_bytes(first_request), "push")
            .unwrap(),
        1
    );

    let later_begin = working.begin_operation(first).unwrap();
    let later = match working
        .operation_commit(later_begin, no_change(&later_begin, first.root))
        .unwrap()
    {
        CommitResult::WorkingRecorded { head, .. } => head,
        CommitResult::Conflict { .. } => panic!("later operation conflicted"),
    };
    assert!(matches!(
        push_branch(
            &working,
            &endpoint,
            [0xb6; 32],
            branch.branch_id,
            Some(first),
            ResumeToken::default(),
        )
        .unwrap()
        .outcome,
        BranchPushOutcome::DurablyAccepted { head, .. } if head == later
    ));
    assert_eq!(durable.branch_head(branch.branch_id).unwrap(), Some(later));

    let reconciled = push_branch(
        &working,
        &endpoint,
        first_request,
        branch.branch_id,
        None,
        ResumeToken::default(),
    )
    .unwrap();
    assert_eq!(
        reconciled.outcome,
        BranchPushOutcome::DurablyAccepted {
            head: first,
            reconciled: true,
        }
    );
    assert_eq!(reconciled.transfer.unique_bytes, 0);
    assert_eq!(durable.branch_head(branch.branch_id).unwrap(), Some(later));
    assert_eq!(
        working
            .push_outbox_state(RequestId::from_bytes(first_request))
            .unwrap()
            .as_deref(),
        Some("accepted")
    );

    drop(durable);
    drop(working);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn mixed_merge_and_rollback_history_pushes_after_durable_prerequisites() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-offline-special-push-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    let mut working = WorkingStore::open(&base.join("working"), IntegrityMode::Verified).unwrap();
    let root = valid_empty_root(&mut working);
    let stack = working
        .create_layer_stack(
            LayerStackId::from_bytes([0xa1; 32]),
            LayerId::from_bytes([0xa2; 32]),
            "offline-special",
            root,
        )
        .unwrap();
    let parent = working
        .create_top_level_branch(
            BranchId::from_bytes([0xa3; 32]),
            Some("offline-parent"),
            stack,
        )
        .unwrap();
    let first_begin = working.begin_operation(parent).unwrap();
    let (first, fork_record) = match working
        .operation_commit(first_begin, no_change(&first_begin, root))
        .unwrap()
    {
        CommitResult::WorkingRecorded { head, record, .. } => (head, record),
        other => panic!("first operation failed: {other:?}"),
    };
    let child = working
        .create_child_branch(
            BranchId::from_bytes([0xa4; 32]),
            Some("offline-child"),
            fork_record,
        )
        .unwrap();
    let durable = DurableStore::open(&base.join("durable")).unwrap();
    let endpoint = LocalDurable::new(&durable);
    push_layer_stack_genesis(
        &working,
        &endpoint,
        [0xa5; 32],
        parent.branch_id,
        stack,
        "offline-special",
        ResumeToken::default(),
    )
    .unwrap();
    assert!(matches!(
        push_branch(
            &working,
            &endpoint,
            [0xa6; 32],
            parent.branch_id,
            None,
            ResumeToken::default(),
        )
        .unwrap()
        .outcome,
        BranchPushOutcome::DurablyAccepted { head, .. } if head == first
    ));
    let child_begin = working.begin_operation(child).unwrap();
    let child_head = match working
        .operation_commit(child_begin, no_change(&child_begin, root))
        .unwrap()
    {
        CommitResult::WorkingRecorded { head, .. } => head,
        other => panic!("child operation failed: {other:?}"),
    };
    assert!(matches!(
        push_branch(
            &working,
            &endpoint,
            [0xa7; 32],
            child.branch_id,
            None,
            ResumeToken::default(),
        )
        .unwrap()
        .outcome,
        BranchPushOutcome::DurablyAccepted { head, .. } if head == child_head
    ));
    let parent_begin = working.begin_operation(first).unwrap();
    let parent_before_merge = match working
        .operation_commit(parent_begin, no_change(&parent_begin, root))
        .unwrap()
    {
        CommitResult::WorkingRecorded { head, .. } => head,
        other => panic!("parent operation failed: {other:?}"),
    };
    let merged = match working
        .child_branch_merge(child_head, parent_before_merge)
        .unwrap()
    {
        ChildMergeResult::WorkingRecorded { parent_head, .. } => parent_head,
        other => panic!("offline merge failed: {other:?}"),
    };
    let later_begin = working.begin_operation(merged).unwrap();
    let later = match working
        .operation_commit(later_begin, no_change(&later_begin, root))
        .unwrap()
    {
        CommitResult::WorkingRecorded { head, .. } => head,
        other => panic!("post-merge operation failed: {other:?}"),
    };
    let rolled_back = match working
        .branch_rollback(later, merged.operation_version_id.unwrap())
        .unwrap()
    {
        BranchRollbackResult::WorkingRecorded { head, .. } => head,
        other => panic!("offline rollback failed: {other:?}"),
    };

    let request = [0xa8; 32];
    let pushed = push_branch(
        &working,
        &endpoint,
        request,
        parent.branch_id,
        Some(first),
        ResumeToken::default(),
    )
    .unwrap();
    assert!(matches!(
        pushed.outcome,
        BranchPushOutcome::DurablyAccepted { head, .. } if head == rolled_back
    ));
    assert_eq!(
        durable.branch_head(parent.branch_id).unwrap(),
        Some(rolled_back)
    );
    assert_eq!(
        durable.branch_head(child.branch_id).unwrap(),
        Some(child_head)
    );
    let replayed = push_branch(
        &working,
        &endpoint,
        request,
        parent.branch_id,
        Some(first),
        ResumeToken::default(),
    )
    .unwrap();
    assert!(
        matches!(
            replayed.outcome,
            BranchPushOutcome::DurablyAccepted {
                head,
                reconciled: true,
            } if head == rolled_back
        ),
        "{:?}",
        replayed.outcome
    );

    let fresh = WorkingStore::open(&base.join("fresh"), IntegrityMode::Verified).unwrap();
    assert_eq!(
        fetch_branch(
            &endpoint,
            &fresh,
            [0xa9; 32],
            parent.branch_id,
            ResumeToken::default(),
        )
        .unwrap()
        .head,
        rolled_back
    );
    assert_eq!(
        fresh.branch_head(child.branch_id).unwrap(),
        Some(child_head)
    );
    drop(fresh);
    drop(durable);
    drop(working);
    fs::remove_dir_all(base).unwrap();
}

fn no_change(begin: &BeginOperation, root: layerfs_core::ObjectId) -> WorkingCandidate {
    WorkingCandidate {
        operation_id: begin.operation_id,
        expected_branch_generation: begin.branch_head_before.generation,
        base_root: begin.base.root(),
        candidate_root: root,
        normalized_transition: Vec::new(),
    }
}

fn new_file_candidate(
    working: &WorkingStore,
    begin: &BeginOperation,
    bytes: &[u8],
) -> WorkingCandidate {
    let mut writer = working.begin_candidate_write().unwrap();
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
            begin.base.root(),
            &layerfs_core::CanonicalPath::new("file").unwrap(),
            Cursor::new(bytes),
            (inode, metadata),
        )
        .unwrap();
    let candidate_root = candidate.root();
    writer
        .commit_trusted_operation_candidate(begin.operation_id, candidate)
        .unwrap();
    WorkingCandidate {
        operation_id: begin.operation_id,
        expected_branch_generation: begin.branch_head_before.generation,
        base_root: begin.base.root(),
        candidate_root,
        normalized_transition: Vec::new(),
    }
}

struct LoseFirstPushAcknowledgement<'a> {
    durable: &'a DurableStore,
    lose: Cell<bool>,
    unique_bytes: Cell<u64>,
}

impl DurableEndpoint for LoseFirstPushAcknowledgement<'_> {
    fn durable_storage_id(&self) -> [u8; 32] {
        self.durable.storage_id()
    }

    fn read_object(
        &self,
        id: layerfs_core::ObjectId,
        maximum: usize,
    ) -> layerfs_sync::Result<Vec<u8>> {
        self.durable
            .sync_read_object(id, maximum)
            .map_err(|error| SyncError::Source(error.to_string()))
    }

    fn contains_object(&self, id: layerfs_core::ObjectId) -> layerfs_sync::Result<bool> {
        self.durable
            .sync_has_object(id)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn accept_objects(
        &self,
        owner_request_id: layerfs_storage::product::RequestId,
        request_id: layerfs_storage::product::RequestId,
        direction: layerfs_sync::Direction,
        objects: &[(layerfs_core::ObjectId, Vec<u8>)],
    ) -> layerfs_sync::Result<()> {
        self.durable
            .sync_accept_objects(
                owner_request_id,
                request_id,
                match direction {
                    layerfs_sync::Direction::Fetch => "fetch",
                    layerfs_sync::Direction::Push => "push",
                },
                objects,
            )
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn abort_transfer(
        &self,
        owner_request_id: layerfs_storage::product::RequestId,
        direction: layerfs_sync::Direction,
    ) -> layerfs_sync::Result<u64> {
        self.durable
            .abort_sync_transfer(
                owner_request_id,
                match direction {
                    layerfs_sync::Direction::Fetch => "fetch",
                    layerfs_sync::Direction::Push => "push",
                },
            )
            .map_err(|error| SyncError::Destination(error.to_string()))
    }
}

impl DurableControlEndpoint for LoseFirstPushAcknowledgement<'_> {
    fn branch_head(
        &self,
        branch_id: BranchId,
    ) -> layerfs_sync::Result<Option<layerfs_sync::BranchHead>> {
        self.durable
            .branch_head(branch_id)
            .map_err(|error| SyncError::Source(error.to_string()))
    }
    fn bootstrap_layer_stack(
        &self,
        stack: layerfs_sync::LayerStackId,
        layer: layerfs_sync::LayerId,
        name: &str,
        root: layerfs_core::ObjectId,
    ) -> layerfs_sync::Result<layerfs_sync::LayerStackHead> {
        self.durable
            .bootstrap_layer_stack(stack, layer, name, root)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn stage_branch_push_page(
        &self,
        transfer_id: layerfs_storage::product::RequestId,
        page_sequence: u64,
        data_request_id: layerfs_storage::product::RequestId,
        bundle: &layerfs_storage::product::BranchPushBundle,
        counters: layerfs_storage::product::SyncTransferCounters,
    ) -> layerfs_sync::Result<()> {
        self.durable
            .stage_branch_push_page(
                transfer_id,
                page_sequence,
                data_request_id,
                bundle,
                counters,
            )
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn commit_staged_branch_push(
        &self,
        request: layerfs_storage::product::BranchPushRequest,
        branch_id: BranchId,
    ) -> layerfs_sync::Result<BranchPushOutcome> {
        self.unique_bytes.set(request.counters.unique_bytes);
        let outcome = self
            .durable
            .commit_staged_branch_push(request, branch_id)
            .map_err(|error| SyncError::Destination(error.to_string()))?;
        if self.lose.replace(false) {
            return Err(SyncError::Destination(
                "injected lost Push acknowledgement".into(),
            ));
        }
        Ok(outcome)
    }

    fn reconcile_branch_push(
        &self,
        request_id: layerfs_storage::product::RequestId,
        expected: Option<layerfs_sync::BranchHead>,
        accepted: layerfs_sync::BranchHead,
    ) -> layerfs_sync::Result<BranchPushOutcome> {
        self.durable
            .reconcile_branch_push(request_id, expected, accepted)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn export_branch_fetch(
        &self,
        branch_id: BranchId,
        base: Option<layerfs_sync::BranchHead>,
        origin_stack_base: Option<layerfs_sync::LayerStackHead>,
    ) -> layerfs_sync::Result<layerfs_storage::product::BranchPushBundle> {
        self.durable
            .export_branch_fetch(branch_id, base, origin_stack_base)
            .map_err(|error| SyncError::Source(error.to_string()))
    }

    fn branch_fetch_object_page(
        &self,
        branch_id: BranchId,
        base: Option<layerfs_sync::BranchHead>,
        origin_stack_base: Option<layerfs_sync::LayerStackHead>,
        expected_head: layerfs_sync::BranchHead,
        expected_stack_head: layerfs_sync::LayerStackHead,
        after: Option<layerfs_core::ObjectId>,
        limit: usize,
    ) -> layerfs_sync::Result<Vec<layerfs_core::ObjectId>> {
        self.durable
            .branch_fetch_object_page(
                branch_id,
                base,
                origin_stack_base,
                expected_head,
                expected_stack_head,
                after,
                limit,
            )
            .map_err(|error| SyncError::Source(error.to_string()))
    }

    fn accept_child_branch_merge(
        &self,
        publication: layerfs_storage::product::ChildMergePublication,
    ) -> layerfs_sync::Result<layerfs_storage::product::ChildMergeOutcome> {
        self.durable
            .accept_child_branch_merge(publication)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn accept_branch_rollback(
        &self,
        publication: layerfs_storage::product::BranchRollbackPublication,
    ) -> layerfs_sync::Result<layerfs_storage::product::BranchRollbackOutcome> {
        self.durable
            .accept_branch_rollback(publication)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn accept_layer_stack_merge(
        &self,
        candidate: layerfs_sync::LayerCandidate,
        expected: layerfs_sync::LayerStackHead,
    ) -> layerfs_sync::Result<layerfs_sync::LayerStackMergeOutcome> {
        self.durable
            .accept_layer_stack_merge(candidate, expected)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn layer_stack_rollback(
        &self,
        expected: layerfs_sync::LayerStackHead,
        target: layerfs_sync::LayerId,
    ) -> layerfs_sync::Result<layerfs_sync::LayerStackRollbackOutcome> {
        self.durable
            .layer_stack_rollback(expected, target)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }
}

fn valid_empty_root(working: &mut WorkingStore) -> layerfs_core::ObjectId {
    let mut publication = working.begin_candidate_write().unwrap();
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
    let inode = publication.allocate_inode_id().unwrap();
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
