use crate::history::{
    dependency_request_id, merge_transfer_receipt, page_request_id, prepare_branch_push_page,
    stage_branch_push_page, PreparedPushPage,
};
use crate::reconcile::{reconcile_recorded_push, record_replayed_push};
use crate::{
    BranchHead, BranchId, BranchPushOutcome, BranchPushRequest, BranchRollbackOutcome,
    BranchRollbackPublication, ChildMergeOutcome, ChildMergePublication, Direction,
    DurableControlEndpoint, PushBranchReceipt, RequestId, Result, ResumeToken, SyncError,
    SyncTransferCounters, TransferReceipt,
};
use layerfs_working_store::WorkingStore;
use std::time::Instant;

pub fn push_branch(
    source: &WorkingStore,
    destination: &impl DurableControlEndpoint,
    request_id: [u8; 32],
    branch_id: BranchId,
    expected: Option<BranchHead>,
    mut resume: ResumeToken,
) -> Result<PushBranchReceipt> {
    let complete = Instant::now();
    if let Some(receipt) =
        reconcile_recorded_push(source, destination, request_id, expected, complete)?
    {
        return Ok(receipt);
    }
    if source
        .branch_has_special_history_after(branch_id, expected.map_or(0, |head| head.generation))
        .map_err(|error| SyncError::Source(error.to_string()))?
    {
        return push_branch_replaying_history(
            source,
            destination,
            request_id,
            branch_id,
            expected,
            resume,
            complete,
        );
    }
    let mut page_base = expected;
    let mut transfer = None;
    let mut page = 0_u64;
    let transfer_id = RequestId::from_bytes(request_id);
    let mut identity = layerfs_storage::BranchPushIdentityBuilder::new(transfer_id);
    let mut history_export_ns = 0_u128;
    let mut closure_traversal_ns = 0_u128;
    let mut staging_ns = 0_u128;
    let final_head;
    loop {
        let page_request = page_request_id(
            request_id,
            b"push",
            page_base.map_or(0, |head| head.generation),
        );
        let staged = match stage_branch_push_page(
            source,
            destination,
            RequestId::from_bytes(request_id),
            page,
            page_request,
            branch_id,
            page_base,
            resume,
        ) {
            Ok(receipt) => receipt,
            Err(error) => {
                let head = source
                    .branch_head(branch_id)
                    .map_err(|source| SyncError::Source(source.to_string()))?
                    .ok_or_else(|| SyncError::Source("Push Branch disappeared".into()))?;
                source
                    .record_push_outbox(
                        RequestId::from_bytes(request_id),
                        destination.durable_storage_id(),
                        head,
                        expected,
                        None,
                        "transferring",
                    )
                    .map_err(|progress| SyncError::Progress(progress.to_string()))?;
                return Err(error);
            }
        };
        identity
            .absorb_page(page, staged.page_digest)
            .map_err(|error| SyncError::Source(error.to_string()))?;
        page = crate::types::add(page, 1)?;
        match transfer.as_mut() {
            Some(total) => merge_transfer_receipt(total, staged.transfer)?,
            None => transfer = Some(staged.transfer),
        }
        history_export_ns = crate::types::add_ns(history_export_ns, staged.history_export_ns)?;
        closure_traversal_ns =
            crate::types::add_ns(closure_traversal_ns, staged.closure_traversal_ns)?;
        staging_ns = crate::types::add_ns(staging_ns, staged.staging_ns)?;
        page_base = Some(staged.head);
        if staged.complete {
            final_head = staged.head;
            break;
        }
        resume = ResumeToken::default();
    }
    let mut transfer = transfer.ok_or(SyncError::CounterOverflow)?;
    transfer.request_id = request_id;
    let push_request = BranchPushRequest {
        request_id: transfer_id,
        transfer_id,
        candidate_digest: identity.finish(final_head),
        expected,
        counters: transfer_counters(&transfer),
    };
    source
        .record_push_outbox(
            RequestId::from_bytes(request_id),
            destination.durable_storage_id(),
            final_head,
            expected,
            Some(push_request),
            "transferred",
        )
        .map_err(|error| SyncError::Progress(error.to_string()))?;
    let head_transaction = Instant::now();
    let outcome = destination.commit_staged_branch_push(push_request, branch_id);
    let head_transaction_ns = head_transaction.elapsed().as_nanos();
    let state = match outcome {
        Ok(BranchPushOutcome::DurablyAccepted { .. }) => "accepted",
        Ok(BranchPushOutcome::Conflict { .. }) => "conflict",
        Err(_) => "indeterminate",
    };
    source
        .record_push_outbox(
            RequestId::from_bytes(request_id),
            destination.durable_storage_id(),
            final_head,
            expected,
            Some(push_request),
            state,
        )
        .map_err(|error| SyncError::Progress(error.to_string()))?;
    if outcome.is_ok() {
        source
            .clear_transfer_state_owner(RequestId::from_bytes(request_id), "push")
            .map_err(|error| SyncError::Progress(error.to_string()))?;
    }
    Ok(PushBranchReceipt {
        outcome: outcome?,
        transfer,
        history_export_ns,
        closure_traversal_ns,
        staging_ns,
        head_transaction_ns,
        complete_wall_ns: complete.elapsed().as_nanos(),
        terminal_queued_batches: 0,
        pages: page,
        complete: true,
    })
}

