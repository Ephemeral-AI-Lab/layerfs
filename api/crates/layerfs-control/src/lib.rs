//! The management library under the SDK call surface (`layerfs-api`): the
//! home and its records, machines and their workers, Stores, projects,
//! Sandboxes and their Workspaces, and backups (#536). `layerfs-api` keeps
//! the call table and its dispatcher and runs each call here.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod base;
#[doc(hidden)]
pub use base::internal;
pub mod common;
#[doc(hidden)]
pub mod family;
#[doc(hidden)]
pub mod host;
#[cfg(feature = "machine")]
#[doc(hidden)]
pub mod machine;
#[doc(hidden)]
pub mod operator;
pub mod project;
pub mod sandbox;
pub mod store;
pub mod worker;
pub use base::failure::{OperationCause, OperationFailure};
pub use base::sdk::{SdkError, SdkResult};
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
pub use sandbox::{BoundWorkspace, MountFailure, MountedWorkspace, Unmounted, WorkspaceApi};
