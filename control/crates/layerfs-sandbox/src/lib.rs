//! Sandboxes (#533 step 12; cut from `layerfs-control` by
//! layerfs-project#106): their records, launch and host operations, exec,
//! the daemon channel (`daemon`), and the Workspaces a Sandbox's daemon
//! serves (`workspace`). Sandbox modules never import `workspace` (rule 4;
//! a test checks it): a host operation's local runner is passed in
//! (`route::Local`).
//!
//! Actual ordinary Sandbox lifecycle and its Workspaces, with exact logical and kernel readiness.
#![deny(missing_docs)]
// unsafe only in backend/namespaces/seccomp.rs (see the backend notes below)
#![deny(unsafe_code)]

pub use route::daemon::call::{SdkError, SdkResult};
#[doc(hidden)]
pub use route::daemon::channel::internal;
pub use route::daemon::failure::{OperationCause, OperationFailure};
pub use route::daemon::workspace::{
    BoundWorkspace, MountFailure, MountedWorkspace, Unmounted, WorkspaceApi,
};
#[cfg(unix)]
#[doc(hidden)]
pub mod backend;
#[doc(hidden)]
pub mod deploy;
#[doc(hidden)]
pub mod exec;
#[doc(hidden)]
pub mod family;
/// A machine's Sandbox images: cache, copy, list, prune and tar reader.
pub mod image;
#[doc(hidden)]
pub mod lifecycle;
#[doc(hidden)]
pub mod record;
/// How a Sandbox call reaches where it runs.
pub mod route;
#[doc(hidden)]
pub mod runtime;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod spawn;
pub mod status;
pub mod users;
#[cfg(unix)]
pub use lifecycle::SandboxApi;
#[cfg(unix)]
pub use lifecycle::{ProviderCreate, ProviderFailure, ProviderSandbox};
#[cfg(unix)]
pub use lifecycle::{SandboxCause, SandboxPhase};

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
// layerfs_os::process (layerfs-project#112)

#[cfg(unix)]
pub use backend::capabilities as backend_capabilities;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use backend::dispatch::{LaunchFailure, Runtime, Sandbox};
#[cfg(target_os = "linux")]
pub use backend::namespaces::{self, NamespaceProvider};
#[cfg(unix)]
pub use backend::namespaces::{network, rootfs, HelperSpec, SpecNetwork, RESOLVER_FILES};
#[cfg(target_os = "macos")]
pub use backend::process::{self, ProcessProvider};
#[cfg(unix)]
pub use layerfs_home::json;
#[cfg(unix)]
pub use layerfs_machine::table::backend::Backend;
#[cfg(unix)]
pub use layerfs_machine::table::postgresql_url::{ip, loopback, socket, url_hostaddr, url_hosts};
pub use layerfs_machine::table::removed as removed_platforms;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use runtime::stop::{self as cancel, Cancelled, ExecSlot, Stop};
pub use runtime::trace;
pub use runtime::{
    Bind, CommandIdentity, ContainerId, ExecExit, ExecRequest, Image, IsolationModel, LaunchPhase,
    LaunchRequest, LocalRecord, MountDriver, NamespaceRecord, PendingRequest, ProviderCapabilities,
    ProviderLaunchOptions, RuntimeError, SandboxId, SandboxInspection, SandboxRecord,
    SearchPathPolicy, WireFailure, OUTCOME_UNKNOWN,
};
