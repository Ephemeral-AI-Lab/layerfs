use crate::{
    BranchId, Command, CommitId, ConflictId, EntityName, ExecutionId, LayerId, LayerStackId,
    ObjectId, OperationId, WorkspaceId,
};
use std::fmt::{Display, Formatter};

pub type CliResult<T> = Result<T, CliError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CliError {
    Parse(String),
    NotFound(String),
    NameConflict(String),
    HeadMoved(String),
    Integrity(String),
    ReadOnly(String),
    WorkspaceBusy(String),
    WorkspaceDirty(String),
    NotPulled(String),
    AuthorityUnavailable(String),
    Database(String),
    Interrupted,
}

impl Display for CliError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(value) => write!(formatter, "parse: {value}"),
            Self::NotFound(value) => write!(formatter, "not found: {value}"),
            Self::NameConflict(value) => write!(formatter, "name conflict: {value}"),
            Self::HeadMoved(value) => write!(formatter, "head moved: {value}"),
            Self::Integrity(value) => write!(formatter, "integrity: {value}"),
            Self::ReadOnly(value) => write!(formatter, "read only: {value}"),
            Self::WorkspaceBusy(value) => write!(formatter, "workspace busy: {value}"),
            Self::WorkspaceDirty(value) => write!(formatter, "workspace dirty: {value}"),
            Self::NotPulled(value) => write!(formatter, "not pulled: {value}"),
            Self::AuthorityUnavailable(value) => {
                write!(formatter, "authority unavailable: {value}")
            }
            Self::Database(value) => write!(formatter, "database: {value}"),
            Self::Interrupted => formatter.write_str("interrupted"),
        }
    }
}

impl std::error::Error for CliError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemotePlacement {
    Reference,
    Replica,
}

