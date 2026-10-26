//! The SDK's public surface, one Rust function per JavaScript method in
//! `{group}/{method}` files: project provisioning and Branch listing (`project`),
//! file reads and one-Commit writes (`fs`), Branch history (`branch`) and
//! Sandbox Workspace control (`sandbox`). The `fs`, `branch` and
//! `project.list_branches` methods read their arguments and run the Store
//! engine (layerfs-store-service) in the caller's process, the only route for
//! Store calls (#352 F-020); [`call`] is their method table.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod base;
#[doc(hidden)]
pub use base::internal;
pub mod branch;
pub mod fs;
#[doc(hidden)]
pub mod operator;
pub mod project;
pub mod sandbox;
pub use base::dispatch::call;
pub use base::failure::{OperationCause, OperationFailure};
pub use base::sdk::{SdkError, SdkResult};
pub use layerfs_project::store::{
    InitError, InitFailure, InitRequest, InstallError, InstallFailure, InstallWork, Installed,
    ProjectApi, SealedProject,
};
pub use layerfs_project::{InitCounters, InitCounts, InitStep, ProjectError, SourceRefusal};
#[cfg(unix)]
pub use sandbox::{
    connect_address, ManagedSandbox, SandboxApi, SandboxCause, SandboxCreate, SandboxFailure,
    SandboxPhase,
};
pub use sandbox::{BoundWorkspace, MountFailure, MountedWorkspace, Unmounted, WorkspaceApi};
