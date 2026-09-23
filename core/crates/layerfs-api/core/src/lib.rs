//! Agent-facing project result and operation errors.
mod project;
mod sandbox;
mod workspace;
pub use project::{Error, Project};
pub use sandbox::{SandboxId, SandboxInfo, SandboxStatus};
pub use workspace::{ExecResult, Mount, WorkspaceError, WorkspaceId};
