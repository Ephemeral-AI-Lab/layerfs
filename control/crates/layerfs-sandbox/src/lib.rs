//! Sandboxes (#533 step 12; cut from `layerfs-control` by
//! layerfs-project#106): their records, launch and host operations, exec,
//! the daemon channel (`daemon`), and the Workspaces a Sandbox's daemon
//! serves (`workspace`). Sandbox modules never import `workspace` (rule 4;
//! a test checks it): a host operation's local runner is passed in
//! (`route::Local`).
//!
//! Actual ordinary Sandbox lifecycle and its Workspaces, with exact logical and kernel readiness.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

#[doc(hidden)]
pub use daemon::internal;
pub mod daemon;
pub mod workspace;
pub use daemon::call::{SdkError, SdkResult};
pub use daemon::failure::{OperationCause, OperationFailure};
pub use daemon::workspace::{
    BoundWorkspace, MountFailure, MountedWorkspace, Unmounted, WorkspaceApi,
};
mod close;
mod create;
#[doc(hidden)]
pub mod deploy;
#[doc(hidden)]
pub mod direct;
#[doc(hidden)]
pub mod dispatch;
#[doc(hidden)]
#[cfg(unix)]
pub mod docker;
#[doc(hidden)]
pub mod exec;
#[doc(hidden)]
pub mod family;
#[doc(hidden)]
pub mod files;
/// The forced close of a `native` Sandbox, from its own records alone.
pub mod force;
mod get;
#[doc(hidden)]
pub mod host;
#[doc(hidden)]
#[cfg(unix)]
pub mod image_artifact;
#[doc(hidden)]
#[cfg(unix)]
pub mod image_load;
#[doc(hidden)]
pub mod image_tar;
#[doc(hidden)]
#[cfg(unix)]
pub mod images;
#[doc(hidden)]
pub mod launch;
#[doc(hidden)]
pub mod lifecycle;
mod list;
#[doc(hidden)]
pub mod output;
#[doc(hidden)]
pub mod record;
#[doc(hidden)]
pub mod registry;
#[doc(hidden)]
pub mod reopen;
#[doc(hidden)]
pub mod route;
#[doc(hidden)]
pub mod runtime;
#[doc(hidden)]
pub mod unmount_guard;
#[cfg(unix)]
pub use launch::SandboxApi;
#[cfg(unix)]
pub use launch::{ProviderCreate, ProviderFailure, ProviderSandbox};
#[cfg(unix)]
pub use launch::{SandboxCause, SandboxPhase};
