//! Agent SDK over one composed host Server and its sandbox owner.
//!
//! The three implementation modules are the whole product surface: `project`
//! (Init and Branch fork), `sandbox` (create, list, delete) and `workspace`
//! (mount, exec, commit, status, unmount).
mod project;
mod sandbox;
mod workspace;
mod workspace_view;
pub use layerfs_api_core::{
    Branch, DeleteError, Error, ExecResult, Mount, Project, SandboxId, SandboxInfo, SandboxStatus,
    WorkspaceError, WorkspaceId, WorkspaceStatus, WorkspaceViewDirectoryPage, WorkspaceViewEntry,
    WorkspaceViewKind, WorkspaceViewLease, WorkspaceViewRead, WorkspaceViewRelease,
    WorkspaceViewStatus,
};
pub use layerfs_bridge::contract::{CommitOutcomeWire, WorkspaceCommitReportWire};
pub use layerfs_server::{HistoryMode, Server, ServerConfig};
pub use project::{ProjectApi, BRANCH_BODY_BYTES};
pub use sandbox::SandboxApi;
pub use workspace::WorkspaceApi;
pub use workspace_view::{WORKSPACE_VIEW_LIST_ENTRIES, WORKSPACE_VIEW_READ_BYTES};
