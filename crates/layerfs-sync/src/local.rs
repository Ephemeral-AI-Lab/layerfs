use crate::{
    BranchHead, BranchId, BranchPushBundle, BranchPushOutcome, BranchPushRequest,
    BranchRollbackOutcome, BranchRollbackPublication, ChildMergeOutcome, ChildMergePublication,
    Direction, DurableControlEndpoint, DurableEndpoint, LayerCandidate, LayerId, LayerStackHead,
    LayerStackId, LayerStackMergeOutcome, LayerStackRollbackOutcome, RequestId, Result, SyncError,
    SyncTransferCounters,
};
use layerfs_core::ObjectId;
use layerfs_durable_store::DurableStore;

pub struct LocalDurable<'a>(&'a DurableStore);

impl<'a> LocalDurable<'a> {
    pub const fn new(store: &'a DurableStore) -> Self {
        Self(store)
    }
}

impl DurableEndpoint for LocalDurable<'_> {
    fn durable_storage_id(&self) -> [u8; 32] {
        self.0.storage_id()
    }

    fn read_object(&self, id: ObjectId, maximum: usize) -> Result<Vec<u8>> {
        self.0
            .sync_read_object(id, maximum)
            .map_err(|error| SyncError::Source(error.to_string()))
    }

    fn contains_object(&self, id: ObjectId) -> Result<bool> {
        self.0
            .sync_has_object(id)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn accept_objects(
        &self,
        owner_request_id: RequestId,
        request_id: RequestId,
        direction: Direction,
        objects: &[(ObjectId, Vec<u8>)],
    ) -> Result<()> {
        self.0
            .sync_accept_objects(
                owner_request_id,
                request_id,
                direction_name(direction),
                objects,
            )
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn abort_transfer(&self, owner_request_id: RequestId, direction: Direction) -> Result<u64> {
        self.0
            .abort_sync_transfer(owner_request_id, direction_name(direction))
            .map_err(|error| SyncError::Destination(error.to_string()))
    }
}

impl DurableControlEndpoint for LocalDurable<'_> {
    fn branch_head(&self, branch_id: BranchId) -> Result<Option<BranchHead>> {
        self.0
            .branch_head(branch_id)
            .map_err(|error| SyncError::Source(error.to_string()))
    }

    fn bootstrap_layer_stack(
        &self,
        stack: LayerStackId,
        layer: LayerId,
        name: &str,
        root: ObjectId,
    ) -> Result<LayerStackHead> {
        self.0
            .bootstrap_layer_stack(stack, layer, name, root)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn stage_branch_push_page(
        &self,
        transfer_id: RequestId,
        page_sequence: u64,
        data_request_id: RequestId,
        bundle: &BranchPushBundle,
        counters: SyncTransferCounters,
    ) -> Result<()> {
        self.0
            .stage_branch_push_page(
                transfer_id,
                page_sequence,
                data_request_id,
                bundle,
                counters,
            )
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn commit_staged_branch_push(
        &self,
        request: BranchPushRequest,
        branch_id: BranchId,
    ) -> Result<BranchPushOutcome> {
        self.0
            .commit_staged_branch_push(request, branch_id)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn reconcile_branch_push(
        &self,
        request: BranchPushRequest,
        accepted: BranchHead,
    ) -> Result<BranchPushOutcome> {
        self.0
            .reconcile_branch_push(request, accepted)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn record_replayed_branch_push(
        &self,
        request: BranchPushRequest,
        accepted: BranchHead,
    ) -> Result<BranchPushOutcome> {
        self.0
            .record_replayed_branch_push(request, accepted)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn export_branch_fetch(
        &self,
        branch_id: BranchId,
        base: Option<BranchHead>,
        origin_stack_base: Option<LayerStackHead>,
    ) -> Result<BranchPushBundle> {
        self.0
            .export_branch_fetch(branch_id, base, origin_stack_base)
            .map_err(|error| SyncError::Source(error.to_string()))
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
    ) -> Result<Vec<ObjectId>> {
        self.0
            .branch_fetch_object_page(
                branch_id,
                base,
                origin_stack_base,
                expected_head,
                expected_stack_head,
                after,
                limit,
            )
            .map_err(|error| SyncError::Source(error.to_string()))
    }

    fn accept_child_branch_merge(
        &self,
        publication: ChildMergePublication,
    ) -> Result<ChildMergeOutcome> {
        self.0
            .accept_child_branch_merge(publication)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn accept_branch_rollback(
        &self,
        publication: BranchRollbackPublication,
    ) -> Result<BranchRollbackOutcome> {
        self.0
            .accept_branch_rollback(publication)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn accept_layer_stack_merge(
        &self,
        candidate: LayerCandidate,
        expected: LayerStackHead,
    ) -> Result<LayerStackMergeOutcome> {
        self.0
            .accept_layer_stack_merge(candidate, expected)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }

    fn layer_stack_rollback(
        &self,
        expected: LayerStackHead,
        target: LayerId,
    ) -> Result<LayerStackRollbackOutcome> {
        self.0
            .layer_stack_rollback(expected, target)
            .map_err(|error| SyncError::Destination(error.to_string()))
    }
}

const fn direction_name(direction: Direction) -> &'static str {
    match direction {
        Direction::Fetch => "fetch",
        Direction::Push => "push",
    }
}
