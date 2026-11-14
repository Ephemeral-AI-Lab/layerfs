//! Ordinary runtime operations, separate from filesystem admission and data ownership.
//!
//! `backend/` holds one folder per Sandbox backend (`host_process`,
//! `namespaces`), named as `LAYERFS_SANDBOX` names them; the docker backend
//! and its Engine client are retired (D-8, layerfs-project#111: the docker
//! machine runs namespaces Sandboxes). `runtime.rs` matches over the backends'
//! providers and Sandboxes; `backend/adapter.rs` is the one match for what is
//! fixed per backend (G2); `types/` holds what they share; `json/`,
//! `helper_spec.rs` and `postgres_url.rs` are the shared formats;
//! `steps.rs` times a traced command's steps. The
//! kernels Sandboxes run on live in the
//! `layerfs-platform` crate (host only).
// unsafe only in local/os/linux/seccomp.rs (the namespaces command filter's
// two prctl calls); the macOS process facts (proc_pidinfo) moved with
// LocalProcess to layerfs_bridge::process (layerfs-project#112)
#![deny(unsafe_code)]
#[cfg(unix)]
mod backend;
#[cfg(unix)]
pub use backend::namespaces::rootfs;
#[cfg(unix)]
pub use backend::Backend;
#[cfg(not(unix))]
pub use types::kind::Backend;
mod helper_spec;
pub mod json;
pub mod platform_policy;
mod postgres_url;
mod runtime;
pub mod steps;
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
pub use helper_spec::HelperSpec;
pub use postgres_url::PostgresUrl;
pub use runtime::{LaunchFailure, Sandbox};
#[cfg(unix)]
pub use runtime::Runtime;
pub use stop::{Cancelled, ExecSlot, Stop};

// The WireFailure twin for the Windows face lives in runtime.rs
// alongside the Sandbox enum (windows_runtime keeps only helpers).
pub use types::{
    Bind, CommandIdentity, ContainerId, ExecExit, ExecRequest, Image, IsolationModel, LaunchPhase,
    LaunchRequest, LocalRecord, MountDriver, NamespaceRecord, PendingRequest, ProviderCapabilities,
    ProviderLaunchOptions, RuntimeError, SandboxId, SandboxInspection, SandboxRecord,
    SearchPathPolicy, WireFailure,
};
