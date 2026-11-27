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
pub mod image;
#[doc(hidden)]
pub mod lifecycle;
#[doc(hidden)]
pub mod record;
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

// #828 compat: old public module paths, removed in wave 3.
#[doc(hidden)]
pub mod daemon {
    // #828 compat
    #[doc(hidden)]
    pub use crate::route::daemon::channel::exchange as channel; // #828 compat
    #[doc(hidden)]
    pub use crate::route::daemon::channel::fence; // #828 compat
    #[doc(hidden)]
    pub use crate::route::daemon::channel::internal; // #828 compat
    #[doc(hidden)]
    pub use crate::route::daemon::channel::operator; // #828 compat
    #[doc(hidden)]
    pub use crate::route::daemon::channel::pool; // #828 compat
    #[doc(hidden)]
    pub use crate::route::daemon::refusal as values;
    #[allow(unused_imports)]
    pub use crate::route::daemon::*; // #828 compat
    #[doc(hidden)]
    pub use crate::status::capacity; // #828 compat // #828 compat
}
#[doc(hidden)]
pub use crate::image::cache as image_artifact; // #828 compat
#[doc(hidden)]
pub use crate::image::copy::docker; // #828 compat
#[doc(hidden)]
pub use crate::image::copy::load as image_load; // #828 compat
#[doc(hidden)]
pub use crate::image::copy::lock as image_lock; // #828 compat
#[doc(hidden)]
pub use crate::image::list as images;
#[doc(hidden)]
pub use crate::image::tar as image_tar; // #828 compat
#[doc(hidden)]
pub use crate::lifecycle::force_remove as force; // #828 compat
#[doc(hidden)]
pub use crate::record::discovery as files; // #828 compat
#[doc(hidden)]
pub use crate::route::direct; // #828 compat
#[doc(hidden)]
pub use crate::route::machine as host; // #828 compat
#[doc(hidden)]
pub use crate::route::machine::dispatch; // #828 compat // #828 compat
#[doc(hidden)]
pub mod inspect {
    // #828 compat
    #[doc(hidden)]
    pub use crate::route::inspect as owner; // #828 compat
    #[doc(hidden)]
    pub use crate::route::inspect as route;
    #[doc(hidden)]
    pub use crate::status::overlay; // #828 compat
    #[doc(hidden)]
    pub use crate::status::stats as process; // #828 compat
    #[allow(unused_imports)]
    pub use layerfs_inspect::*; // #828 compat // #828 compat
}
#[doc(hidden)]
pub mod launch {
    // #828 compat
    #[doc(hidden)]
    pub use crate::lifecycle::boot as local;
    #[doc(hidden)]
    pub use crate::lifecycle::startup as create; // #828 compat
    #[allow(unused_imports)]
    pub use crate::lifecycle::*; // #828 compat // #828 compat
}
#[doc(hidden)]
pub use crate::exec::output; // #828 compat
#[doc(hidden)]
pub use crate::lifecycle::mount_intents; // #828 compat
#[doc(hidden)]
pub use crate::lifecycle::registry; // #828 compat
#[doc(hidden)]
pub use crate::lifecycle::serve as lifecycle; // #828 compat
#[doc(hidden)]
pub use crate::record::reopen; // #828 compat
#[doc(hidden)]
pub use crate::record::sweep; // #828 compat
#[doc(hidden)]
pub use crate::record::unmount_guard;
#[doc(hidden)]
pub use crate::route::machine::send as route; // #828 compat
#[doc(hidden)]
pub use crate::status::list; // #828 compat
#[doc(hidden)]
pub use crate::status::run_folder; // #828 compat
#[doc(hidden)]
pub use crate::status::state; // #828 compat // #828 compat
