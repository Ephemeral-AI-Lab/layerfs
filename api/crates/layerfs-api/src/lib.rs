//! The SDK's public surface: the call table and its one dispatcher
//! (`calls`), over the management library `layerfs-control`, whose public
//! paths it re-exports for the front ends: project provisioning (`project`)
//! and Sandbox Workspace control (`sandbox`). The `fs`, `branch` and
//! `project.list_branches` methods (file reads and one-Commit writes, Branch
//! history and listing) are the Store engine's SDK facade
//! (layerfs-store-service `methods/`), run in the caller's process, the only
//! route for Store calls (#352 F-020); [`call`] is their method table.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod calls;
pub use layerfs_control::common::{home, names};
use layerfs_control::family;
#[doc(hidden)]
pub use layerfs_control::host;
#[doc(hidden)]
pub use layerfs_control::internal;
#[cfg(feature = "machine")]
#[doc(hidden)]
pub use layerfs_control::machine;
pub use layerfs_control::machine::worker;
#[doc(hidden)]
pub use layerfs_control::operator;
pub use layerfs_control::{
    call, projects, AddFailure, AddRequest, AddedProject, CreateRequest, InitCounters, InitCounts,
    InitError, InitFailure, InitRequest, InitStep, InstallError, InstallFailure, InstallWork,
    Installed, OperationCause, OperationFailure, ProjectApi, ProjectError, SdkError, SdkResult,
    SealedProject, SourceRefusal,
};
pub use layerfs_control::{project, sandbox, store};
#[cfg(unix)]
pub use sandbox::{
    connect_address, ProviderCreate, ProviderFailure, ProviderSandbox, SandboxApi, SandboxCause,
    SandboxPhase,
};
pub use sandbox::{BoundWorkspace, MountFailure, MountedWorkspace, Unmounted, WorkspaceApi};
