#![forbid(unsafe_code)]

mod capture;
mod changes;
mod container;
mod daemon;
mod docker;
mod docker_engine;
mod execution;
pub(crate) use layerfs_workspace_core::file_edit;
mod file_io;
mod lifecycle;
mod live_backing;

mod cow_tree;
mod output;
mod projection;
mod reconcile;
mod registry;
mod remote_commit;
mod session;
mod snapshot_input;
mod worker;

pub use container::{
    ContainerBinding, ContainerCreate, ContainerError, ContainerLimits, ContainerManager,
    ContainerResult, ContainerStatus, CreatedContainer, RunningContainer,
};
pub(crate) use cow_tree::{Attr, Kind, NodeId, Workspace, ROOT};
pub use layerfs_daemon::protocol::CgroupResourceSample;
pub use layerfs_daemon::ResourceSampleClock;
pub(crate) use layerfs_workspace_core::ResourcePolicy;
pub use lifecycle::WorkspaceState;
#[cfg(feature = "test-instrumentation")]
pub use lifecycle::{
    arm_verification_fault, take_verification_fault_receipt, VerificationFault,
    VerificationFaultReceipt, VerificationWorkspaceState,
};
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

#[cfg(any(debug_assertions, feature = "test-instrumentation"))]
#[doc(hidden)]
pub fn inject_projection_refresh_failure_once() {
    projection::inject_refresh_failure_once();
}

#[cfg(any(debug_assertions, feature = "test-instrumentation"))]
#[doc(hidden)]
pub fn inject_projection_resume_failure_once() {
    projection::inject_resume_failure_once();
}

#[cfg(any(debug_assertions, feature = "test-instrumentation"))]
#[doc(hidden)]
pub fn inject_candidate_failure_once() {
    changes::inject_candidate_failure_once();
}

fn live_error(error: layerfs_workspace_core::Error) -> layerfs_layerstack_store::StoreError {
    use layerfs_layerstack_store::StoreError;
    use layerfs_workspace_core::Error;
    match error {
        Error::InvalidInput(message) => StoreError::InvalidInput(message),
        Error::Integrity(message) => StoreError::Integrity(message),
        Error::NotFound(message) => StoreError::NotFound(message),
        Error::Core(error) => error.into(),
    }
}
