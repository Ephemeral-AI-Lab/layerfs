use crate::{OperationId, OperationState, Presentation, RuntimeObservation};
use layerfs_core::ObjectId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BeginOperationReceipt {
    pub operation_id: OperationId,
    pub working_storage_id: [u8; 32],
    pub expected_branch_generation: u64,
    pub base_root: ObjectId,
    pub presentation: Presentation,
    pub state: OperationState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinalizedCandidate {
    pub operation_id: OperationId,
    pub expected_branch_generation: u64,
    pub base_root: ObjectId,
    pub candidate_root: ObjectId,
    pub normalized_transition: Vec<u8>,
}

impl FinalizedCandidate {
    pub fn into_working(self) -> layerfs_working_store::WorkingCandidate {
        layerfs_working_store::WorkingCandidate {
            operation_id: working_operation_id(self.operation_id),
            expected_branch_generation: self.expected_branch_generation,
            base_root: self.base_root,
            candidate_root: self.candidate_root,
            normalized_transition: self.normalized_transition,
        }
    }
}

fn working_operation_id(operation_id: OperationId) -> layerfs_working_store::OperationId {
    layerfs_working_store::OperationId::from_bytes(operation_id.0)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EndOperationReceipt {
    pub operation_id: OperationId,
    pub state: OperationState,
    pub candidate_root: Option<ObjectId>,
    pub runtime_terminal: RuntimeObservation,
    pub cleanup_complete: bool,
}
