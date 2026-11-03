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
/// `layerfs doctor` (#536 H7): report-only, outside the call table.
pub use layerfs_control::doctor::{doctor, doctor_json};
#[cfg(feature = "machine")]
#[doc(hidden)]
pub use layerfs_control::machine;
pub use layerfs_control::machine::worker;
pub use layerfs_control::{
    call, projects, AddFailure, AddRequest, AddedProject, CreateRequest, InitCounters, InitCounts,
    InitError, InitFailure, InitRequest, InitStep, InstallError, InstallFailure, InstallWork,
    Installed, OperationCause, OperationFailure, ProjectApi, ProjectError, SdkError, SdkResult,
    SealedProject, SourceRefusal,
};
pub use layerfs_control::{daemon, project, sandbox, store};
pub use layerfs_control::{
    BoundWorkspace, MountFailure, MountedWorkspace, Unmounted, WorkspaceApi,
};
