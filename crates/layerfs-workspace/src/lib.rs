#![forbid(unsafe_code)]

mod changes;
mod docker;
mod execution;
mod file_io;
mod lifecycle;

mod cow_tree;
mod limits;
mod output;
mod placement;
mod projection;
mod registry;
mod session;
mod worker;

pub(crate) use cow_tree::{Attr, Kind, NodeId, Workspace, ROOT};
pub use lifecycle::WorkspaceState;
pub(crate) use limits::ResourcePolicy;
pub use output::{OutputPage, OutputReader};
pub use placement::{ContainerId, WorkspacePlacement, WorkspaceProjection};
pub use registry::Workspaces;
pub use session::{
    CreateWorkspaceSession, EndWorkspaceMode, ExecutionEvent, ExecutionId, ExecutionReceipt,
    ExecutionSummary, NonEmpty, OutputChunk, OutputStream, WorkspaceCommitResult, WorkspaceDetail,
    WorkspaceDiff, WorkspaceEndResult, WorkspaceError, WorkspaceExecution, WorkspaceResult,
    WorkspaceSession, WorkspaceSessionId, WorkspaceSummary,
};

#[cfg(test)]
#[path = "../tests/support/lifecycle.rs"]
mod lifecycle_tests;
