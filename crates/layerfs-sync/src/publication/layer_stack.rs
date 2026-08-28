use crate::object_transfer::{push_objects, WorkingObjectPages};
use crate::{
    BranchId, Direction, DurableControlEndpoint, LayerCandidate, LayerId, LayerStackHead,
    LayerStackMergeOutcome, LayerStackRollbackOutcome, PushLayerStackGenesisReceipt, RequestId,
    Result, ResumeToken, SyncError,
};
use layerfs_working_store::WorkingStore;
use std::time::Instant;

pub fn push_layer_stack_genesis(
    source: &WorkingStore,
    destination: &impl DurableControlEndpoint,
    request_id: [u8; 32],
    branch_id: BranchId,
    stack: LayerStackHead,
    name: &str,
    resume: ResumeToken,
) -> Result<PushLayerStackGenesisReceipt> {
    let complete = Instant::now();
    let mut object_ids = WorkingObjectPages::new(source, branch_id, None);
    let transfer = push_objects(source, destination, request_id, &mut object_ids, resume)?;
    if let Some(error) = object_ids.error.take() {
        return Err(error);
    }
    let closure_traversal_ns = object_ids.traversal_ns;
    let head_transaction = Instant::now();
    let head = destination.bootstrap_layer_stack(
        stack.layer_stack_id,
        stack.layer_id,
        name,
        stack.root,
    )?;
    destination.abort_transfer(RequestId::from_bytes(request_id), Direction::Push)?;
    source
        .clear_transfer_state(RequestId::from_bytes(request_id), "push")
        .map_err(|error| SyncError::Progress(error.to_string()))?;
    let head_transaction_ns = head_transaction.elapsed().as_nanos();
    if head != stack {
        return Err(SyncError::Destination(
            "LayerStack bootstrap head".to_owned(),
        ));
    }
    Ok(PushLayerStackGenesisReceipt {
        head,
        transfer,
        closure_traversal_ns,
        head_transaction_ns,
        complete_wall_ns: complete.elapsed().as_nanos(),
    })
}

pub fn push_layer_stack_merge(
    destination: &impl DurableControlEndpoint,
    candidate: LayerCandidate,
    expected: LayerStackHead,
) -> Result<LayerStackMergeOutcome> {
    destination.accept_layer_stack_merge(candidate, expected)
}

pub fn push_layer_stack_rollback(
    destination: &impl DurableControlEndpoint,
    expected: LayerStackHead,
    target: LayerId,
) -> Result<LayerStackRollbackOutcome> {
    destination.layer_stack_rollback(expected, target)
}
