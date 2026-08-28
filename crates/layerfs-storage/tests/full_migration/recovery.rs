use super::fixture::{path, publish_valid_root, push_branch_to_authority, remove, transfer_all};
use layerfs_storage::{
    derive_id, migration::migrate_legacy_durable_file, BranchId, BranchPushOutcome, Engine,
    LayerCandidateRequest, LayerId, LayerStackId, LayerStackMergeOutcome,
    LayerStackRollbackOutcome, LeaseId, OperationCandidate, OperationCommitOutcome, OperationId,
    RequestId, VerifiedFetchRequest,
};
use rusqlite::{params, Connection};

#[test]
fn shared_request_releases_fold_to_distinct_typed_full_rows() {
    let working_path = path("release-working");
    let source = path("release-source");
    let candidate_path = path("release-candidate");
    let working = Engine::open(&working_path).unwrap();
    let (root0, _) = publish_valid_root(&working, "release-root-0", [0x81; 32]);
    let (root1, _) = publish_valid_root(&working, "release-root-1", [0x82; 32]);
    let (root2, _) = publish_valid_root(&working, "release-root-2", [0x83; 32]);
    let stack_id = LayerStackId::from_bytes([0x84; 32]);
    let stack = working
        .product_create_layer_stack(stack_id, LayerId::from_bytes([0x85; 32]), "release", root0)
        .unwrap();
    let branch = working
        .product_create_top_level_branch(BranchId::from_bytes([0x86; 32]), None, stack)
        .unwrap();
    let operation = OperationId::from_bytes([0x87; 32]);
    working
        .product_begin_operation(operation, branch, LeaseId::from_bytes([0x88; 32]))
        .unwrap();
    let working_head = match working
        .product_operation_commit(OperationCandidate {
            operation_id: operation,
            expected: branch,
            candidate_root: root1,
            normalized_transition: Vec::new(),
            request_id: RequestId::from_bytes([0x89; 32]),
        })
        .unwrap()
    {
        OperationCommitOutcome::WorkingRecorded { head, .. } => head,
        OperationCommitOutcome::Conflict { .. } => panic!("unexpected Branch conflict"),
    };
    let engine = Engine::open(&source).unwrap();
    let storage_id = engine.store_id().unwrap();
    let source_head = push_branch_to_authority(&working, &engine, stack, branch.branch_id, 0x8f);
    assert_eq!(source_head, working_head);
    let first = engine
        .product_prepare_layer_candidate(LayerCandidateRequest {
            source: source_head,
            expected_stack: stack,
            result_root: root1,
            source_transition: Vec::new(),
            applied_transition: Vec::new(),
            request_id: RequestId::from_bytes([0x8a; 32]),
        })
        .unwrap();
    let stack1 = match engine
        .product_accept_layer_stack_merge(first, stack, RequestId::from_bytes([0x8b; 32]))
        .unwrap()
    {
        LayerStackMergeOutcome::DurablyAccepted { head, .. } => head,
        LayerStackMergeOutcome::Conflict { .. } => panic!("unexpected LayerStack conflict"),
    };
    let second = engine
        .product_prepare_layer_candidate(LayerCandidateRequest {
            source: source_head,
            expected_stack: stack1,
            result_root: root2,
            source_transition: Vec::new(),
            applied_transition: Vec::new(),
            request_id: RequestId::from_bytes([0x8c; 32]),
        })
        .unwrap();
    let stack2 = match engine
        .product_accept_layer_stack_merge(second, stack1, RequestId::from_bytes([0x8d; 32]))
        .unwrap()
    {
        LayerStackMergeOutcome::DurablyAccepted { head, .. } => head,
        LayerStackMergeOutcome::Conflict { .. } => panic!("unexpected LayerStack conflict"),
    };
    let rollback_request = RequestId::from_bytes([0x8e; 32]);
    assert!(matches!(
        engine
            .product_layer_stack_rollback(stack2, stack.layer_id, rollback_request)
            .unwrap(),
        LayerStackRollbackOutcome::DurablyAccepted { .. }
    ));
    drop(working);
    drop(engine);

    let connection = Connection::open(&source).unwrap();
    connection.execute("DELETE FROM layerfs_refs", []).unwrap();
    let legacy = connection
        .prepare(
            "SELECT version_id, root_id, release_generation, request_id
             FROM layerfs_released_versions WHERE target_kind = 'layer'
             ORDER BY version_id",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Vec<u8>>(3)?,
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(legacy.len(), 2);
    assert!(legacy
        .iter()
        .all(|row| row.2 == 3 && row.3.as_slice() == rollback_request.as_bytes()));
    drop(connection);
    let source_before = std::fs::read(&source).unwrap();

    drop(migrate_legacy_durable_file(&source, &candidate_path, storage_id).unwrap());
    assert_eq!(std::fs::read(&source).unwrap(), source_before);
    let candidate = Connection::open(&candidate_path).unwrap();
    let full = candidate
        .prepare(
            "SELECT release_id, layer_stack_id, layer_id, branch_id,
                    operation_version_id, root_id, release_generation, request_id
             FROM layerfs_released_versions ORDER BY layer_id",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get::<_, Vec<u8>>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, Option<Vec<u8>>>(3)?,
                row.get::<_, Option<Vec<u8>>>(4)?,
                row.get::<_, Vec<u8>>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, Vec<u8>>(7)?,
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(full.len(), 2);
    assert_ne!(full[0].0, full[1].0);
    for (target, source) in full.iter().zip(legacy.iter()) {
        assert_eq!(
            target.0,
            derive_id(
                b"layerfs.full.release.v1",
                &[b"layer", stack_id.as_bytes(), source.0.as_slice()]
            )
        );
        assert_eq!(target.1.as_slice(), stack_id.as_bytes());
        assert_eq!(target.2, source.0);
        assert_eq!((target.3.as_ref(), target.4.as_ref()), (None, None));
        assert_eq!(target.5, source.1);
        assert_eq!(
            (target.6, target.7.as_slice()),
            (source.2, source.3.as_slice())
        );
    }
    drop(candidate);

    remove(&working_path);
    remove(&source);
    remove(&candidate_path);
}

#[test]
fn verified_tracking_rebuilds_exact_membership_without_legacy_closure_ids() {
    let upstream_path = path("tracking-upstream");
    let source = path("tracking-source");
    let candidate_path = path("tracking-candidate");
    let upstream = Engine::open(&upstream_path).unwrap();
    let (root, _) = publish_valid_root(&upstream, "tracking", [0x91; 32]);
    let stack = upstream
        .product_create_layer_stack(
            LayerStackId::from_bytes([0x92; 32]),
            LayerId::from_bytes([0x93; 32]),
            "tracking",
            root,
        )
        .unwrap();
    let branch = upstream
        .product_create_top_level_branch(BranchId::from_bytes([0x94; 32]), None, stack)
        .unwrap();
    let bundle = upstream
        .product_export_branch_fetch(branch.branch_id)
        .unwrap();

    let destination = Engine::open(&source).unwrap();
    let storage_id = destination.store_id().unwrap();
    let request = RequestId::from_bytes([0x95; 32]);
    let (objects, counters) = transfer_all(
        &upstream,
        &destination,
        request,
        RequestId::from_bytes([0x96; 32]),
        "fetch",
    );
    assert_eq!(
        destination
            .product_import_verified_branch_fetch(
                None,
                &bundle,
                VerifiedFetchRequest {
                    request_id: request,
                    durable_storage_id: storage_id,
                    counters,
                },
            )
            .unwrap(),
        BranchPushOutcome::DurablyAccepted {
            head: branch,
            reconciled: false,
        }
    );
    drop(upstream);
    drop(destination);

    let legacy = Connection::open(&source).unwrap();
    assert_eq!(
        legacy
            .query_row(
                "SELECT count(*) FROM layerfs_fetch_closure_items",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        legacy
            .query_row(
                "SELECT count(*) FROM layerfs_durable_tracking_refs
                 WHERE status = 'verified_complete' AND verification_receipt_id = ?1",
                params![request.as_bytes().as_slice()],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        2
    );
    drop(legacy);
    let source_before = std::fs::read(&source).unwrap();

    drop(migrate_legacy_durable_file(&source, &candidate_path, storage_id).unwrap());
    assert_eq!(std::fs::read(&source).unwrap(), source_before);
    let candidate = Connection::open(&candidate_path).unwrap();
    let expected_membership = i64::try_from(objects.len() * 2).unwrap();
    assert_eq!(
        candidate
            .query_row(
                "SELECT count(*) FROM layerfs_fetch_closure_items",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        expected_membership
    );
    assert!(candidate
        .query_row(
            "SELECT NOT EXISTS(
                 SELECT r.tracking_ref_id, o.object_id
                 FROM layerfs_durable_tracking_refs AS r CROSS JOIN layerfs_objects AS o
                 EXCEPT SELECT tracking_ref_id, object_id FROM layerfs_fetch_closure_items)
             AND NOT EXISTS(
                 SELECT tracking_ref_id, object_id FROM layerfs_fetch_closure_items
                 EXCEPT SELECT r.tracking_ref_id, o.object_id
                 FROM layerfs_durable_tracking_refs AS r CROSS JOIN layerfs_objects AS o)",
            [],
            |row| row.get::<_, bool>(0),
        )
        .unwrap());
    assert_eq!(
        candidate
            .query_row(
                "SELECT count(*) FROM layerfs_sync_receipts
                 WHERE request_id = ?1 AND result = 'fetched'",
                params![request.as_bytes().as_slice()],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );
    drop(candidate);

    remove(&upstream_path);
    remove(&source);
    remove(&candidate_path);
}
