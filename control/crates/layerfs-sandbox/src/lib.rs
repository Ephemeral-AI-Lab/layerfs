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
pub use daemon::call::{SdkError, SdkResult};
pub use daemon::failure::{OperationCause, OperationFailure};
pub use daemon::workspace::{
    BoundWorkspace, MountFailure, MountedWorkspace, Unmounted, WorkspaceApi,
};
mod create;
#[doc(hidden)]
pub mod deploy;
#[doc(hidden)]
pub mod direct;
#[doc(hidden)]
pub mod dispatch;
#[doc(hidden)]
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
pub mod image_artifact;
#[doc(hidden)]
pub mod image_load;
#[doc(hidden)]
pub mod image_lock;
mod image_sweep;
#[doc(hidden)]
pub mod image_tar;
#[doc(hidden)]
pub mod images;
#[doc(hidden)]
pub mod launch;
#[doc(hidden)]
pub mod lifecycle;
mod list;
#[doc(hidden)]
pub mod mount_intents;
#[doc(hidden)]
pub mod output;
#[doc(hidden)]
pub mod record;
#[doc(hidden)]
pub mod registry;
mod remove;
#[doc(hidden)]
pub mod reopen;
#[doc(hidden)]
pub mod route;
#[doc(hidden)]
pub mod run_folder;
#[doc(hidden)]
pub mod runtime;
/// A Sandbox's state and a Workspace's, from records and daemon words.
pub mod state;
/// What interrupted Sandbox starts and removals left in a run root, removed
/// where nothing can own it (spec v20).
pub mod sweep;
#[doc(hidden)]
pub mod unmount_guard;
#[cfg(unix)]
mod backend;
pub mod inspect;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod spawn;
pub mod status;
#[cfg(unix)]
pub use launch::SandboxApi;
#[cfg(unix)]
pub use launch::{ProviderCreate, ProviderFailure, ProviderSandbox};
#[cfg(unix)]
pub use launch::{SandboxCause, SandboxPhase};

// Ordinary runtime operations, separate from filesystem admission and data ownership.
//
// The crate is grouped by owner:
// - `runtime/`: the backend-neutral surface: `dispatch.rs` matches over the
//   backends' providers and Sandboxes, `cancel.rs` stops one command,
//   `trace.rs` times a traced command's steps, `types/` holds what the
//   backends share.
// - `shared/`: the process lifecycle both backends use (daemon start, its
//   listener, search path, streams, confined reads).
// - `backend/`: one folder per Sandbox backend, named as records store it
//   (`process`, `namespaces`), holding everything only that backend uses,
//   its operating-system facts included (macOS runs only `process`, Linux
//   only `namespaces`); `backend/adapter.rs` is the one match for what is
//   fixed per backend (G2). The docker backend and its Engine client are
//   retired (D-8, layerfs-project#111: the docker machine runs namespaces
//   Sandboxes).
// - `support/`: small formats other crates read (`json/`, `postgres_url.rs`,
//   `removed_platforms.rs`).
//
// The kernels Sandboxes run on live in the `layerfs-platform` crate (host
// only).
// unsafe only in backend/namespaces/seccomp.rs (the namespaces command
// filter's two prctl calls and the helper's clone3 into a cgroup); the macOS
// process facts (proc_pidinfo) moved with LocalProcess to
// layerfs_bridge::process (layerfs-project#112)
#![deny(unsafe_code)]

#[cfg(target_os = "linux")]
pub use backend::namespaces::{self, NamespaceProvider};
#[cfg(unix)]
pub use backend::namespaces::{network, rootfs, HelperSpec, SpecNetwork, RESOLVER_FILES};
#[cfg(target_os = "macos")]
pub use backend::process::{self, ProcessProvider};
#[cfg(unix)]
pub use layerfs_machine::table::backend::Backend;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use runtime::stop::{self as cancel, Cancelled, ExecSlot, Stop};
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use backend::dispatch::{LaunchFailure, Runtime, Sandbox};
pub use runtime::trace;
pub use runtime::{Bind, CommandIdentity, ContainerId, ExecExit, ExecRequest, Image, IsolationModel, LaunchPhase, LaunchRequest, LocalRecord, MountDriver, NamespaceRecord, PendingRequest, ProviderCapabilities, ProviderLaunchOptions, RuntimeError, SandboxId, SandboxInspection, SandboxRecord, SearchPathPolicy, WireFailure, OUTCOME_UNKNOWN};
#[cfg(unix)]
pub use layerfs_home::json;
#[cfg(unix)]
pub use layerfs_machine::table::postgresql_url::{ip, loopback, socket, url_hostaddr, url_hosts};
pub use layerfs_machine::table::removed as removed_platforms;
