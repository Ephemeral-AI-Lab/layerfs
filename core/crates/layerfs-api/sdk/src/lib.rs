//! Project provisioning and authenticated filesystem facades, with no host data service.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod control;
mod operation;
pub mod project;
pub mod workspace;
pub use operation::{OperationCause, OperationFailure};
pub use project::{
    initialize, install, InitError, InitFailure, InitRequest, InstallError, InstallFailure,
    InstallWork, Installed, ProjectApi, SealedProject,
};
pub use workspace::{BoundWorkspace, WorkspaceApi};
