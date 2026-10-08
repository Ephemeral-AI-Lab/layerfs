//! Project provisioning and authenticated filesystem facades, with no host data service.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod control;
mod operation;
pub mod project;
#[cfg(unix)]
pub mod sandbox;
pub mod workspace;
pub use operation::{OperationCause, OperationFailure};
pub use project::{
    initialize, install, InitError, InitFailure, InitRequest, InstallError, InstallFailure,
    InstallWork, Installed, ProjectApi, SealedProject,
};
#[cfg(unix)]
pub use sandbox::{
    ManagedSandbox, SandboxApi, SandboxCause, SandboxCreate, SandboxFailure, SandboxPhase,
};
pub use workspace::{BoundWorkspace, MountFailure, MountedWorkspace, WorkspaceApi};