#[allow(clippy::too_many_arguments)]
fn push_branch_replaying_history(
    source: &WorkingStore,
    destination: &impl DurableControlEndpoint,
    request_id: [u8; 32],
    branch_id: BranchId,
    expected: Option<BranchHead>,
    mut resume: ResumeToken,
    complete: Instant,
) -> Result<PushBranchReceipt> {
    let mut current = expected;
    let mut total = None;
    let mut history_export_ns = 0_u128;
    let mut closure_traversal_ns = 0_u128;
    let mut staging_ns = 0_u128;
    let mut head_transaction_ns = 0_u128;
    let mut pages = 0_u64;
    let final_head;
    loop {
        let base_generation = current.map_or(0, |head| head.generation);
        let segment = RequestId::from_bytes(layerfs_storage::derive_id(
            b"ordered-push-segment",
            &[&request_id, &base_generation.to_be_bytes()],
        ));
        let data_request = page_request_id(request_id, b"push-replay", base_generation);
        let PreparedPushPage {
            mut bundle,
            transfer,
            history_export_ns: page_export_ns,
            closure_traversal_ns: page_closure_ns,
        } = prepare_branch_push_page(
            source,
            destination,
            segment,
            data_request,
            branch_id,
            current,
            resume,
        )?;
        pages = crate::types::add(pages, 1)?;
        history_export_ns = crate::types::add_ns(history_export_ns, page_export_ns)?;
        closure_traversal_ns = crate::types::add_ns(closure_traversal_ns, page_closure_ns)?;
        let page_counters = transfer_counters(&transfer);
        match total.as_mut() {
            Some(total) => merge_transfer_receipt(total, transfer)?,
            None => total = Some(transfer),
        }
        let done = bundle.complete;
        let action_started = Instant::now();
        let outcome = if !bundle.operations.is_empty()
            && bundle.child_merges.is_empty()
            && bundle.rollbacks.is_empty()
        {
            for operation in &mut bundle.operations {
                operation.release = None;
            }
            bundle.complete = true;
            let staging = Instant::now();
            destination.stage_branch_push_page(
                segment,
                0,
                RequestId::from_bytes(data_request),
                &bundle,
                page_counters,
            )?;
            staging_ns = crate::types::add_ns(staging_ns, staging.elapsed().as_nanos())?;
            destination.commit_staged_branch_push(
                BranchPushRequest {
                    request_id: segment,
                    transfer_id: segment,
                    candidate_digest: staged_page_candidate_digest(
                        segment,
                        RequestId::from_bytes(data_request),
                        &bundle,
                        page_counters,
                    )?,
                    expected: current,
                    counters: page_counters,
                },
                branch_id,
            )?
        } else if bundle.operations.is_empty()
            && bundle.child_merges.len() == 1
            && bundle.rollbacks.is_empty()
        {
            replay_child_merge(source, destination, request_id, branch_id, segment, &bundle)?
        } else if bundle.operations.is_empty()
            && bundle.child_merges.is_empty()
            && bundle.rollbacks.len() == 1
        {
            replay_rollback(destination, branch_id, current, segment, &bundle)?
        } else {
            return Err(SyncError::Source("ordered Push history page".into()));
        };
        head_transaction_ns =
            crate::types::add_ns(head_transaction_ns, action_started.elapsed().as_nanos())?;
        source
            .clear_transfer_state(RequestId::from_bytes(data_request), "push")
            .map_err(|error| SyncError::Progress(error.to_string()))?;
        match outcome {
            BranchPushOutcome::DurablyAccepted { head, .. } if head == bundle.head => {
                current = Some(head)
            }
            BranchPushOutcome::Conflict { .. } => {
                return replay_receipt(
                    outcome,
                    total,
                    request_id,
                    history_export_ns,
                    closure_traversal_ns,
                    staging_ns,
                    head_transaction_ns,
                    complete,
                    pages,
                )
            }
            _ => return Err(SyncError::Destination("ordered Push head".into())),
        }
        if done {
            final_head = bundle.head;
            break;
        }
        resume = ResumeToken::default();
    }
    let mut transfer = total.ok_or(SyncError::CounterOverflow)?;
    transfer.request_id = request_id;
    let receipt_started = Instant::now();
    let transfer_id = RequestId::from_bytes(request_id);
    let push_request = BranchPushRequest {
        request_id: transfer_id,
        transfer_id,
        candidate_digest: layerfs_storage::BranchPushIdentityBuilder::new(transfer_id)
            .finish(final_head),
        expected,
        counters: transfer_counters(&transfer),
    };
    let outcome = record_replayed_push(destination, push_request, final_head)?;
    head_transaction_ns =
        crate::types::add_ns(head_transaction_ns, receipt_started.elapsed().as_nanos())?;
    source
        .record_push_outbox(
            RequestId::from_bytes(request_id),
            destination.durable_storage_id(),
            final_head,
            expected,
            Some(push_request),
            "accepted",
        )
        .map_err(|error| SyncError::Progress(error.to_string()))?;
    Ok(PushBranchReceipt {
        outcome,
        transfer,
        history_export_ns,
        closure_traversal_ns,
        staging_ns,
        head_transaction_ns,
        complete_wall_ns: complete.elapsed().as_nanos(),
        terminal_queued_batches: 0,
        pages,
        complete: true,
    })
}

