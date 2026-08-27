use layerfs_sdk::{
    BranchId, CommitResult, IntegrityMode, LayerFs, LayerId, LayerStackId, OperationVersionId,
    VersionRef,
};
use std::fs;
use std::io::Cursor;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn shipped_direct_route_uses_isolated_operations_and_working_commit() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-sdk-product-direct-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    let fs = LayerFs::open(&base, IntegrityMode::TrustedLocalDev).unwrap();
    let root = fs.initialize_empty_root().unwrap();
    let stack = fs
        .create_layer_stack(
            LayerStackId::from_bytes([0x11; 32]),
            LayerId::from_bytes([0x12; 32]),
            "main",
            root,
        )
        .unwrap();
    let branch = fs
        .create_top_level_branch(BranchId::from_bytes([0x13; 32]), Some("work"), stack)
        .unwrap();
    fs.reset_working_storage_counters().unwrap();
    let mut first = fs.begin_direct(branch).unwrap();
    let mut stale = fs.begin_direct(branch).unwrap();
    first
        .replace_file("file", Cursor::new(b"first candidate"))
        .unwrap();
    stale
        .replace_file("file", Cursor::new(b"stale candidate"))
        .unwrap();
    let first_receipt = first.commit().unwrap();
    assert!(first_receipt.cleanup.is_ok());
    assert!(first_receipt
        .acknowledgement
        .as_ref()
        .is_some_and(Result::is_ok));
    assert!(first_receipt.timers.equation_closed);
    assert_eq!(
        first_receipt.timers.working_recorded_ns,
        first_receipt.timers.quiescence_ns
            + first_receipt.timers.candidate_ns
            + first_receipt.timers.working_commit_ns
    );
    assert_eq!(
        first_receipt.candidate_root,
        first_receipt
            .cleanup
            .as_ref()
            .unwrap()
            .candidate_root
            .unwrap()
    );
    let accepted = match first_receipt.outcome {
        CommitResult::WorkingRecorded { head, .. } => head,
        CommitResult::Conflict { .. } => panic!("first direct operation conflicted"),
    };
    let locality = fs.working_storage_counters().unwrap();
    assert_eq!(locality.candidate_full_scans, 0, "{locality:?}");
    assert!(locality.candidate_shallow_bindings >= 3, "{locality:?}");
    let stale_root = stale.candidate_root();
    let stale_receipt = stale.commit().unwrap();
    assert!(stale_receipt.cleanup.is_ok());
    let stale_id = match stale_receipt.outcome {
        CommitResult::Conflict { actual, candidate } => {
            assert_eq!(actual, accepted);
            assert_eq!(candidate.root, stale_root);
            candidate.operation_id
        }
        CommitResult::WorkingRecorded { .. } => panic!("stale operation was accepted"),
    };
    assert_eq!(
        fs.recoverable_operations(16).unwrap()[0].operation_id,
        stale_id
    );
    fs.discard_recovered_operation(stale_id).unwrap();
    let mut namespace = fs.begin_direct(accepted).unwrap();
    namespace.create_directory("dir").unwrap();
    namespace.hard_link("file", "dir/link").unwrap();
    namespace.rename("dir/link", "dir/moved").unwrap();
    namespace.create_symlink("sym", b"dir/moved").unwrap();
    namespace
        .replace_file("temporary", Cursor::new(b"remove"))
        .unwrap();
    namespace.remove("temporary").unwrap();
    let namespace_head = match namespace.commit().unwrap().outcome {
        CommitResult::WorkingRecorded { head, .. } => head,
        CommitResult::Conflict { .. } => panic!("namespace operation conflicted"),
    };
    let mut current = Vec::new();
    fs.stream(
        fs.pin_branch_version(namespace_head).unwrap(),
        "dir/moved",
        &mut current,
    )
    .unwrap();
    assert_eq!(current, b"first candidate");
    assert!(fs
        .stream(
            VersionRef::OperationVersion {
                branch_id: namespace_head.branch_id,
                operation_version_id: OperationVersionId::from_bytes([0xee; 32]),
                root: namespace_head.root,
            },
            "file",
            Vec::new(),
        )
        .is_err());
    assert_eq!(
        fs.readlink(fs.pin_branch_version(namespace_head).unwrap(), "sym")
            .unwrap()
            .0,
        b"dir/moved"
    );
    assert!(fs
        .stream(
            VersionRef::Layer {
                layer_stack_id: stack.layer_stack_id,
                layer_id: stack.layer_id,
                root,
            },
            "file",
            Vec::new(),
        )
        .is_err());
    assert_eq!(
        fs.branch_head(branch.branch_id).unwrap(),
        Some(namespace_head)
    );

    let mut discarded = fs.begin_direct(namespace_head).unwrap();
    discarded
        .replace_range("file", 0, 5, Cursor::new(b"drop!"))
        .unwrap();
    drop(discarded);
    assert!(fs.recoverable_operations(16).unwrap().is_empty());

    let mut crashed = fs.begin_direct(namespace_head).unwrap();
    crashed
        .replace_range("file", 0, 5, Cursor::new(b"crash"))
        .unwrap();
    let crashed_id = crashed.operation_id();
    let crashed_root = crashed.candidate_root();
    std::mem::forget(crashed);

    drop(fs);
    let reopened = LayerFs::open(&base, IntegrityMode::TrustedLocalDev).unwrap();
    let recovery = reopened.recoverable_operations(16).unwrap();
    assert_eq!(recovery.len(), 1);
    assert_eq!(recovery[0].operation_id, crashed_id);
    assert_eq!(recovery[0].candidate_root, Some(crashed_root));
    reopened.discard_recovered_operation(crashed_id).unwrap();
    assert!(reopened.recoverable_operations(16).unwrap().is_empty());
    assert_eq!(
        reopened.branch_head(branch.branch_id).unwrap(),
        Some(namespace_head)
    );
    drop(reopened);
    fs::remove_dir_all(base).unwrap();
}
