#![forbid(unsafe_code)]

mod command;
mod database;
mod fixture;
mod ids;
mod model;
mod session;
mod workspace;

pub use command::{Command, CommandKind, DiffRequest, StoreRole, WorkspaceAnchor};
pub use ids::{
    BranchId, CommitId, ConflictId, EntityName, ExecutionId, LayerId, LayerStackId, ObjectId,
    OperationId, WorkspaceId,
};
pub use model::{
    ActivitySnapshot, BranchOrigin, BranchRelation, BranchView, CliError, CliEvent, CliResult,
    CommandEffect, CommandPlan, CommandResult, CommitView, Completion, ConflictView,
    ContextProfile, DiffChange, DiffEntryView, DiffSnapshot, FilePreview, FinishedStatus,
    LayerCoverage, LayerView, OperationReceipt, OperationState, OperationView, Page, PageRequest,
    PlanField, ProjectRelation, ProjectSnapshot, ProjectSummary, RemotePlacement, RouteTarget,
    SemanticAction, StorageSnapshot, ViewQuery, ViewSnapshot, WorkspaceCommitReceipt,
    WorkspaceFileKind, WorkspaceFileView, WorkspaceRunView, WorkspaceSnapshot, WorkspaceState,
    WorkspaceStorageView, WorkspaceTimingView, WorkspaceView,
};
pub use session::{CliSession, OperationHandle};
