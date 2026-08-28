use crate::AuthenticatedSession;
use layerfs_core::ObjectId;
use layerfs_durable_store::{
    BranchHead, BranchId, LayerCandidate, LayerId, LayerStackHead, LayerStackId,
    LayerStackMergeOutcome, LayerStackRollbackOutcome,
};
use layerfs_sync::{
    BranchPushBundle, BranchPushOutcome, BranchPushRequest, BranchRollbackOutcome,
    BranchRollbackPublication, ChildMergeOutcome, ChildMergePublication, DurableControlEndpoint,
    RequestId, SyncError, SyncTransferCounters,
};

pub(super) fn stage_branch_push_page(
    session: &AuthenticatedSession<'_>,
    transfer_id: RequestId,
    page_sequence: u64,
    data_request_id: RequestId,
    bundle: &BranchPushBundle,
    counters: SyncTransferCounters,
) -> layerfs_sync::Result<()> {
    session
        .durable()
        .stage_branch_push_page(
            transfer_id,
            page_sequence,
            data_request_id,
            bundle,
            counters,
        )
        .map_err(|error| SyncError::Destination(error.to_string()))
}

pub(super) fn commit_staged_branch_push(
    session: &AuthenticatedSession<'_>,
    request: BranchPushRequest,
    branch_id: BranchId,
) -> layerfs_sync::Result<BranchPushOutcome> {
    session
        .durable()
        .commit_staged_branch_push(request, branch_id)
        .map_err(|error| SyncError::Destination(error.to_string()))
}

pub(super) fn accept_child_branch_merge(
    session: &AuthenticatedSession<'_>,
    publication: ChildMergePublication,
) -> layerfs_sync::Result<ChildMergeOutcome> {
    session
        .durable()
        .accept_child_branch_merge(publication)
        .map_err(|error| SyncError::Destination(error.to_string()))
}

pub(super) fn accept_branch_rollback(
    session: &AuthenticatedSession<'_>,
    publication: BranchRollbackPublication,
) -> layerfs_sync::Result<BranchRollbackOutcome> {
    session
        .durable()
        .accept_branch_rollback(publication)
        .map_err(|error| SyncError::Destination(error.to_string()))
}

impl DurableControlEndpoint for AuthenticatedSession<'_> {
    fn branch_head(&self, branch_id: BranchId) -> layerfs_sync::Result<Option<BranchHead>> {
        super::history::branch_head(self, branch_id)
    }

    fn bootstrap_layer_stack(
        &self,
        stack: LayerStackId,
        layer: LayerId,
        name: &str,
        root: ObjectId,
    ) -> layerfs_sync::Result<LayerStackHead> {
        AuthenticatedSession::bootstrap_layer_stack(self, stack, layer, name, root)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn stage_branch_push_page(
        &self,
        transfer_id: RequestId,
        page_sequence: u64,
        data_request_id: RequestId,
        bundle: &BranchPushBundle,
        counters: SyncTransferCounters,
    ) -> layerfs_sync::Result<()> {
        stage_branch_push_page(
            self,
            transfer_id,
            page_sequence,
            data_request_id,
            bundle,
            counters,
        )
    }

    fn commit_staged_branch_push(
        &self,
        request: BranchPushRequest,
        branch_id: BranchId,
    ) -> layerfs_sync::Result<BranchPushOutcome> {
        commit_staged_branch_push(self, request, branch_id)
    }

    fn reconcile_branch_push(
        &self,
        request: BranchPushRequest,
        accepted: BranchHead,
    ) -> layerfs_sync::Result<BranchPushOutcome> {
        super::reconcile::reconcile_branch_push(self, request, accepted)
    }

    fn record_replayed_branch_push(
        &self,
        request: BranchPushRequest,
        accepted: BranchHead,
    ) -> layerfs_sync::Result<BranchPushOutcome> {
        super::reconcile::record_replayed_branch_push(self, request, accepted)
    }

    fn export_branch_fetch(
        &self,
        branch_id: BranchId,
        base: Option<BranchHead>,
        origin_stack_base: Option<LayerStackHead>,
    ) -> layerfs_sync::Result<BranchPushBundle> {
        super::history::export_branch_fetch(self, branch_id, base, origin_stack_base)
    }

    fn branch_fetch_object_page(
        &self,
        branch_id: BranchId,
        base: Option<BranchHead>,
        origin_stack_base: Option<LayerStackHead>,
        expected_head: BranchHead,
        expected_stack_head: LayerStackHead,
        after: Option<ObjectId>,
        limit: usize,
    ) -> layerfs_sync::Result<Vec<ObjectId>> {
        super::history::branch_fetch_object_page(
            self,
            branch_id,
            base,
            origin_stack_base,
            expected_head,
            expected_stack_head,
            after,
            limit,
        )
    }

    fn accept_child_branch_merge(
        &self,
        publication: ChildMergePublication,
    ) -> layerfs_sync::Result<ChildMergeOutcome> {
        accept_child_branch_merge(self, publication)
    }

    fn accept_branch_rollback(
        &self,
        publication: BranchRollbackPublication,
    ) -> layerfs_sync::Result<BranchRollbackOutcome> {
        accept_branch_rollback(self, publication)
    }

    fn accept_layer_stack_merge(
        &self,
        candidate: LayerCandidate,
        expected: LayerStackHead,
    ) -> layerfs_sync::Result<LayerStackMergeOutcome> {
        AuthenticatedSession::accept_layer_stack_merge(self, candidate, expected)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn layer_stack_rollback(
        &self,
        expected: LayerStackHead,
        target: LayerId,
    ) -> layerfs_sync::Result<LayerStackRollbackOutcome> {
        AuthenticatedSession::layer_stack_rollback(self, expected, target)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }
}
