use layerfs_core::content::rope::{build, ObjectStore};
use layerfs_core::inode::{inode_table_from_root, InodeKind, InodeRecordV1};
use layerfs_core::metadata::{build_metadata_tree, MetadataEntryV1, MetadataKey};
use layerfs_core::namespace::{empty_directory, NamespaceRootV1};
use layerfs_core::namespace_codec::{
    encode_inode_record, encode_namespace_root, profile_id as namespace_profile_id,
};
use layerfs_core::{CanonicalPath, ObjectId};
use layerfs_durable_store::DurableStore;
use layerfs_storage::{derive_id, LayerCandidate, LayerStackMergeOutcome, RequestId};
use layerfs_sync::LocalDurable;
use layerfs_sync::ResumeToken;
use layerfs_sync::{fetch_branch, push_branch, push_branch_rollback, push_layer_stack_genesis};
use layerfs_working_store::{
    BranchId, BranchRollbackResult, CommitResult, IntegrityMode, LayerId, LayerStackId,
    WorkingCandidate, WorkingStore,
};
use std::fs;
use std::io::Cursor;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn branch_history_over_one_thousand_records_pushes_and_fetches_in_bounded_pages() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-history-pages-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    let working = WorkingStore::open(&base.join("working-a"), IntegrityMode::Verified).unwrap();
    let root = empty_root(&working);
    let stack = working
        .create_layer_stack(
            LayerStackId::from_bytes([0x31; 32]),
            LayerId::from_bytes([0x32; 32]),
            "main",
            root,
        )
        .unwrap();
    let mut head = working
        .create_top_level_branch(BranchId::from_bytes([0x33; 32]), Some("history"), stack)
        .unwrap();
    let mut last_record = None;
    for _ in 0..1_025 {
        let begin = working.begin_operation(head).unwrap();
        let outcome = working
            .operation_commit(
                begin,
                WorkingCandidate {
                    operation_id: begin.operation_id,
                    expected_branch_generation: head.generation,
                    base_root: root,
                    candidate_root: root,
                    normalized_transition: Vec::new(),
                },
            )
            .unwrap();
        match outcome {
            CommitResult::WorkingRecorded {
                head: accepted,
                record,
                ..
            } => {
                working.acknowledge_operation(record).unwrap();
                head = accepted;
                last_record = Some(record);
            }
            CommitResult::Conflict { .. } => panic!("sequential history conflicted"),
        }
    }

    let durable = DurableStore::open(&base.join("durable")).unwrap();
    let endpoint = LocalDurable::new(&durable);
    push_layer_stack_genesis(
        &working,
        &endpoint,
        [0x34; 32],
        head.branch_id,
        stack,
        "main",
        ResumeToken::default(),
    )
    .unwrap();
    durable.reset_counters().unwrap();
    let pushed = push_branch(
        &working,
        &endpoint,
        [0x35; 32],
        head.branch_id,
        None,
        ResumeToken::default(),
    )
    .unwrap();
    assert!(pushed.complete);
    assert_eq!(pushed.pages, 17);
    assert_eq!(durable.counters().unwrap().durable_head_transactions, 1);
    assert_eq!(durable.branch_head(head.branch_id).unwrap(), Some(head));
    let mut durable_stack = stack;
    for sequence in 0_u64..65 {
        let request_id = RequestId::from_bytes(derive_id(
            b"history-paged-layer",
            &[&sequence.to_be_bytes()],
        ));
        let layer_id = LayerId::from_bytes(derive_id(
            b"candidate-layer",
            &[
                stack.layer_stack_id.as_bytes(),
                request_id.as_bytes(),
                root.as_bytes(),
            ],
        ));
        let candidate = LayerCandidate {
            layer_stack_id: stack.layer_stack_id,
            layer_id,
            parent_layer_id: durable_stack.layer_id,
            source: head,
            source_depth: 0,
            root,
            request_id,
        };
        durable_stack = match durable
            .accept_layer_stack_merge(candidate, durable_stack)
            .unwrap()
        {
            LayerStackMergeOutcome::DurablyAccepted { head, .. } => head,
            LayerStackMergeOutcome::Conflict { .. } => panic!("LayerStack paging conflicted"),
        };
    }

    let child = working
        .create_child_branch(
            BranchId::from_bytes([0x37; 32]),
            Some("nested"),
            last_record.unwrap(),
        )
        .unwrap();
    let child_begin = working.begin_operation(child).unwrap();
    let child_head = match working
        .operation_commit(
            child_begin,
            WorkingCandidate {
                operation_id: child_begin.operation_id,
                expected_branch_generation: child.generation,
                base_root: root,
                candidate_root: root,
                normalized_transition: Vec::new(),
            },
        )
        .unwrap()
    {
        CommitResult::WorkingRecorded {
            head: accepted,
            record,
            ..
        } => {
            working.acknowledge_operation(record).unwrap();
            accepted
        }
        CommitResult::Conflict { .. } => panic!("nested history conflicted"),
    };
    let child_push = push_branch(
        &working,
        &endpoint,
        [0x38; 32],
        child.branch_id,
        None,
        ResumeToken::default(),
    )
    .unwrap();
    assert!(child_push.complete);
    assert_eq!(
        durable.branch_head(child.branch_id).unwrap(),
        Some(child_head)
    );

    let fresh = WorkingStore::open(&base.join("working-b"), IntegrityMode::Verified).unwrap();
    let fetched = fetch_branch(
        &endpoint,
        &fresh,
        [0x36; 32],
        head.branch_id,
        ResumeToken::default(),
    )
    .unwrap();
    assert!(fetched.complete);
    assert_eq!(fetched.pages, 18);
    assert_eq!(fetched.head, head);
    assert_eq!(fresh.branch_head(head.branch_id).unwrap(), Some(head));
    assert_eq!(
        fresh.layer_stack_head(stack.layer_stack_id).unwrap(),
        Some(durable_stack)
    );

    let nested_fresh =
        WorkingStore::open(&base.join("working-child-only"), IntegrityMode::Verified).unwrap();
    let nested = fetch_branch(
        &endpoint,
        &nested_fresh,
        [0x39; 32],
        child.branch_id,
        ResumeToken::default(),
    )
    .unwrap();
    assert_eq!(nested.head, child_head);
    assert!(nested.dependency_pages >= 17);
    assert!(nested.dependency_transfer.is_some());
    assert_eq!(
        nested_fresh.branch_head(head.branch_id).unwrap(),
        Some(head)
    );

    drop(fresh);
    drop(nested_fresh);
    drop(durable);
    drop(working);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn compacted_hard_rollback_history_fetches_without_released_page_heads() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-rollback-pages-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    let working = WorkingStore::open(&base.join("working-a"), IntegrityMode::Verified).unwrap();
    let root = empty_root(&working);
    let stack = working
        .create_layer_stack(
            LayerStackId::from_bytes([0xd1; 32]),
            LayerId::from_bytes([0xd2; 32]),
            "rollback-pages",
            root,
        )
        .unwrap();
    let mut head = working
        .create_top_level_branch(
            BranchId::from_bytes([0xd3; 32]),
            Some("rollback-pages"),
            stack,
        )
        .unwrap();
    let begin = working.begin_operation(head).unwrap();
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
            root,
            &CanonicalPath::new("file").unwrap(),
            Cursor::new(vec![0_u8; 4096]),
            (inode, metadata),
        )
        .unwrap();
    let candidate_root = candidate.root();
    writer
        .commit_trusted_operation_candidate(begin.operation_id, candidate)
        .unwrap();
    let (target, target_version) = match working
        .operation_commit(
            begin,
            WorkingCandidate {
                operation_id: begin.operation_id,
                expected_branch_generation: head.generation,
                base_root: root,
                candidate_root,
                normalized_transition: Vec::new(),
            },
        )
        .unwrap()
    {
        CommitResult::WorkingRecorded { head, record, .. } => {
            working.acknowledge_operation(record).unwrap();
            (head, record.operation_version_id)
        }
        CommitResult::Conflict { .. } => panic!("seed conflict"),
    };
    head = target;
    for value in 1_u8..=64 {
        let begin = working.begin_operation(head).unwrap();
        let mut writer = working.begin_candidate_write().unwrap();
        let candidate = writer
            .trusted_replace_range(
                head.root,
                &CanonicalPath::new("file").unwrap(),
                0,
                1,
                Cursor::new([value]),
            )
            .unwrap();
        let candidate_root = candidate.root();
        writer
            .commit_trusted_operation_candidate(begin.operation_id, candidate)
            .unwrap();
        head = match working
            .operation_commit(
                begin,
                WorkingCandidate {
                    operation_id: begin.operation_id,
                    expected_branch_generation: head.generation,
                    base_root: head.root,
                    candidate_root,
                    normalized_transition: Vec::new(),
                },
            )
            .unwrap()
        {
            CommitResult::WorkingRecorded { head, record, .. } => {
                working.acknowledge_operation(record).unwrap();
                head
            }
            CommitResult::Conflict { .. } => panic!("sequential conflict"),
        };
    }
    let durable = DurableStore::open(&base.join("durable")).unwrap();
    let endpoint = LocalDurable::new(&durable);
    push_layer_stack_genesis(
        &working,
        &endpoint,
        [0xd4; 32],
        head.branch_id,
        stack,
        "rollback-pages",
        ResumeToken::default(),
    )
    .unwrap();
    push_branch(
        &working,
        &endpoint,
        [0xd5; 32],
        head.branch_id,
        None,
        ResumeToken::default(),
    )
    .unwrap();
    let (rolled_back, publication) = match working.branch_rollback(head, target_version).unwrap() {
        BranchRollbackResult::WorkingRecorded {
            head, publication, ..
        } => (head, publication),
        other => panic!("rollback failed: {other:?}"),
    };
    push_branch_rollback(&endpoint, publication).unwrap();
    let durable = durable.compact().unwrap();
    let endpoint = LocalDurable::new(&durable);
    let fresh_path = base.join("working-b");
    let mut fresh = WorkingStore::open(&fresh_path, IntegrityMode::Verified).unwrap();
    fresh.inject_fetch_boundary_failure_for_test();
    let interrupted = fetch_branch(
        &endpoint,
        &fresh,
        [0xd6; 32],
        head.branch_id,
        ResumeToken::default(),
    )
    .unwrap_err();
    assert_eq!(fresh.branch_head(head.branch_id).unwrap(), None);
    assert_eq!(
        fresh
            .fetch_resume_branch_head(head.branch_id)
            .unwrap()
            .unwrap_or_else(|| panic!("no staged head after {interrupted:?}"))
            .generation,
        65
    );
    drop(fresh);
    let fresh = WorkingStore::open(&fresh_path, IntegrityMode::Verified).unwrap();
    let fetched = fetch_branch(
        &endpoint,
        &fresh,
        [0xd6; 32],
        head.branch_id,
        ResumeToken::default(),
    )
    .unwrap();
    assert_eq!(fetched.head, rolled_back);
    assert_eq!(
        fresh.branch_head(head.branch_id).unwrap(),
        Some(rolled_back)
    );
    drop(fresh);
    drop(durable);
    drop(working);
    fs::remove_dir_all(base).unwrap();
}

fn empty_root(working: &WorkingStore) -> ObjectId {
    let mut writer = working.begin_candidate_write().unwrap();
    let root_inode = writer.allocate_inode_id().unwrap();
    let (mode, _) = build(&mut writer, 0o755_u32.to_be_bytes().as_slice()).unwrap();
    let mut mtime = Vec::new();
    mtime.extend_from_slice(&0_i64.to_be_bytes());
    mtime.extend_from_slice(&0_u32.to_be_bytes());
    let (mtime, _) = build(&mut writer, mtime.as_slice()).unwrap();
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
    let directory = empty_directory(&mut writer).unwrap();
    let record = writer
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
    let table = inode_table_from_root(&mut writer, root_inode, record).unwrap();
    let root = writer
        .put(
            &encode_namespace_root(NamespaceRootV1 {
                profile_id: namespace_profile_id(),
                root_directory_inode: root_inode,
                inode_table_root: table.0,
            })
            .unwrap(),
        )
        .unwrap();
    writer.commit_candidate(root).unwrap()
}
