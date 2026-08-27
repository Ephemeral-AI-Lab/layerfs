use layerfs_core::content::rope::build;
use layerfs_core::inode::{inode_table_from_root, InodeKind, InodeRecordV1};
use layerfs_core::metadata::{build_metadata_tree, MetadataEntryV1, MetadataKey};
use layerfs_core::namespace::{empty_directory, NamespaceRootV1};
use layerfs_core::namespace_codec::{encode_inode_record, encode_namespace_root, profile_id};
use layerfs_core::object::access::ObjectStore;
use layerfs_storage::integrity::IntegrityMode;
use layerfs_storage::product::{BranchId, LayerId, LayerStackId};
use layerfs_working_store::{
    BeginOperation, BranchRollbackResult, ChildMergeResult, CommitResult, LayerPreparationResult,
    WorkingCandidate, WorkingError, WorkingStore,
};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn concurrent_exact_head_operations_record_one_winner_and_preserve_the_conflict() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-working-lifecycle-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let mut working = WorkingStore::open(&root, IntegrityMode::TrustedLocalDev).unwrap();
    let storage_id = working.storage_id();
    let canonical_root = valid_empty_root(&mut working);
    let stack = working
        .create_layer_stack(
            LayerStackId::from_bytes([0x21; 32]),
            LayerId::from_bytes([0x22; 32]),
            "main",
            canonical_root,
        )
        .unwrap();
    let branch = working
        .create_top_level_branch(BranchId::from_bytes([0x23; 32]), Some("work"), stack)
        .unwrap();

    let first_begin = working.begin_operation(branch).unwrap();
    let second_begin = working.begin_operation(branch).unwrap();
    assert_ne!(first_begin.operation_id, second_begin.operation_id);
    let first = candidate(&first_begin, canonical_root);
    let second = candidate(&second_begin, canonical_root);

    let (accepted, first_record) = match working.operation_commit(first_begin, first).unwrap() {
        CommitResult::WorkingRecorded { head, record, .. } => (head, record),
        CommitResult::Conflict { .. } => panic!("first operation conflicted"),
    };
    assert_eq!(accepted.generation, 1);
    assert_eq!(accepted.root, canonical_root);
    match working.operation_commit(second_begin, second).unwrap() {
        CommitResult::Conflict { actual, .. } => assert_eq!(actual, accepted),
        CommitResult::WorkingRecorded { .. } => panic!("stale operation was accepted"),
    }
    assert_eq!(
        working.branch_head(branch.branch_id).unwrap(),
        Some(accepted)
    );

    let child_b = working
        .create_child_branch(BranchId::from_bytes([0x24; 32]), Some("B"), first_record)
        .unwrap();
    let (child_b1, child_b_record) = commit_no_change(&working, child_b);
    let child_c = working
        .create_child_branch(BranchId::from_bytes([0x25; 32]), Some("C"), child_b_record)
        .unwrap();
    let (child_c1, _) = commit_no_change(&working, child_c);
    assert!(matches!(
        working.child_branch_merge(child_c1, accepted),
        Err(WorkingError::InvalidReceipt)
    ));
    let child_b2 = match working.child_branch_merge(child_c1, child_b1).unwrap() {
        ChildMergeResult::WorkingRecorded { parent_head, .. } => parent_head,
        other => panic!("C to B merge failed: {other:?}"),
    };
    assert_eq!(
        working.branch_head(child_c.branch_id).unwrap(),
        Some(child_c1)
    );
    let accepted2 = match working.child_branch_merge(child_b2, accepted).unwrap() {
        ChildMergeResult::WorkingRecorded { parent_head, .. } => parent_head,
        other => panic!("B to A merge failed: {other:?}"),
    };
    assert_eq!(
        working.branch_head(child_b.branch_id).unwrap(),
        Some(child_b2)
    );
    assert_eq!(accepted2.generation, accepted.generation + 1);
    let first_candidate = match working.prepare_layer_stack_merge(child_c1, stack).unwrap() {
        LayerPreparationResult::Prepared(candidate) => candidate,
        other => panic!("depth-2 Layer candidate failed: {other:?}"),
    };
    let second_candidate = match working.prepare_layer_stack_merge(child_c1, stack).unwrap() {
        LayerPreparationResult::Prepared(candidate) => candidate,
        other => panic!("repeated Layer candidate failed: {other:?}"),
    };
    assert_eq!(first_candidate.source_depth, 2);
    assert_eq!(first_candidate.parent_layer_id, stack.layer_id);
    assert_ne!(first_candidate.layer_id, second_candidate.layer_id);
    let candidates = working.recoverable_layer_candidates_after(None, 8).unwrap();
    let mut expected_candidates = vec![first_candidate, second_candidate];
    expected_candidates.sort_unstable_by_key(|candidate| candidate.layer_id);
    assert_eq!(candidates, expected_candidates);
    assert!(!working
        .drop_layer_candidate(first_candidate.layer_id)
        .unwrap());
    assert!(working
        .drop_layer_candidate(first_candidate.layer_id)
        .unwrap());
    assert_eq!(
        working.recoverable_layer_candidates_after(None, 8).unwrap(),
        vec![second_candidate]
    );
    assert!(!working
        .drop_layer_candidate(second_candidate.layer_id)
        .unwrap());
    assert!(working
        .recoverable_layer_candidates_after(None, 8)
        .unwrap()
        .is_empty());
    assert_eq!(
        working.layer_stack_head(stack.layer_stack_id).unwrap(),
        Some(stack)
    );
    assert_eq!(
        working.branch_head(child_c.branch_id).unwrap(),
        Some(child_c1)
    );
    let (accepted3, accepted3_record) = commit_no_change(&working, accepted2);
    let rollback_blocker = working
        .create_child_branch(
            BranchId::from_bytes([0x26; 32]),
            Some("rollback-blocker"),
            accepted3_record,
        )
        .unwrap();
    let (accepted4, _) = commit_no_change(&working, accepted3);
    assert_eq!(
        working
            .branch_rollback(accepted4, accepted2.operation_version_id.unwrap())
            .unwrap(),
        BranchRollbackResult::Blocked
    );
    working.drop_branch(rollback_blocker.branch_id).unwrap();
    let rolled_back = match working
        .branch_rollback(accepted4, accepted2.operation_version_id.unwrap())
        .unwrap()
    {
        BranchRollbackResult::WorkingRecorded { head, .. } => head,
        other => panic!("Branch rollback failed: {other:?}"),
    };
    assert_eq!(rolled_back.root, accepted2.root);
    assert_eq!(rolled_back.generation, accepted4.generation + 1);
    let unreachable = {
        let mut publication = working.begin_candidate_write().unwrap();
        let unreachable = publication
            .put(&layerfs_core::encode_bytes_object(b"unreachable compaction proof").unwrap())
            .unwrap();
        publication.commit_objects().unwrap();
        unreachable
    };
    assert!(working.sync_has_object(unreachable).unwrap());
    drop(working);

    let reopened = WorkingStore::open(&root, IntegrityMode::TrustedLocalDev).unwrap();
    assert_eq!(reopened.storage_id(), storage_id);
    assert_eq!(
        reopened.branch_head(branch.branch_id).unwrap(),
        Some(rolled_back)
    );
    assert!(root.join("working.sqlite.generations/CURRENT").is_file());
    assert!(reopened
        .database_path()
        .ends_with("generation-0000000000000000.sqlite"));
    let reopened = reopened.compact().unwrap();
    let observation = reopened.last_compaction_observation().unwrap();
    assert_eq!(
        observation.source_indexed_objects,
        observation.retained_objects + observation.reclaimed_objects
    );
    assert_eq!(
        observation.source_indexed_canonical_bytes,
        observation.retained_canonical_bytes + observation.reclaimed_canonical_bytes
    );
    assert_eq!(
        observation.candidate_indexed_objects,
        observation.retained_objects
    );
    assert_eq!(
        observation.candidate_indexed_canonical_bytes,
        observation.retained_canonical_bytes
    );
    assert!(observation.reclaimed_objects > 0);
    assert!(observation.reclaimed_canonical_bytes > 0);
    assert_eq!(reopened.storage_id(), storage_id);
    assert_eq!(
        reopened.branch_head(branch.branch_id).unwrap(),
        Some(rolled_back)
    );
    assert!(reopened
        .database_path()
        .ends_with("generation-0000000000000001.sqlite"));
    assert!(!root
        .join("working.sqlite.generations/generation-0000000000000000.sqlite")
        .exists());
    assert!(!reopened.sync_has_object(unreachable).unwrap());
    assert_eq!(
        fs::metadata(root.join("working.sqlite.generations/CURRENT"))
            .unwrap()
            .len(),
        layerfs_storage::generation::SELECTOR_BYTES as u64
    );
    drop(reopened);
    fs::remove_dir_all(root).unwrap();
}

fn commit_no_change(
    working: &WorkingStore,
    head: layerfs_storage::product::BranchHead,
) -> (
    layerfs_storage::product::BranchHead,
    layerfs_storage::product::OperationRecordRef,
) {
    let begin = working.begin_operation(head).unwrap();
    let candidate = candidate(&begin, head.root);
    match working.operation_commit(begin, candidate).unwrap() {
        CommitResult::WorkingRecorded { head, record, .. } => (head, record),
        CommitResult::Conflict { .. } => panic!("no-change operation conflicted"),
    }
}

fn candidate(begin: &BeginOperation, candidate_root: layerfs_core::ObjectId) -> WorkingCandidate {
    WorkingCandidate {
        operation_id: begin.operation_id,
        expected_branch_generation: begin.branch_head_before.generation,
        base_root: begin.base.root(),
        candidate_root,
        normalized_transition: Vec::new(),
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
