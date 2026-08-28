use layerfs_core::ObjectId;
use layerfs_working_store::{BeginOperation, WorkingCandidate};

pub(crate) fn no_change(begin: &BeginOperation, root: ObjectId) -> WorkingCandidate {
    WorkingCandidate {
        operation_id: begin.operation_id,
        expected_branch_generation: begin.branch_head_before.generation,
        base_root: begin.base.root(),
        candidate_root: root,
        normalized_transition: Vec::new(),
    }
}
