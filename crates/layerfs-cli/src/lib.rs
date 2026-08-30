#![forbid(unsafe_code)]

mod command;
mod fixture;
mod ids;
mod model;
mod session;

pub use command::{Command, CommandKind, DiffRequest, WorkspaceAnchor};
pub use ids::{
    BranchId, CommitId, ConflictId, EntityName, ExecutionId, LayerId, LayerStackId, ObjectId,
    OperationId, WorkspaceId,
};
pub use model::{
    ActivitySnapshot, BranchOrigin, BranchRelation, BranchView, CliError, CliEvent, CliResult,
    CommandEffect, CommandPlan, CommandResult, CommitView, Completion, ConflictView, DiffChange,
    DiffEntryView, DiffSnapshot, FinishedStatus, LayerCoverage, LayerView, OperationReceipt,
    OperationState, OperationView, Page, PageRequest, PlanField, ProjectRelation, ProjectSnapshot,
    ProjectSummary, RemotePlacement, RouteTarget, SemanticAction, StorageSnapshot, ViewQuery,
    ViewSnapshot, WorkspaceSnapshot, WorkspaceState, WorkspaceView,
};
pub use session::{CliSession, OperationHandle};
