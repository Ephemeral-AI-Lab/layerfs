//! Ordinary runtime operations, separate from filesystem admission and data ownership.
//!
//! The crate is grouped by owner:
//! - `runtime/`: the backend-neutral surface: `dispatch.rs` matches over the
//!   backends' providers and Sandboxes, `cancel.rs` stops one command,
//!   `trace.rs` times a traced command's steps, `types/` holds what the
//!   backends share.
//! - `shared/`: the process lifecycle both backends use (daemon start, its
//!   listener, search path, streams, confined reads).
//! - `backend/`: one folder per Sandbox backend, named as records store it
//!   (`process`, `namespaces`), holding everything only that backend uses,
//!   its operating-system facts included (macOS runs only `process`, Linux
//!   only `namespaces`); `backend/adapter.rs` is the one match for what is
//!   fixed per backend (G2). The docker backend and its Engine client are
//!   retired (D-8, layerfs-project#111: the docker machine runs namespaces
//!   Sandboxes).
//! - `support/`: small formats other crates read (`json/`, `postgres_url.rs`,
//!   `removed_platforms.rs`).
//!
//! The kernels Sandboxes run on live in the `layerfs-platform` crate (host
//! only).
// unsafe only in backend/namespaces/seccomp.rs (the namespaces command
// filter's two prctl calls and the helper's clone3 into a cgroup); the macOS
// process facts (proc_pidinfo) moved with LocalProcess to
// layerfs_bridge::process (layerfs-project#112)
#![deny(unsafe_code)]
#[cfg(unix)]
mod backend;
mod runtime;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod shared;
mod support;

#[cfg(target_os = "linux")]
pub use backend::namespaces::{self, NamespaceProvider};
#[cfg(unix)]
pub use backend::namespaces::{network, rootfs, HelperSpec, SpecNetwork, RESOLVER_FILES};
#[cfg(target_os = "macos")]
pub use backend::process::{self, ProcessProvider};
#[cfg(unix)]
pub use backend::Backend;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use runtime::cancel::{self, Cancelled, ExecSlot, Stop};
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use runtime::dispatch::{LaunchFailure, Runtime, Sandbox};
pub use runtime::trace;
pub use runtime::types::{
    Bind, CommandIdentity, ContainerId, ExecExit, ExecRequest, Image, IsolationModel, LaunchPhase,
    LaunchRequest, LocalRecord, MountDriver, NamespaceRecord, PendingRequest, ProviderCapabilities,
    ProviderLaunchOptions, RuntimeError, SandboxId, SandboxInspection, SandboxRecord,
    SearchPathPolicy, WireFailure, OUTCOME_UNKNOWN,
};
#[cfg(unix)]
pub use support::json;
#[cfg(unix)]
pub use support::postgres_url::{ip, loopback, socket, url_hostaddr, url_hosts};
pub use support::removed_platforms;
