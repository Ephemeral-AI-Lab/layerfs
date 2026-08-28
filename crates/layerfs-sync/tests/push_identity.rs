mod support;

use layerfs_core::ObjectId;
use layerfs_storage::{
    BranchPushBundle, BranchPushIdentityBuilder, BranchPushOutcome, BranchPushRequest,
    OperationVersionId, RequestId, SyncTransferCounters,
};
use layerfs_sync::{push_layer_stack_genesis, LocalDurable};
use layerfs_sync::{DurableControlEndpoint, ResumeToken};
use layerfs_working_store::CommitResult;
use support::{no_change, Scenario};

fn request_for_page(
    request_id: RequestId,
    transfer_id: RequestId,
    data_request_id: RequestId,
    page_sequence: u64,
    bundle: &BranchPushBundle,
) -> BranchPushRequest {
    let counters = SyncTransferCounters::default();
    let page_digest = layerfs_storage::branch_push_bundle_page_digest(
        transfer_id,
        page_sequence,
        data_request_id,
        bundle,
        counters,
    )
    .unwrap();
    let mut identity = BranchPushIdentityBuilder::new(transfer_id);
    if page_sequence == 0 {
        identity.absorb_page(0, page_digest).unwrap();
    }
    BranchPushRequest {
        request_id,
        transfer_id,
        candidate_digest: identity.finish(bundle.head),
        expected: None,
        counters,
    }
}

fn stage(
    endpoint: &impl DurableControlEndpoint,
    transfer_id: RequestId,
    data_request_id: RequestId,
    page_sequence: u64,
    bundle: &BranchPushBundle,
) -> layerfs_sync::Result<()> {
    endpoint.stage_branch_push_page(
        transfer_id,
        page_sequence,
        data_request_id,
        bundle,
        SyncTransferCounters::default(),
    )
}

#[test]
fn staged_push_replay_binds_candidate_page_chain_and_transfer_identity() {
    let scenario = Scenario::new();
    let begin = scenario.working.begin_operation(scenario.accepted).unwrap();
    let final_head = match scenario
        .working
        .operation_commit(begin, no_change(&begin, scenario.root))
        .unwrap()
    {
        CommitResult::WorkingRecorded { head, .. } => head,
        CommitResult::Conflict { .. } => panic!("unexpected Working conflict"),
    };
    let endpoint = LocalDurable::new(&scenario.durable);
    push_layer_stack_genesis(
        &scenario.working,
        &endpoint,
        [0xa0; 32],
        scenario.branch_id,
        scenario.stack,
        "main",
        ResumeToken::default(),
    )
    .unwrap();
    let bundle = scenario
        .working
        .export_branch_push(scenario.branch_id, None)
        .unwrap();
    assert_eq!(bundle.head, final_head);
    assert_eq!(bundle.operations.len(), 2);

    let request_id = RequestId::from_bytes([0xa1; 32]);
    let transfer_id = RequestId::from_bytes([0xa2; 32]);
    let data_request_id = RequestId::from_bytes([0xa3; 32]);
    let request = request_for_page(request_id, transfer_id, data_request_id, 0, &bundle);
    stage(&endpoint, transfer_id, data_request_id, 0, &bundle).unwrap();
    assert_eq!(
        scenario
            .durable
            .commit_staged_branch_push(request, scenario.branch_id)
            .unwrap(),
        BranchPushOutcome::DurablyAccepted {
            head: final_head,
            reconciled: false,
        }
    );

    stage(&endpoint, transfer_id, data_request_id, 0, &bundle).unwrap();
    assert_eq!(
        scenario
            .durable
            .commit_staged_branch_push(request, scenario.branch_id)
            .unwrap(),
        BranchPushOutcome::DurablyAccepted {
            head: final_head,
            reconciled: true,
        }
    );

    let unchanged_receipt_and_head =
        |changed_transfer: RequestId,
         changed_data: RequestId,
         sequence: u64,
         changed: &BranchPushBundle| {
            let staged = stage(&endpoint, changed_transfer, changed_data, sequence, changed);
            let changed_request = request_for_page(
                request_id,
                changed_transfer,
                changed_data,
                sequence,
                changed,
            );
            if staged.is_ok() {
                assert!(scenario
                    .durable
                    .commit_staged_branch_push(changed_request, scenario.branch_id)
                    .is_err());
            }
            assert_eq!(
                scenario.durable.branch_head(scenario.branch_id).unwrap(),
                Some(final_head)
            );
            scenario
                .durable
                .abort_sync_transfer(changed_transfer, "push")
                .unwrap();
            assert_eq!(
                scenario
                    .durable
                    .reconcile_branch_push(request, final_head)
                    .unwrap(),
                BranchPushOutcome::DurablyAccepted {
                    head: final_head,
                    reconciled: true,
                }
            );
        };

    let mut changed = bundle.clone();
    changed.name = Some("changed page bytes".into());
    unchanged_receipt_and_head(transfer_id, data_request_id, 0, &changed);

    let mut changed = bundle.clone();
    changed.operations.swap(0, 1);
    unchanged_receipt_and_head(transfer_id, data_request_id, 0, &changed);

    unchanged_receipt_and_head(transfer_id, data_request_id, 1, &bundle);

    let mut changed = bundle.clone();
    changed.head.operation_version_id = Some(OperationVersionId::from_bytes([0xb1; 32]));
    unchanged_receipt_and_head(transfer_id, data_request_id, 0, &changed);

    let mut changed = bundle.clone();
    changed.head.root = ObjectId::for_bytes(b"changed final root");
    unchanged_receipt_and_head(transfer_id, data_request_id, 0, &changed);

    let mut changed = bundle.clone();
    changed.head.generation += 1;
    changed
        .operations
        .push(changed.operations.last().unwrap().clone());
    unchanged_receipt_and_head(transfer_id, data_request_id, 0, &changed);

    unchanged_receipt_and_head(
        RequestId::from_bytes([0xb2; 32]),
        RequestId::from_bytes([0xb3; 32]),
        0,
        &bundle,
    );

    scenario.cleanup();
}
