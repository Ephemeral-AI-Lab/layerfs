use layerfs_sdk::{
    BranchId, BranchRollbackOutcome, BranchRollbackResult, ChildMergeOutcome, ChildMergeResult,
    CommitResult, IntegrityMode, LayerFs, LayerId, LayerStackId, ResumeToken,
};
use layerfs_service::Service;
use std::fs;
use std::io::Cursor;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn shipped_facade_pushes_merge_rollback_and_fresh_fetch_history() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-sdk-product-history-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    let fs = LayerFs::open(&base.join("working"), IntegrityMode::TrustedLocalDev).unwrap();
    let root = fs.initialize_empty_root().unwrap();
    let stack = fs
        .create_layer_stack(
            LayerStackId::from_bytes([0x61; 32]),
            LayerId::from_bytes([0x62; 32]),
            "main",
            root,
        )
        .unwrap();
    let branch = fs
        .create_top_level_branch(BranchId::from_bytes([0x63; 32]), Some("main"), stack)
        .unwrap();
    let mut parent_operation = fs.begin_direct(branch).unwrap();
    parent_operation
        .replace_file("file", Cursor::new(b"parent"))
        .unwrap();
    let (parent, parent_record) = match parent_operation.commit().unwrap().outcome {
        CommitResult::WorkingRecorded { head, record, .. } => (head, record),
        CommitResult::Conflict { .. } => panic!("parent conflicted"),
    };

    let bearer = [0x64; 32];
    let service = Service::open(&base.join("service"), &bearer).unwrap();
    let durable = service.authenticate(&bearer).unwrap();
    fs.push_layer_stack_genesis(
        &durable,
        [0x60; 32],
        branch.branch_id,
        stack,
        "main",
        ResumeToken::default(),
    )
    .unwrap();
    fs.push_branch(
        &durable,
        [0x65; 32],
        branch.branch_id,
        None,
        ResumeToken::default(),
    )
    .unwrap();

    let child = fs
        .create_child_branch(
            BranchId::from_bytes([0x66; 32]),
            Some("child"),
            parent_record,
        )
        .unwrap();
    let mut child_operation = fs.begin_direct(child).unwrap();
    child_operation
        .replace_range("file", 0, 6, Cursor::new(b"child!"))
        .unwrap();
    let child_head = match child_operation.commit().unwrap().outcome {
        CommitResult::WorkingRecorded { head, .. } => head,
        CommitResult::Conflict { .. } => panic!("child conflicted"),
    };
    fs.push_branch(
        &durable,
        [0x67; 32],
        child.branch_id,
        None,
        ResumeToken::default(),
    )
    .unwrap();

    let (merged, merge_publication) = match fs.child_branch_merge(child_head, parent).unwrap() {
        ChildMergeResult::WorkingRecorded {
            parent_head,
            publication,
            ..
        } => (parent_head, publication),
        other => panic!("merge failed: {other:?}"),
    };
    assert!(matches!(
        fs.push_child_branch_merge(&durable, merge_publication)
            .unwrap(),
        ChildMergeOutcome::WorkingRecorded { parent_head, .. } if parent_head == merged
    ));
    let (rolled, rollback_publication) = match fs
        .branch_rollback(merged, parent.operation_version_id.unwrap())
        .unwrap()
    {
        BranchRollbackResult::WorkingRecorded {
            head, publication, ..
        } => (head, publication),
        other => panic!("rollback failed: {other:?}"),
    };
    assert!(matches!(
        fs.push_branch_rollback(&durable, rollback_publication)
            .unwrap(),
        BranchRollbackOutcome::WorkingRecorded { head, .. } if head == rolled
    ));

    let fresh = LayerFs::open(&base.join("fresh"), IntegrityMode::Verified).unwrap();
    let fetched = fresh
        .fetch_branch(
            &durable,
            [0x68; 32],
            branch.branch_id,
            ResumeToken::default(),
        )
        .unwrap();
    assert_eq!(fetched.head, rolled);
    assert_eq!(
        fresh.branch_head(child.branch_id).unwrap(),
        Some(child_head)
    );
    assert_eq!(
        fresh.layer_stack_head(stack.layer_stack_id).unwrap(),
        Some(stack)
    );
    let mut bytes = Vec::new();
    fresh
        .stream(
            fresh.pin_branch_version(rolled).unwrap(),
            "file",
            &mut bytes,
        )
        .unwrap();
    assert_eq!(bytes, b"parent");
    drop(fresh);
    drop(service);
    drop(fs);
    fs::remove_dir_all(base).unwrap();
}
