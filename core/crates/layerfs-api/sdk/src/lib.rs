//! Agent SDK over one composed host Server and its sandbox owner.
//!
//! The three implementation modules are the whole product surface: `project`
//! (Init and Branch fork), `sandbox` (create, list, delete) and `workspace`
//! (mount, exec, commit, status, unmount).
mod project;
mod sandbox;
mod workspace;
pub use layerfs_api_core::{
    Branch, DeleteError, Error, ExecResult, Mount, Project, SandboxId, SandboxInfo, SandboxStatus,
    WorkspaceError, WorkspaceId, WorkspaceStatus,
};
pub use layerfs_server::{HistoryMode, Server, ServerConfig};
pub use project::{ProjectApi, BRANCH_BODY_BYTES};
pub use sandbox::SandboxApi;
pub use workspace::WorkspaceApi;