fn replay_child_merge(
    source: &WorkingStore,
    destination: &impl DurableControlEndpoint,
    request_id: [u8; 32],
    branch_id: BranchId,
    segment: RequestId,
    bundle: &crate::BranchPushBundle,
) -> Result<BranchPushOutcome> {
    let merge = &bundle.child_merges[0];
    let _ = push_branch(
        source,
        destination,
        dependency_request_id(request_id, merge.source_branch_id),
        merge.source_branch_id,
        None,
        ResumeToken::default(),
    )?;
    let publication = ChildMergePublication {
        candidate: layerfs_storage::ChildMergeCandidate {
            source: BranchHead {
                branch_id: merge.source_branch_id,
                generation: merge.source_branch_generation,
                operation_version_id: Some(merge.source_operation_version_id),
                root: merge.source_root,
            },
            expected_parent: BranchHead {
                branch_id,
                generation: merge.before_generation,
                operation_version_id: merge.parent_operation_version_id,
                root: merge.destination_root,
            },
            result_root: merge.root,
            source_transition: merge.source_transition_payload.clone(),
            applied_transition: merge.applied_transition_payload.clone(),
            request_id: merge.request_id,
        },
        accepted_parent: bundle.head,
    };
    let outcome = match destination.accept_child_branch_merge(publication)? {
        ChildMergeOutcome::WorkingRecorded { parent_head, .. } if parent_head == bundle.head => {
            BranchPushOutcome::DurablyAccepted {
                head: parent_head,
                reconciled: false,
            }
        }
        ChildMergeOutcome::Conflict { actual_parent } => BranchPushOutcome::Conflict {
            actual: Some(actual_parent),
        },
        _ => return Err(SyncError::Destination("Child merge replay receipt".into())),
    };
    destination.abort_transfer(segment, Direction::Push)?;
    Ok(outcome)
}

