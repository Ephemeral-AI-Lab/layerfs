mod helpers;

use helpers::{no_change, valid_empty_root};
use layerfs_durable_store::DurableStore;
use layerfs_storage::integrity::IntegrityMode;
use layerfs_storage::{BranchId, BranchPushOutcome, LayerId, LayerStackId};
use layerfs_sync::ResumeToken;
use layerfs_sync::{fetch_branch, push_branch, push_layer_stack_genesis, LocalDurable};
use layerfs_working_store::{BranchRollbackResult, ChildMergeResult, CommitResult, WorkingStore};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn offline_merge_and_rollback_history_pushes_to_an_absent_durable_store() {
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
    let child_begin = working.begin_operation(child).unwrap();
    let child_head = match working
        .operation_commit(child_begin, no_change(&child_begin, root))
        .unwrap()
    {
        CommitResult::WorkingRecorded { head, .. } => head,
        other => panic!("child operation failed: {other:?}"),
    };
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
    let request = [0xa6; 32];
    assert!(matches!(
        push_branch(
            &working,
            &endpoint,
            request,
            parent.branch_id,
            None,
            ResumeToken::default(),
        )
        .unwrap()
        .outcome,
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
    assert!(matches!(
        push_branch(
            &working,
            &endpoint,
            request,
            parent.branch_id,
            None,
            ResumeToken::default(),
        )
        .unwrap()
        .outcome,
        BranchPushOutcome::DurablyAccepted {
            head,
            reconciled: true,
        } if head == rolled_back
    ));

    let fresh = WorkingStore::open(&base.join("fresh"), IntegrityMode::Verified).unwrap();
    assert_eq!(
        fetch_branch(
            &endpoint,
            &fresh,
            [0xa7; 32],
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
