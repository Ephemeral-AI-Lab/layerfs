use crate::{
    BranchHead, BranchPushOutcome, BranchPushRequest, DurableControlEndpoint, PushBranchReceipt,
    RequestId, Result, TransferReceipt, TransferResult,
};
use layerfs_working_store::WorkingStore;
use std::time::Instant;

pub(crate) fn reconcile_recorded_push(
    source: &WorkingStore,
    destination: &impl DurableControlEndpoint,
    request_id: [u8; 32],
    expected: Option<BranchHead>,
    complete: Instant,
) -> Result<Option<PushBranchReceipt>> {
    let Some(entry) = source
        .push_outbox_entry(RequestId::from_bytes(request_id))
        .map_err(|error| crate::SyncError::Progress(error.to_string()))?
    else {
        return Ok(None);
    };
    if entry.state != "accepted" {
        return Ok(None);
    }
    let request = entry
        .request
        .filter(|request| request.expected == expected)
        .ok_or_else(|| crate::SyncError::Progress("Push outbox identity".into()))?;
    let head_transaction = Instant::now();
    let outcome = destination.reconcile_branch_push(request, entry.head)?;
    let mut transfer = TransferReceipt::default_for(crate::Direction::Push, request_id);
    transfer.source_storage_id = source.storage_id();
    transfer.destination_storage_id = destination.durable_storage_id();
    transfer.result = TransferResult::ReconciledNoTransfer;
    Ok(Some(PushBranchReceipt {
        outcome,
        transfer,
        history_export_ns: 0,
        closure_traversal_ns: 0,
        staging_ns: 0,
        head_transaction_ns: head_transaction.elapsed().as_nanos(),
        complete_wall_ns: complete.elapsed().as_nanos(),
        terminal_queued_batches: 0,
        pages: 0,
        complete: true,
    }))
}

pub(crate) fn record_replayed_push(
    destination: &impl DurableControlEndpoint,
    request: BranchPushRequest,
    accepted: BranchHead,
) -> Result<BranchPushOutcome> {
    destination.record_replayed_branch_push(request, accepted)
}
