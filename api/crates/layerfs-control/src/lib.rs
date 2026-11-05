//! The management library under the SDK call surface (`layerfs-api`): the
//! home and its records, machines and their workers, Stores, projects,
//! Sandboxes and their Workspaces, and backups (#536), the launcher-owned
//! step runtime outside the Sandbox compute plane (#555), and the call
//! machinery (`calls`). `layerfs-api` holds the call table's entries, one
//! file per call, and runs each call here.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

#[doc(hidden)]
pub use daemon::internal;
pub mod backup;
pub mod calls;
pub mod common;
pub mod daemon;
pub mod doctor;
#[doc(hidden)]
pub mod inventory;
#[doc(hidden)]
pub mod machine;
pub mod project;
pub mod sandbox;
pub mod step;
pub mod store;
pub mod workspace;
pub use daemon::call::{SdkError, SdkResult};
pub use daemon::failure::{OperationCause, OperationFailure};
pub use daemon::workspace::{
    BoundWorkspace, MountFailure, MountedWorkspace, Unmounted, WorkspaceApi,
};
pub use layerfs_project::store::{
    projects, AddFailure, AddRequest, AddedProject, CreateRequest, InitError, InitFailure,
    InitRequest, InstallError, InstallFailure, InstallWork, Installed, ProjectApi, SealedProject,
};
pub use layerfs_project::{InitCounters, InitCounts, InitStep, ProjectError, SourceRefusal};
pub use layerfs_store_service::methods::call;
#[cfg(unix)]
pub use sandbox::{
    connect_address, ProviderCreate, ProviderFailure, ProviderSandbox, SandboxApi, SandboxCause,
    SandboxPhase,
};
