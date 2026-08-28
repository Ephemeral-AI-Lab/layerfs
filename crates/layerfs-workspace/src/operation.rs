use layerfs_core::ObjectId;
pub use layerfs_working_store::OperationId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Presentation {
    Direct,
    Mount,
    Materialization,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationState {
    Active,
    Frozen,
    Finalized,
    Cleaned,
    Incomplete,
}

/// Single-use runtime admission created only after the owning WorkingStore has
/// persisted the operation identity, expected head, base pin, and lease.
#[derive(Debug)]
pub struct WorkspaceTicket {
    pub operation_id: OperationId,
    pub working_storage_id: [u8; 32],
    pub expected_branch_generation: u64,
    pub base_root: ObjectId,
    pub nonce: [u8; 16],
    pub presentation: Presentation,
}

impl WorkspaceTicket {
    pub fn from_admission(
        admission: &layerfs_working_store::BeginOperation,
        presentation: Presentation,
    ) -> Self {
        Self {
            operation_id: admission.operation_id,
            working_storage_id: admission.working_storage_id,
            expected_branch_generation: admission.branch_head_before.generation,
            base_root: admission.base.root(),
            nonce: admission.workspace_nonce,
            presentation,
        }
    }
}

pub fn begin_operation(
    working: &layerfs_working_store::WorkingStore,
    expected: layerfs_working_store::BranchHead,
    presentation: Presentation,
) -> layerfs_working_store::Result<(layerfs_working_store::BeginOperation, WorkspaceTicket)> {
    let admission = working.begin_operation(expected)?;
    let ticket = WorkspaceTicket::from_admission(&admission, presentation);
    Ok((admission, ticket))
}
