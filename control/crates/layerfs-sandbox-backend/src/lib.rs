//! Ordinary runtime operations, separate from filesystem admission and data ownership.
//!
//! `backend/` holds one folder per Sandbox backend (`host_process`,
//! `namespaces`), named as `LAYERFS_SANDBOX` names them; the docker backend
//! and its Engine client are retired (D-8, layerfs-project#111: the docker
//! machine runs namespaces Sandboxes). `runtime.rs` matches over the backends'
//! providers and Sandboxes; `backend/adapter.rs` is the one match for what is
//! fixed per backend (G2); `types/` holds what they share; `json/`,
//! `helper_spec.rs` and `postgres_url.rs` are the shared formats. The
//! kernels Sandboxes run on live in the
//! `layerfs-platform` crate (host only).
// unsafe only in local/os/linux/seccomp.rs (the namespaces command filter's
// two prctl calls); the macOS process facts (proc_pidinfo) moved with
// LocalProcess to layerfs_bridge::process (layerfs-project#112)
#![deny(unsafe_code)]
#[cfg(unix)]
mod backend;
#[cfg(unix)]
pub use backend::Backend;
#[cfg(unix)]
mod helper_spec;
#[cfg(unix)]
pub mod json;
pub mod platform_policy;
#[cfg(unix)]
mod postgres_url;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod runtime;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod stop;
mod types;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use backend::host_process;
#[cfg(target_os = "linux")]
pub use backend::namespaces::{self, NamespaceProvider};
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod local;
#[cfg(target_os = "macos")]
pub use backend::host_process::HostProcessProvider;
#[cfg(unix)]
pub use helper_spec::HelperSpec;
#[cfg(unix)]
pub use postgres_url::PostgresUrl;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use runtime::{LaunchFailure, Runtime, Sandbox};
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use stop::{Cancelled, ExecSlot, Stop};
pub use types::{
    Bind, CommandIdentity, ContainerId, ExecExit, ExecRequest, Image, IsolationModel, LaunchPhase,
    LaunchRequest, LocalRecord, MountDriver, NamespaceRecord, PendingRequest, ProviderCapabilities,
    ProviderLaunchOptions, RuntimeError, SandboxId, SandboxInspection, SandboxRecord,
    SearchPathPolicy, WireFailure,
};
