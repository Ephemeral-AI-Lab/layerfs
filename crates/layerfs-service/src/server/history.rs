use crate::AuthenticatedSession;
use layerfs_core::ObjectId;
use layerfs_durable_store::{BranchHead, BranchId, LayerStackHead};
use layerfs_sync::{BranchPushBundle, SyncError};

pub(super) fn branch_head(
    session: &AuthenticatedSession<'_>,
    branch_id: BranchId,
) -> layerfs_sync::Result<Option<BranchHead>> {
    session
        .durable()
        .branch_head(branch_id)
        .map_err(|error| SyncError::Source(error.to_string()))
}

pub(super) fn export_branch_fetch(
    session: &AuthenticatedSession<'_>,
    branch_id: BranchId,
    base: Option<BranchHead>,
    origin_stack_base: Option<LayerStackHead>,
) -> layerfs_sync::Result<BranchPushBundle> {
    session
        .durable()
        .export_branch_fetch(branch_id, base, origin_stack_base)
        .map_err(|error| SyncError::Source(error.to_string()))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn branch_fetch_object_page(
    session: &AuthenticatedSession<'_>,
    branch_id: BranchId,
    base: Option<BranchHead>,
    origin_stack_base: Option<LayerStackHead>,
    expected_head: BranchHead,
    expected_stack_head: LayerStackHead,
    after: Option<ObjectId>,
    limit: usize,
) -> layerfs_sync::Result<Vec<ObjectId>> {
    session
        .durable()
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