impl Display for RemotePlacement {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Reference => "REF",
            Self::Replica => "REP",
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectRelation {
    NotPulled,
    Current { mode: RemotePlacement },
    PullBehind { mode: RemotePlacement, layers: u16 },
    AuthorityUnknown,
    Integrity,
}

impl Display for ProjectRelation {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotPulled => formatter.write_str("NOT PULLED"),
            Self::Current { mode } => write!(formatter, "{mode} SYNC"),
            Self::PullBehind { mode, layers } => write!(formatter, "{mode} PULL +{layers}"),
            Self::AuthorityUnknown => formatter.write_str("AUTH UNKNOWN"),
            Self::Integrity => formatter.write_str("INTEGRITY"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BranchRelation {
    AuthorityOnly,
    RemoteCurrent { mode: RemotePlacement },
    RemotePullBehind { mode: RemotePlacement, commits: u16 },
    LocalOnly,
    LocalCurrent,
    LocalPushAhead { commits: u16 },
    AuthorityAhead { commits: u16 },
    Diverged,
    Integrity,
}

impl Display for BranchRelation {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AuthorityOnly => formatter.write_str("NOT PULLED"),
            Self::RemoteCurrent { mode } => write!(formatter, "REM {mode} SYNC"),
            Self::RemotePullBehind { mode, commits } => {
                write!(formatter, "REM {mode} PULL +{commits}")
            }
            Self::LocalOnly => formatter.write_str("LOC UNPUSHED"),
            Self::LocalCurrent => formatter.write_str("LOC SYNC"),
            Self::LocalPushAhead { commits } => write!(formatter, "LOC PUSH +{commits}"),
            Self::AuthorityAhead { commits } => {
                write!(formatter, "LOC HEAD MOVED +{commits}")
            }
            Self::Diverged => formatter.write_str("LOC DIVERGED"),
            Self::Integrity => formatter.write_str("INTEGRITY"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayerCoverage {
    Complete,
    ParentBacked,
    NotPulled,
    Unavailable,
}

impl Display for LayerCoverage {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Complete => "complete",
            Self::ParentBacked => "parent-backed",
            Self::NotPulled => "not pulled",
            Self::Unavailable => "unavailable",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceState {
    Clean,
    Dirty,
    Running,
    Busy,
    HeadMoved,
    ReadOnly,
}

impl Display for WorkspaceState {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Clean => "clean",
            Self::Dirty => "dirty",
            Self::Running => "running",
            Self::Busy => "busy",
            Self::HeadMoved => "HeadMoved",
            Self::ReadOnly => "read-only",
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RouteTarget {
    Project(LayerStackId),
    Layer(LayerId),
    Branch(BranchId),
    Commit(BranchId, CommitId),
    Workspace(WorkspaceId),
    Operation(OperationId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SemanticAction {
    Pull,
    Fork,
    Push,
    Add,
    Diff,
    Materialize,
    Workspace,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next: Option<String>,
}

impl<T> Page<T> {
    pub fn all(items: Vec<T>) -> Self {
        Self { items, next: None }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageRequest {
    pub after: Option<String>,
    pub limit: u16,
}

impl PageRequest {
    pub fn first(limit: u16) -> Self {
        Self { after: None, limit }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectSummary {
    pub id: LayerStackId,
    pub name: EntityName,
    pub authority_head: LayerId,
    pub authority_number: u16,
    pub work_boundary: Option<LayerId>,
    pub work_number: Option<u16>,
    pub complete_roots: u16,
    pub relation: ProjectRelation,
    pub remote_branches: u16,
    pub local_branches: u16,
    pub workspaces: u16,
    pub dirty_workspaces: u16,
    pub running_workspaces: u16,
    pub busy_workspaces: u16,
    pub retained_workspaces: u16,
    pub observed: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayerView {
    pub id: LayerId,
    pub number: u16,
    pub parent: Option<LayerId>,
    pub root: ObjectId,
    pub source: Option<(BranchId, CommitId)>,
    pub authority: bool,
    pub work: bool,
    pub coverage: LayerCoverage,
    pub direct_branches: u16,
    pub authority_head: bool,
    pub work_boundary: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BranchOrigin {
    Layer(LayerId),
    Commit(BranchId, CommitId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitView {
    pub id: CommitId,
    pub number: u16,
    pub parent: Option<CommitId>,
    pub root: ObjectId,
    pub base_layer: LayerId,
    pub authority: bool,
    pub work: bool,
    pub inherited: bool,
    pub owned: bool,
    pub authority_head: bool,
    pub work_head: bool,
    pub child_branches: u16,
    pub workspaces: Vec<WorkspaceId>,
    pub accepted_layer: Option<LayerId>,
    pub actions: Vec<SemanticAction>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchView {
    pub id: BranchId,
    pub project_id: LayerStackId,
    pub name: EntityName,
    pub origin: BranchOrigin,
    pub authority_head: Option<CommitId>,
    pub authority_number: Option<u16>,
    pub work_head: Option<CommitId>,
    pub work_number: Option<u16>,
    pub remote_complete_through: Option<CommitId>,
    pub visible_roots_complete: bool,
    pub relation: BranchRelation,
    pub commits: Vec<CommitView>,
    pub commits_next: Option<String>,
    pub direct_children: u16,
    pub descendant_count: u16,
    pub workspace_count: u16,
    pub actions: Vec<SemanticAction>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectSnapshot {
    pub project: ProjectSummary,
    pub layers: Page<LayerView>,
    pub branches: Page<BranchView>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceView {
    pub id: WorkspaceId,
    pub project_id: LayerStackId,
    pub project_name: EntityName,
    pub branch_id: BranchId,
    pub branch_name: EntityName,
    pub branch_relation: BranchRelation,
    pub anchor_commit: Option<CommitId>,
    pub anchor_layer: Option<LayerId>,
    pub anchor_root: ObjectId,
    pub expected_branch_head: Option<CommitId>,
    pub published_commit: Option<CommitId>,
    pub published_root: Option<ObjectId>,
    pub state: WorkspaceState,
    pub generation: u64,
    pub projection: String,
    pub placement: String,
    pub mount: String,
    pub changed_paths: u16,
    pub output_bytes: u64,
    pub execution: Option<ExecutionId>,
    pub output: Vec<String>,
    pub conflicts: Vec<ConflictView>,
    pub files: Vec<WorkspaceFileView>,
    pub changes: Vec<DiffEntryView>,
    pub runs: Vec<WorkspaceRunView>,
    pub storage: WorkspaceStorageView,
    pub timing: WorkspaceTimingView,
    pub commit_receipt: Option<WorkspaceCommitReceipt>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkspaceFileKind {
    Directory,
    File,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FilePreview {
    None,
    Text(String),
    Binary,
    Truncated(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceFileView {
    pub path: String,
    pub kind: WorkspaceFileKind,
    pub bytes: u64,
    pub allocated_bytes: Option<u64>,
    pub preview: FilePreview,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilesSnapshot {
    pub target: RouteTarget,
    pub resolved: RouteTarget,
    pub root: ObjectId,
    pub generation: Option<u64>,
    pub files: Page<WorkspaceFileView>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceRunView {
    pub execution_id: ExecutionId,
    pub script: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub output_bytes: u64,
    pub elapsed_micros: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WorkspaceStorageView {
    pub logical_bytes: u64,
    pub materialized_allocated_bytes: u64,
    pub cow_delta_bytes: u64,
    pub base_reused_bytes: u64,
    pub candidate_objects: u64,
    pub candidate_bytes: u64,
    pub inserted_objects: u64,
    pub inserted_bytes: u64,
    pub reused_objects: u64,
    pub reused_bytes: u64,
    pub sqlite_growth_bytes: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WorkspaceTimingView {
    pub create_micros: u64,
    pub bash_last_micros: u64,
    pub bash_total_micros: u64,
    pub capture_micros: u64,
    pub admission_micros: u64,
    pub publish_micros: u64,
    pub commit_total_micros: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceCommitReceipt {
    pub commit_id: CommitId,
    pub root: ObjectId,
    pub generation: u64,
    pub changed_paths: u16,
    pub candidate_objects: u64,
    pub candidate_bytes: u64,
    pub inserted_objects: u64,
    pub inserted_bytes: u64,
    pub reused_objects: u64,
    pub reused_bytes: u64,
    pub sqlite_growth_bytes: u64,
    pub capture_micros: u64,
    pub admission_micros: u64,
    pub publish_micros: u64,
    pub total_micros: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConflictView {
    pub id: ConflictId,
    pub path: String,
    pub kind: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceSnapshot {
    pub workspaces: Page<WorkspaceView>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationState {
    Running,
    Succeeded,
    Failed,
    Interrupted,
}

impl Display for OperationState {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Running => "running",
            Self::Succeeded => "success",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationReceipt {
    pub facts_announced: u64,
    pub facts_missing: u64,
    pub facts_inserted: u64,
    pub objects_announced: u64,
    pub objects_missing: u64,
    pub objects_sent: u64,
    pub objects_inserted: u64,
    pub objects_raced: u64,
    pub elapsed_ms: u64,
    pub elapsed_micros: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationView {
    pub id: OperationId,
    pub title: String,
    pub project: Option<EntityName>,
    pub phase: String,
    pub completed: u64,
    pub total: u64,
    pub state: OperationState,
    pub receipt: OperationReceipt,
    pub events: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageSnapshot {
    pub layerstack_store_id: String,
    pub branch_store_id: String,
    pub projects: u16,
    pub layers: u16,
    pub authority_branches: u16,
    pub remote_branches: u16,
    pub local_branches: u16,
    pub shared_objects: u64,
    pub authority_bytes: u64,
    pub branch_bytes: u64,
    pub unique_bytes: u64,
    pub replica_roots: u16,
    pub reference_scopes: u16,
    pub analysis_available: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextProfile {
    pub layerstack: std::path::PathBuf,
    pub branch: std::path::PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivitySnapshot {
    pub operations: Page<OperationView>,
    pub storage: StorageSnapshot,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiffChange {
    Add,
    Remove,
    Modify,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffEntryView {
    pub path: String,
    pub change: DiffChange,
    pub aspects: Vec<String>,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DeltaSummary {
    pub added: u64,
    pub modified: u64,
    pub removed: u64,
    pub before_bytes: u64,
    pub after_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiffSnapshot {
    pub target: RouteTarget,
    pub from_target: Option<RouteTarget>,
    pub to_target: RouteTarget,
    pub from_root: Option<ObjectId>,
    pub to_root: ObjectId,
    pub summary: DeltaSummary,
    pub title: String,
    pub from: String,
    pub to: String,
    pub entries: Page<DiffEntryView>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ViewQuery {
    Projects(PageRequest),
    Project {
        id: LayerStackId,
        page: PageRequest,
    },
    Branch {
        id: BranchId,
        page: PageRequest,
    },
    Workspaces {
        project: Option<LayerStackId>,
        page: PageRequest,
    },
    Activity(PageRequest),
    Files {
        target: RouteTarget,
        page: PageRequest,
    },
    Changes {
        target: RouteTarget,
        page: PageRequest,
    },
    Diff {
        request: crate::DiffRequest,
        page: PageRequest,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ViewSnapshot {
    Projects(Page<ProjectSummary>),
    Project(ProjectSnapshot),
    Branch(ProjectSnapshot, BranchId),
    Workspaces(WorkspaceSnapshot),
    Activity(ActivitySnapshot),
    Files(FilesSnapshot),
    Changes(DiffSnapshot),
    Diff(DiffSnapshot),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandEffect {
    Read,
    Mutate,
    Execute,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanField {
    pub label: String,
    pub value: String,
}

pub(crate) fn field(label: &str, value: String) -> PlanField {
    PlanField {
        label: label.into(),
        value,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandPlan {
    pub title: String,
    pub effect: CommandEffect,
    pub summary: String,
    pub fields: Vec<PlanField>,
    pub consequences: Vec<String>,
    pub confirmation_required: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Completion {
    pub start: usize,
    pub end: usize,
    pub value: String,
    pub description: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandResult {
    Pull(String),
    Fork {
        branch_id: BranchId,
        name: EntityName,
        origin: String,
        remote_calls: u8,
        objects_copied: u8,
    },
    Push(String),
    Add(String),
    NeedsResolution {
        workspace_id: WorkspaceId,
        old_base: LayerId,
        current_layer: LayerId,
        conflict_count: u16,
    },
    Workspace(String),
    Diff(String),
    Monitor(String),
    Query(String),
    Initialized(String),
    Context(ContextProfile),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinishedStatus {
    Succeeded,
    Failed,
    Interrupted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CliEvent {
    Started {
        operation_id: OperationId,
        command: String,
    },
    Progress {
        operation_id: OperationId,
        phase: String,
        completed: u64,
        total: u64,
        elapsed_ms: u64,
    },
    Output {
        operation_id: OperationId,
        line: String,
    },
    Snapshot {
        operation_id: OperationId,
        scope: String,
    },
    Finished {
        operation_id: OperationId,
        status: FinishedStatus,
        result: Result<CommandResult, CliError>,
        receipt: OperationReceipt,
    },
}

impl CliEvent {
    pub fn operation_id(&self) -> &OperationId {
        match self {
            Self::Started { operation_id, .. }
            | Self::Progress { operation_id, .. }
            | Self::Output { operation_id, .. }
            | Self::Snapshot { operation_id, .. }
            | Self::Finished { operation_id, .. } => operation_id,
        }
    }
}

#[allow(dead_code)]
fn _command_is_public(_: Command) {}
