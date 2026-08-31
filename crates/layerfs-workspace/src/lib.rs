#![forbid(unsafe_code)]

mod capture;
mod changes;
mod container;
mod daemon;
mod docker;
mod docker_engine;
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
    OutputStream, WorkspaceCommitResult, WorkspaceDetail, WorkspaceDiff, WorkspaceEndResult,
    WorkspaceError, WorkspaceExecution, WorkspaceId, WorkspacePlacement, WorkspaceProjection,
    WorkspaceResult, WorkspaceSession, WorkspaceSummary,
};