fn replay_rollback(
    destination: &impl DurableControlEndpoint,
    branch_id: BranchId,
    current: Option<BranchHead>,
    segment: RequestId,
    bundle: &crate::BranchPushBundle,
) -> Result<BranchPushOutcome> {
    let rollback = bundle.rollbacks[0];
    let outcome = match destination.accept_branch_rollback(BranchRollbackPublication {
        expected: BranchHead {
            branch_id,
            generation: rollback.before_generation,
            operation_version_id: Some(rollback.before_operation_version_id),
            root: current
                .ok_or_else(|| SyncError::Source("Rollback base".into()))?
                .root,
        },
        target: rollback.target_operation_version_id,
        request_id: rollback.request_id,
        accepted: bundle.head,
    })? {
        BranchRollbackOutcome::WorkingRecorded { head, .. } if head == bundle.head => {
            BranchPushOutcome::DurablyAccepted {
                head,
                reconciled: false,
            }
        }
        BranchRollbackOutcome::Conflict { actual } => BranchPushOutcome::Conflict {
            actual: Some(actual),
        },
        BranchRollbackOutcome::Blocked => {
            return Err(SyncError::Destination(
                "Branch rollback replay blocked".into(),
            ))
        }
        _ => {
            return Err(SyncError::Destination(
                "Branch rollback replay receipt".into(),
            ))
        }
    };
    destination.abort_transfer(segment, Direction::Push)?;
    Ok(outcome)
}

#[allow(clippy::too_many_arguments)]
fn replay_receipt(
    outcome: BranchPushOutcome,
    total: Option<TransferReceipt>,
    request_id: [u8; 32],
    history_export_ns: u128,
    closure_traversal_ns: u128,
    staging_ns: u128,
    head_transaction_ns: u128,
    complete: Instant,
    pages: u64,
) -> Result<PushBranchReceipt> {
    let mut transfer = total.ok_or(SyncError::CounterOverflow)?;
    transfer.request_id = request_id;
    Ok(PushBranchReceipt {
        outcome,
        transfer,
        history_export_ns,
        closure_traversal_ns,
        staging_ns,
        head_transaction_ns,
        complete_wall_ns: complete.elapsed().as_nanos(),
        terminal_queued_batches: 0,
        pages,
        complete: true,
    })
}

fn transfer_counters(transfer: &TransferReceipt) -> SyncTransferCounters {
    SyncTransferCounters {
        unique_bytes: transfer.unique_bytes,
        resumed_bytes: transfer.resumed_bytes,
        retransmitted_bytes: transfer.retransmitted_bytes,
    }
}

fn staged_page_candidate_digest(
    transfer_id: RequestId,
    data_request_id: RequestId,
    bundle: &crate::BranchPushBundle,
    counters: SyncTransferCounters,
) -> Result<[u8; 32]> {
    let page_digest = layerfs_storage::branch_push_bundle_page_digest(
        transfer_id,
        0,
        data_request_id,
        bundle,
        counters,
    )
    .map_err(|error| SyncError::Source(error.to_string()))?;
    let mut identity = layerfs_storage::BranchPushIdentityBuilder::new(transfer_id);
    identity
        .absorb_page(0, page_digest)
        .map_err(|error| SyncError::Source(error.to_string()))?;
    Ok(identity.finish(bundle.head))
}

pub fn push_child_branch_merge(
    destination: &impl DurableControlEndpoint,
    publication: ChildMergePublication,
) -> Result<ChildMergeOutcome> {
    destination.accept_child_branch_merge(publication)
}

pub fn push_branch_rollback(
    destination: &impl DurableControlEndpoint,
    publication: BranchRollbackPublication,
) -> Result<BranchRollbackOutcome> {
    destination.accept_branch_rollback(publication)
}
