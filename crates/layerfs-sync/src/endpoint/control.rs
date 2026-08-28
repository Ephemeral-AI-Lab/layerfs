use crate::{
    BranchHead, BranchId, BranchPushBundle, BranchPushOutcome, BranchPushRequest,
    BranchRollbackOutcome, BranchRollbackPublication, ChildMergeOutcome, ChildMergePublication,
    DurableEndpoint, LayerCandidate, LayerId, LayerStackHead, LayerStackId, LayerStackMergeOutcome,
    LayerStackRollbackOutcome, RequestId, Result, SyncTransferCounters,
};
use layerfs_core::ObjectId;

pub trait DurableControlEndpoint: DurableEndpoint {
    fn branch_head(&self, branch_id: BranchId) -> Result<Option<BranchHead>>;
    fn bootstrap_layer_stack(
        &self,
        stack: LayerStackId,
        layer: LayerId,
        name: &str,
        root: ObjectId,
    ) -> Result<LayerStackHead>;
    fn stage_branch_push_page(
        &self,
        transfer_id: RequestId,
        page_sequence: u64,
        data_request_id: RequestId,
        bundle: &BranchPushBundle,
        counters: SyncTransferCounters,
    ) -> Result<()>;
    fn commit_staged_branch_push(
        &self,
        request: BranchPushRequest,
        branch_id: BranchId,
    ) -> Result<BranchPushOutcome>;
    fn reconcile_branch_push(
        &self,
        request: BranchPushRequest,
        accepted: BranchHead,
    ) -> Result<BranchPushOutcome>;
    fn record_replayed_branch_push(
        &self,
        request: BranchPushRequest,
        accepted: BranchHead,
    ) -> Result<BranchPushOutcome>;
    fn export_branch_fetch(
        &self,
        branch_id: BranchId,
        base: Option<BranchHead>,
        origin_stack_base: Option<LayerStackHead>,
    ) -> Result<BranchPushBundle>;
    #[allow(clippy::too_many_arguments)]
    fn branch_fetch_object_page(
        &self,
        branch_id: BranchId,
        base: Option<BranchHead>,
        origin_stack_base: Option<LayerStackHead>,
        expected_head: BranchHead,
        expected_stack_head: LayerStackHead,
        after: Option<ObjectId>,
        limit: usize,
    ) -> Result<Vec<ObjectId>>;
    fn accept_child_branch_merge(
        &self,
        publication: ChildMergePublication,
    ) -> Result<ChildMergeOutcome>;
    fn accept_branch_rollback(
        &self,
        publication: BranchRollbackPublication,
    ) -> Result<BranchRollbackOutcome>;
    fn accept_layer_stack_merge(
        &self,
        candidate: LayerCandidate,
        expected: LayerStackHead,
    ) -> Result<LayerStackMergeOutcome>;
    fn layer_stack_rollback(
        &self,
        expected: LayerStackHead,
        target: LayerId,
    ) -> Result<LayerStackRollbackOutcome>;
}
