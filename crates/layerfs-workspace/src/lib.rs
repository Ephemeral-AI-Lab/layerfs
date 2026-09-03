#![forbid(unsafe_code)]

mod capture;
mod changes;
mod container;
mod daemon;
mod docker;
mod docker_engine;
mod execution;
mod file_edit;
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

pub use container::{
    ContainerBinding, ContainerCreate, ContainerError, ContainerLimits, ContainerManager,
    ContainerResult, ContainerStatus, CreatedContainer, RunningContainer,
};
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
    ContainerId, CreateWorkspaceSession, DaemonTiming, EndWorkspaceMode, ExecutionEvent,
    ExecutionId, ExecutionReceipt, ExecutionSummary, ExecutionTransport, NonEmpty, OutputChunk,
    OutputStream, WorkspaceCommitResult, WorkspaceCommitStatus, WorkspaceDetail, WorkspaceDiff,
    WorkspaceEndResult, WorkspaceError, WorkspaceExecution, WorkspaceFileRangeEdit,
    WorkspaceFileReplacement, WorkspaceId, WorkspacePlacement, WorkspaceProjection,
    WorkspaceResult, WorkspaceSession, WorkspaceSummary,
};

#[cfg(debug_assertions)]
#[doc(hidden)]
pub fn inject_projection_refresh_failure_once() {
    projection::inject_refresh_failure_once();
}

#[cfg(debug_assertions)]
#[doc(hidden)]
pub fn inject_projection_resume_failure_once() {
    projection::inject_resume_failure_once();
}

#[cfg(debug_assertions)]
#[doc(hidden)]
pub fn inject_candidate_failure_once() {
    changes::inject_candidate_failure_once();
}
