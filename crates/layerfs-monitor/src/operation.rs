use layerfs_layerstack_store::{
    BranchId, CommitId, EntityName, LayerId, LayerStackId, StorageReceipt,
    ADMISSION_BATCH_COUNT, OBJECT_PAGE_BYTES,
};
use layerfs_workspace::{ExecutionId, WorkspaceId};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OperationId(pub u64);

impl OperationId {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for OperationId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationFamily {
    LayerStackInitialize,
    LayerStackDiff,
    LayerStackAdd,
    BranchFork,
    BranchDiff,
    WorkspaceCreate,
    WorkspaceExec,
    WorkspaceShell,
    WorkspaceOutput,
    WorkspaceStop,
    WorkspaceConflicts,
    WorkspaceResolve,
    WorkspaceFileRangeEdit,
    WorkspaceCommit,
    WorkspaceEnd,
    Query,
    DedupAnalyze,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationOutcome {
    Success,
    UpToDate,
    NoChanges,
    HeadMoved,
    Busy,
    Failed,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CandidateStats {
    pub candidate_objects: u64,
    pub candidate_bytes: u64,
    pub inserted_objects: u64,
    pub inserted_bytes: u64,
    pub reused_objects: u64,
    pub reused_bytes: u64,
    pub batch_inserted_objects: u64,
    pub batch_inserted_bytes: u64,
    pub final_inserted_objects: u64,
    pub final_inserted_bytes: u64,
    pub preexisting_reused_objects: u64,
    pub preexisting_reused_bytes: u64,
    pub admission_transactions: u64,
    pub max_transaction_objects: u64,
    pub max_transaction_bytes: u64,
}

impl CandidateStats {
    pub fn validate(self) -> bool {
        self.validate_for(OperationFamily::WorkspaceCommit)
    }

    pub fn validate_for(self, _family: OperationFamily) -> bool {
        self.candidate_objects == self.inserted_objects + self.reused_objects
            && self.candidate_bytes == self.inserted_bytes + self.reused_bytes
            && self.inserted_objects == self.batch_inserted_objects + self.final_inserted_objects
            && self.inserted_bytes == self.batch_inserted_bytes + self.final_inserted_bytes
            && self.reused_objects == self.preexisting_reused_objects
            && self.reused_bytes == self.preexisting_reused_bytes
            && self.max_transaction_objects <= ADMISSION_BATCH_COUNT as u64
            && self.max_transaction_bytes < OBJECT_PAGE_BYTES as u64
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SemanticOperation {
    pub family: OperationFamily,
    pub name: Option<EntityName>,
    pub layer_stack_id: Option<LayerStackId>,
    pub layer_id: Option<LayerId>,
    pub branch_id: Option<BranchId>,
    pub commit_id: Option<CommitId>,
    pub workspace_id: Option<WorkspaceId>,
    pub execution_id: Option<ExecutionId>,
}

impl SemanticOperation {
    pub fn new(family: OperationFamily) -> Self {
        Self {
            family,
            name: None,
            layer_stack_id: None,
            layer_id: None,
            branch_id: None,
            commit_id: None,
            workspace_id: None,
            execution_id: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationReceipt {
    pub id: OperationId,
    pub operation: SemanticOperation,
    pub outcome: OperationOutcome,
    pub queue_ns: u64,
    pub service_ns: u64,
    pub candidate: Option<CandidateStats>,
    pub storage: Vec<StorageReceipt>,
}

impl OperationReceipt {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"schema_version\":4,\"id\":{},\"family\":\"{}\",\"outcome\":\"{}\",\"queue_ns\":{},\"service_ns\":{}}}",
            self.id.0,
            family_name(self.operation.family),
            outcome_name(self.outcome),
            self.queue_ns,
            self.service_ns,
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimingFragment {
    pub name: String,
    pub elapsed_ns: u64,
}

fn family_name(family: OperationFamily) -> &'static str {
    match family {
        OperationFamily::LayerStackInitialize => "layerstack.initialize",
        OperationFamily::LayerStackDiff => "layerstack.diff",
        OperationFamily::LayerStackAdd => "layerstack.add",
        OperationFamily::BranchFork => "branch.fork",
        OperationFamily::BranchDiff => "branch.diff",
        OperationFamily::WorkspaceCreate => "workspace.create",
        OperationFamily::WorkspaceExec => "workspace.exec",
        OperationFamily::WorkspaceShell => "workspace.shell",
        OperationFamily::WorkspaceOutput => "workspace.output",
        OperationFamily::WorkspaceStop => "workspace.stop",
        OperationFamily::WorkspaceConflicts => "workspace.conflicts",
        OperationFamily::WorkspaceResolve => "workspace.resolve",
        OperationFamily::WorkspaceFileRangeEdit => "workspace.file_range_edit",
        OperationFamily::WorkspaceCommit => "workspace.commit",
        OperationFamily::WorkspaceEnd => "workspace.end",
        OperationFamily::Query => "query",
        OperationFamily::DedupAnalyze => "dedup.analyze",
    }
}

fn outcome_name(outcome: OperationOutcome) -> &'static str {
    match outcome {
        OperationOutcome::Success => "success",
        OperationOutcome::UpToDate => "up_to_date",
        OperationOutcome::NoChanges => "no_changes",
        OperationOutcome::HeadMoved => "head_moved",
        OperationOutcome::Busy => "busy",
        OperationOutcome::Failed => "failed",
    }
}
