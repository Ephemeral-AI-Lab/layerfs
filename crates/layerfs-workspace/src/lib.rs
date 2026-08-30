#![forbid(unsafe_code)]

mod changes;
mod docker;
mod execution;
mod file_io;
mod lifecycle;

mod cow_tree;
mod limits;
mod output;
mod projection;
mod reconcile;
mod registry;
mod session;
mod worker;

pub(crate) use cow_tree::{Attr, Kind, NodeId, Workspace, ROOT};
pub use lifecycle::WorkspaceState;
pub(crate) use limits::ResourcePolicy;
pub use output::{OutputPage, OutputReader};
pub use reconcile::{
    ConflictCursor, ConflictId, ConflictKind, ConflictPage, ResolveChoice, ResolveResult,
    WorkspaceConflict,
};
pub use registry::Workspaces;
pub use session::{
    ContainerId, CreateWorkspaceSession, EndWorkspaceMode, ExecutionEvent, ExecutionId,
    ExecutionReceipt, ExecutionSummary, NonEmpty, OutputChunk, OutputStream, WorkspaceCommitResult,
    WorkspaceDetail, WorkspaceDiff, WorkspaceEndResult, WorkspaceError, WorkspaceExecution,
    WorkspaceId, WorkspacePlacement, WorkspaceProjection, WorkspaceResult, WorkspaceSession,
    WorkspaceSummary,
};
