use super::fixture::{path, publish_valid_root, remove};
use layerfs_storage::{
    branch_push_bundle_page_digest, migration::migrate_legacy_durable_file, BranchId,
    BranchPushIdentityBuilder, BranchPushOutcome, BranchPushRequest, Engine, LayerCandidateRequest,
    LayerId, LayerStackId, LayerStackMergeOutcome, LeaseId, OperationCandidate,
    OperationCommitOutcome, OperationId, RequestId, SyncTransferCounters,
};
use rusqlite::{params, Connection};

#[test]
fn accepted_history_and_folded_rows_migrate_exactly() {
    let working_path = path("history-working");
    let source = path("history-durable");
    let candidate_path = path("history-candidate");
    let working = Engine::open(&working_path).unwrap();
    let (base_root, _) = publish_valid_root(&working, "history-base", [0x61; 32]);
    let (result_root, _) = publish_valid_root(&working, "history-result", [0x62; 32]);
    let stack_id = LayerStackId::from_bytes([0x63; 32]);
    let layer_id = LayerId::from_bytes([0x64; 32]);
    let branch_id = BranchId::from_bytes([0x65; 32]);
    let operation_id = OperationId::from_bytes([0x66; 32]);

    let stack = working
        .product_create_layer_stack(stack_id, layer_id, "history", base_root)
        .unwrap();
    let branch = working
        .product_create_top_level_branch(branch_id, Some("history"), stack)
        .unwrap();
    working
        .product_begin_operation(operation_id, branch, LeaseId::from_bytes([0x67; 32]))
        .unwrap();
    let accepted_branch = match working
        .product_operation_commit(OperationCandidate {
            operation_id,
            expected: branch,
            candidate_root: result_root,
            normalized_transition: Vec::new(),
            request_id: RequestId::from_bytes([0x68; 32]),
        })
        .unwrap()
    {
        OperationCommitOutcome::WorkingRecorded { head, .. } => head,
        OperationCommitOutcome::Conflict { .. } => panic!("unexpected Working conflict"),
    };

    let durable = Engine::open(&source).unwrap();
    let storage_id = durable.store_id().unwrap();
    let object_ids = working.object_ids_page(None, 1024).unwrap();
    let objects = object_ids
        .iter()
        .map(|id| {
            (
                *id,
                working
                    .load_canonical_authenticated_bounded(*id, 1024 * 1024)
                    .unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let counters = SyncTransferCounters {
        unique_bytes: objects
            .iter()
            .map(|(_, canonical)| canonical.len() as u64)
            .sum(),
        ..SyncTransferCounters::default()
    };
    let transfer_id = RequestId::from_bytes([0x69; 32]);
    let data_request_id = RequestId::from_bytes([0x6a; 32]);
    durable
        .accept_canonical_batch_pinned(transfer_id, data_request_id, "push", objects.as_slice())
        .unwrap();
    durable
        .product_create_layer_stack(stack_id, layer_id, "history", base_root)
        .unwrap();
    let bundle = working.product_export_branch_push(branch_id, None).unwrap();
    durable
        .product_stage_branch_push_page(transfer_id, 0, data_request_id, &bundle, counters)
        .unwrap();
    let page_digest =
        branch_push_bundle_page_digest(transfer_id, 0, data_request_id, &bundle, counters).unwrap();
    let mut identity = BranchPushIdentityBuilder::new(transfer_id);
    identity.absorb_page(0, page_digest).unwrap();
    let request = BranchPushRequest {
        request_id: transfer_id,
        transfer_id,
        candidate_digest: identity.finish(bundle.head),
        expected: None,
        counters,
    };
    assert_eq!(
        durable
            .product_commit_staged_branch_push(request, branch_id)
            .unwrap(),
        BranchPushOutcome::DurablyAccepted {
            head: accepted_branch,
            reconciled: false,
        }
    );
    let layer_candidate = durable
        .product_prepare_layer_candidate(LayerCandidateRequest {
            source: accepted_branch,
            expected_stack: stack,
            result_root,
            source_transition: Vec::new(),
            applied_transition: Vec::new(),
            request_id: RequestId::from_bytes([0x6b; 32]),
        })
        .unwrap();
    let accepted_stack = match durable
        .product_accept_layer_stack_merge(layer_candidate, stack, RequestId::from_bytes([0x6c; 32]))
        .unwrap()
    {
        LayerStackMergeOutcome::DurablyAccepted { head, .. } => head,
        LayerStackMergeOutcome::Conflict { .. } => panic!("unexpected LayerStack conflict"),
    };
    drop(working);
    drop(durable);
    let source_before = std::fs::read(&source).unwrap();

    let candidate = migrate_legacy_durable_file(&source, &candidate_path, storage_id).unwrap();
    drop(candidate);
    assert_eq!(std::fs::read(&source).unwrap(), source_before);

    let candidate = Connection::open(&candidate_path).unwrap();
    assert_eq!(
        candidate
            .query_row(
                "SELECT generation, head_layer_id FROM layerfs_layer_stacks
                 WHERE layer_stack_id = ?1",
                params![stack_id.as_bytes().as_slice()],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .unwrap(),
        (1, accepted_stack.layer_id.as_bytes().to_vec())
    );
    assert_eq!(
        candidate
            .query_row(
                "SELECT b.generation, b.head_operation_version_id, o.state
                 FROM layerfs_branches AS b JOIN layerfs_operations AS o
                   ON o.result_operation_version_id = b.head_operation_version_id
                 WHERE b.branch_id = ?1",
                params![branch_id.as_bytes().as_slice()],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .unwrap(),
        (
            1,
            accepted_branch
                .operation_version_id
                .unwrap()
                .as_bytes()
                .to_vec(),
            "durably_accepted".to_owned()
        )
    );
    candidate
        .execute(
            "ATTACH DATABASE ?1 AS legacy",
            params![source.to_str().unwrap()],
        )
        .unwrap();
    for table in [
        "layerfs_branch_transitions",
        "layerfs_layer_stack_transitions",
    ] {
        let sql = format!(
            "SELECT NOT EXISTS(SELECT * FROM legacy.{table} EXCEPT SELECT * FROM main.{table})
                AND NOT EXISTS(SELECT * FROM main.{table} EXCEPT SELECT * FROM legacy.{table})"
        );
        assert!(candidate
            .query_row(&sql, [], |row| row.get::<_, bool>(0))
            .unwrap());
    }
    assert!(candidate
        .query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM main.layerfs_operation_versions AS f
                 JOIN legacy.layerfs_operation_versions AS v
                   ON v.operation_version_id = f.operation_version_id
                 JOIN legacy.layerfs_operation_deltas AS d
                   ON d.operation_version_id = v.operation_version_id
                  AND d.operation_id = v.created_by_operation_id
                 WHERE f.operation_version_id = ?1
                   AND f.branch_id = v.branch_id AND f.sequence = v.sequence
                   AND f.parent_operation_version_id IS v.parent_operation_version_id
                   AND f.created_by_kind = v.created_by_kind
                   AND f.operation_id IS v.created_by_operation_id
                   AND f.child_branch_id IS v.created_by_child_branch_id
                   AND f.branch_delta_id IS v.created_by_branch_delta_id
                   AND f.transition_delta_id = d.transition_delta_id
                   AND f.base_root_id = d.base_root
                   AND f.result_root_id = d.result_root)",
            params![accepted_branch
                .operation_version_id
                .unwrap()
                .as_bytes()
                .as_slice()],
            |row| row.get::<_, bool>(0),
        )
        .unwrap());
    assert!(candidate
        .query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM main.layerfs_layers AS f
                 JOIN legacy.layerfs_layers AS l ON l.layer_id = f.layer_id
                 JOIN legacy.layerfs_layer_deltas AS d ON d.candidate_layer_id = l.layer_id
                 WHERE f.layer_id = ?1 AND f.layer_stack_id = l.layer_stack_id
                   AND f.parent_layer_id = l.parent_layer_id AND f.result_root_id = l.root_id
                   AND f.creation_kind = l.creation_kind
                   AND f.source_branch_id = l.source_branch_id
                   AND f.source_branch_depth = l.source_branch_depth
                   AND f.source_branch_generation = l.source_branch_generation
                   AND f.source_operation_version_id =
                       l.source_branch_head_operation_version_id
                   AND f.source_branch_delta_id = l.source_branch_delta_id
                   AND f.transition_delta_id = d.transition_delta_id
                   AND f.parent_root_id = d.parent_root AND f.state = l.state
                   AND f.prepared_request_id = l.prepared_request_id
                   AND f.accepted_generation = l.accepted_generation)",
            params![accepted_stack.layer_id.as_bytes().as_slice()],
            |row| row.get::<_, bool>(0),
        )
        .unwrap());
    assert_eq!(
        candidate
            .query_row(
                "SELECT (SELECT count(*) FROM layerfs_operation_versions),
                        (SELECT count(*) FROM layerfs_layers),
                        (SELECT count(*) FROM layerfs_branch_transitions),
                        (SELECT count(*) FROM layerfs_layer_stack_transitions),
                        (SELECT count(*) FROM layerfs_sync_receipts)",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .unwrap(),
        (1, 2, 1, 1, 1)
    );
    candidate.execute_batch("DETACH DATABASE legacy").unwrap();
    drop(candidate);

    let source_reopened = Engine::open(&source).unwrap();
    assert_eq!(
        source_reopened.product_branch_head(branch_id).unwrap(),
        Some(accepted_branch)
    );
    assert_eq!(
        source_reopened.product_layer_stack_head(stack_id).unwrap(),
        Some(accepted_stack)
    );
    drop(source_reopened);

    remove(&working_path);
    remove(&source);
    remove(&candidate_path);
}
