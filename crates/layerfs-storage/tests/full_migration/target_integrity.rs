use super::fixture::{path, publish_valid_root, remove, transfer_all};
use layerfs_storage::{
    migration::migrate_legacy_durable_file, BranchId, BranchPushOutcome, Engine, FullStorage,
    LayerCandidateRequest, LayerId, LayerStackId, LayerStackMergeOutcome,
    LayerStackRollbackOutcome, LeaseId, OperationCandidate, OperationCommitOutcome, OperationId,
    RequestId, VerifiedFetchRequest,
};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

struct Baseline {
    working: PathBuf,
    source: PathBuf,
    candidate: PathBuf,
    storage_id: [u8; 32],
}

fn rich_baseline(label: &str) -> Baseline {
    let working_path = path(&format!("{label}-working"));
    let source = path(&format!("{label}-source"));
    let candidate = path(&format!("{label}-candidate"));
    let working = Engine::open(&working_path).unwrap();
    let (root0, _) = publish_valid_root(&working, "integrity-0", [0xa1; 32]);
    let (root1, _) = publish_valid_root(&working, "integrity-1", [0xa2; 32]);
    let (root2, _) = publish_valid_root(&working, "integrity-2", [0xa3; 32]);
    let stack = working
        .product_create_layer_stack(
            LayerStackId::from_bytes([0xa4; 32]),
            LayerId::from_bytes([0xa5; 32]),
            "integrity",
            root0,
        )
        .unwrap();
    let branch = working
        .product_create_top_level_branch(BranchId::from_bytes([0xa6; 32]), None, stack)
        .unwrap();
    let operation = OperationId::from_bytes([0xa7; 32]);
    working
        .product_begin_operation(operation, branch, LeaseId::from_bytes([0xa8; 32]))
        .unwrap();
    let working_head = match working
        .product_operation_commit(OperationCandidate {
            operation_id: operation,
            expected: branch,
            candidate_root: root1,
            normalized_transition: Vec::new(),
            request_id: RequestId::from_bytes([0xa9; 32]),
        })
        .unwrap()
    {
        OperationCommitOutcome::WorkingRecorded { head, .. } => head,
        OperationCommitOutcome::Conflict { .. } => panic!("unexpected Working conflict"),
    };

    let durable = Engine::open(&source).unwrap();
    let storage_id = durable.store_id().unwrap();
    let fetch_request = RequestId::from_bytes([0xaa; 32]);
    let (_, counters) = transfer_all(
        &working,
        &durable,
        fetch_request,
        RequestId::from_bytes([0xab; 32]),
        "fetch",
    );
    let accepted = match durable
        .product_import_verified_branch_fetch(
            None,
            &working
                .product_export_branch_fetch(branch.branch_id)
                .unwrap(),
            VerifiedFetchRequest {
                request_id: fetch_request,
                durable_storage_id: storage_id,
                counters,
            },
        )
        .unwrap()
    {
        BranchPushOutcome::DurablyAccepted { head, .. } => head,
        BranchPushOutcome::Conflict { .. } => panic!("unexpected Fetch conflict"),
    };
    assert_eq!(accepted, working_head);
    let first = durable
        .product_prepare_layer_candidate(LayerCandidateRequest {
            source: accepted,
            expected_stack: stack,
            result_root: root1,
            source_transition: Vec::new(),
            applied_transition: Vec::new(),
            request_id: RequestId::from_bytes([0xad; 32]),
        })
        .unwrap();
    let stack1 = match durable
        .product_accept_layer_stack_merge(first, stack, RequestId::from_bytes([0xae; 32]))
        .unwrap()
    {
        LayerStackMergeOutcome::DurablyAccepted { head, .. } => head,
        LayerStackMergeOutcome::Conflict { .. } => panic!("unexpected LayerStack conflict"),
    };
    let second = durable
        .product_prepare_layer_candidate(LayerCandidateRequest {
            source: accepted,
            expected_stack: stack1,
            result_root: root2,
            source_transition: Vec::new(),
            applied_transition: Vec::new(),
            request_id: RequestId::from_bytes([0xaf; 32]),
        })
        .unwrap();
    let stack2 = match durable
        .product_accept_layer_stack_merge(second, stack1, RequestId::from_bytes([0xb0; 32]))
        .unwrap()
    {
        LayerStackMergeOutcome::DurablyAccepted { head, .. } => head,
        LayerStackMergeOutcome::Conflict { .. } => panic!("unexpected LayerStack conflict"),
    };
    assert!(matches!(
        durable
            .product_layer_stack_rollback(
                stack2,
                stack.layer_id,
                RequestId::from_bytes([0xb1; 32]),
            )
            .unwrap(),
        LayerStackRollbackOutcome::DurablyAccepted { .. }
    ));
    drop(working);
    drop(durable);
    drop(migrate_legacy_durable_file(&source, &candidate, storage_id).unwrap());
    drop(FullStorage::open_durable_verified(&candidate).unwrap());
    Baseline {
        working: working_path,
        source,
        candidate,
        storage_id,
    }
}

