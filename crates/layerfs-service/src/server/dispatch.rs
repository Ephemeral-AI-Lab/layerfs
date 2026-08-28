//! Authenticated wire-request dispatch.

use super::{history, object, pin, publication, reconcile};
use crate::protocol::request::{WireEnvelope, WireRequest};
use crate::protocol::response::WireResponse;
use crate::{Result, Service, ServiceError};
pub(crate) fn dispatch(service: &Service, envelope: WireEnvelope) -> Result<WireResponse> {
    let session = service.authenticate(&envelope.bearer)?;
    session.authorize_storage(envelope.expected_storage_id, &envelope.request)?;
    Ok(match envelope.request {
        WireRequest::StorageId => WireResponse::StorageId(session.storage_id()),
        WireRequest::BranchHead(branch) => WireResponse::BranchHead(session.branch_head(branch)?),
        WireRequest::BootstrapLayerStack(stack, layer, name, root) => {
            WireResponse::LayerStackHead(session.bootstrap_layer_stack(stack, layer, &name, root)?)
        }
        WireRequest::ReadObject(id, maximum) => WireResponse::Object(
            object::read_object(&session, id, maximum.min(layerfs_sync::MAX_BATCH_BYTES))
                .map_err(ServiceError::Sync)?,
        ),
        WireRequest::ContainsObject(id) => {
            WireResponse::Bool(object::contains_object(&session, id).map_err(ServiceError::Sync)?)
        }
        WireRequest::AcceptObjects(owner, request, direction, objects) => {
            object::accept_objects(&session, owner, request, direction, &objects)
                .map_err(ServiceError::Sync)?;
            WireResponse::Unit
        }
        WireRequest::AbortTransfer(owner, direction) => WireResponse::Count(
            pin::abort_transfer(&session, owner, direction).map_err(ServiceError::Sync)?,
        ),
        WireRequest::StageBranchPush(transfer, sequence, data_request, bundle, counters) => {
            publication::stage_branch_push_page(
                &session,
                transfer,
                sequence,
                data_request,
                &bundle,
                counters,
            )
            .map_err(ServiceError::Sync)?;
            WireResponse::Unit
        }
        WireRequest::CommitBranchPush(request, branch) => WireResponse::BranchPush(
            publication::commit_staged_branch_push(&session, request, branch)
                .map_err(ServiceError::Sync)?,
        ),
        WireRequest::ReconcileBranchPush(request, accepted) => WireResponse::BranchPush(
            reconcile::reconcile_branch_push(&session, request, accepted)
                .map_err(ServiceError::Sync)?,
        ),
        WireRequest::RecordReplayedBranchPush(request, accepted) => WireResponse::BranchPush(
            reconcile::record_replayed_branch_push(&session, request, accepted)
                .map_err(ServiceError::Sync)?,
        ),
        WireRequest::ExportBranchFetch(branch, base, stack_base) => WireResponse::BranchBundle(
            history::export_branch_fetch(&session, branch, base, stack_base)
                .map_err(ServiceError::Sync)?,
        ),
        WireRequest::BranchFetchObjectPage(
            branch,
            base,
            stack_base,
            expected_head,
            expected_stack_head,
            after,
            limit,
        ) => WireResponse::ObjectPage(
            history::branch_fetch_object_page(
                &session,
                branch,
                base,
                stack_base,
                expected_head,
                expected_stack_head,
                after,
                limit,
            )
            .map_err(ServiceError::Sync)?,
        ),
        WireRequest::AcceptChildBranchMerge(publication) => WireResponse::ChildMerge(
            publication::accept_child_branch_merge(&session, publication)
                .map_err(ServiceError::Sync)?,
        ),
        WireRequest::AcceptBranchRollback(publication) => WireResponse::BranchRollback(
            publication::accept_branch_rollback(&session, publication)
                .map_err(ServiceError::Sync)?,
        ),
        WireRequest::AcceptLayerStackMerge(candidate, expected) => {
            WireResponse::LayerStackMerge(session.accept_layer_stack_merge(candidate, expected)?)
        }
        WireRequest::LayerStackRollback(expected, target) => {
            WireResponse::LayerStackRollback(session.layer_stack_rollback(expected, target)?)
        }
    })
}
