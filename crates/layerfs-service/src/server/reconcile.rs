use crate::AuthenticatedSession;
use layerfs_durable_store::BranchHead;
use layerfs_sync::{BranchPushOutcome, BranchPushRequest, SyncError};

pub(super) fn reconcile_branch_push(
    session: &AuthenticatedSession<'_>,
    request: BranchPushRequest,
    accepted: BranchHead,
) -> layerfs_sync::Result<BranchPushOutcome> {
    session
        .durable()
        .reconcile_branch_push(request, accepted)
        .map_err(|error| SyncError::Destination(error.to_string()))
}

pub(super) fn record_replayed_branch_push(
    session: &AuthenticatedSession<'_>,
    request: BranchPushRequest,
    accepted: BranchHead,
) -> layerfs_sync::Result<BranchPushOutcome> {
    session
        .durable()
        .record_replayed_branch_push(request, accepted)
        .map_err(|error| SyncError::Destination(error.to_string()))
}