fn copy_and_mutate(source: &Path, label: &str, sql: &str) -> PathBuf {
    let copy = path(label);
    std::fs::copy(source, &copy).unwrap();
    let connection = Connection::open(&copy).unwrap();
    assert_eq!(connection.execute(sql, []).unwrap(), 1);
    assert_eq!(
        connection
            .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert!(!connection
        .prepare("PRAGMA foreign_key_check")
        .unwrap()
        .exists([])
        .unwrap());
    drop(connection);
    copy
}

fn cleanup(baseline: Baseline) {
    remove(&baseline.working);
    remove(&baseline.source);
    remove(&baseline.candidate);
}

#[test]
fn static_full_admission_accepts_but_verified_admission_rejects_semantic_corruption() {
    let baseline = rich_baseline("target-integrity");
    for (label, sql) in [
        (
            "corrupt-full-branch-head",
            "UPDATE layerfs_branches SET generation = generation + 1",
        ),
        (
            "corrupt-full-stack-head",
            "UPDATE layerfs_layer_stacks SET generation = generation + 1",
        ),
        (
            "corrupt-full-release-request",
            "UPDATE layerfs_released_versions SET request_id = zeroblob(32)
             WHERE rowid = (SELECT min(rowid) FROM layerfs_released_versions)",
        ),
        (
            "corrupt-full-release-generation",
            "UPDATE layerfs_released_versions SET release_generation = release_generation + 1
             WHERE rowid = (SELECT min(rowid) FROM layerfs_released_versions)",
        ),
        (
            "corrupt-full-tracking-root",
            "UPDATE layerfs_durable_tracking_refs
             SET root_id = (SELECT object_id FROM layerfs_objects
                            WHERE object_id != layerfs_durable_tracking_refs.root_id
                            ORDER BY object_id LIMIT 1)
             WHERE target_kind = 'branch'",
        ),
        (
            "corrupt-full-object",
            "UPDATE layerfs_objects SET canonical_bytes = zeroblob(canonical_length)
             WHERE rowid = (SELECT min(rowid) FROM layerfs_objects)",
        ),
    ] {
        let corrupted = copy_and_mutate(&baseline.candidate, label, sql);
        drop(FullStorage::open_durable(&corrupted).unwrap());
        assert!(FullStorage::open_durable_verified(&corrupted).is_err());
        remove(&corrupted);
    }
    cleanup(baseline);
}

#[test]
fn legacy_semantic_corruption_is_rejected_before_candidate_creation() {
    let baseline = rich_baseline("legacy-integrity");
    for (label, sql) in [
        (
            "corrupt-legacy-branch-head",
            "UPDATE layerfs_branches SET generation = generation + 1",
        ),
        (
            "corrupt-legacy-stack-head",
            "UPDATE layerfs_layer_stacks SET generation = generation + 1",
        ),
    ] {
        let corrupted = copy_and_mutate(&baseline.source, label, sql);
        let candidate = path(&format!("{label}-candidate"));
        assert!(migrate_legacy_durable_file(&corrupted, &candidate, baseline.storage_id).is_err());
        assert!(!candidate.exists());
        remove(&corrupted);
    }
    cleanup(baseline);
}
