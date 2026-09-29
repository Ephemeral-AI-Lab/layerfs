//! Agent-facing project result and operation errors.
mod project;
mod sandbox;
mod workspace;
mod workspace_view;
pub use project::{Branch, Error, Project};
pub use sandbox::{DeleteError, SandboxId, SandboxInfo, SandboxStatus};
pub use workspace::{ExecResult, Mount, WorkspaceError, WorkspaceId, WorkspaceStatus};
pub use workspace_view::{
    WorkspaceViewDirectoryPage, WorkspaceViewEntry, WorkspaceViewKind, WorkspaceViewLease,
    WorkspaceViewRead, WorkspaceViewRelease, WorkspaceViewStatus,
};
