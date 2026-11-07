//! Ordinary runtime operations, separate from filesystem admission and data ownership.
//!
//! `backend/` holds one folder per Sandbox backend (`docker`, `host_process`,
//! `namespaces`), named as `LAYERFS_SANDBOX` names them. `docker_engine/` is
//! the Docker Engine HTTP client. `runtime.rs` matches over the backends'
//! providers and Sandboxes; `backend/adapter.rs` is the one match for what is
//! fixed per backend (G2); `types/` holds what they share; `json/`,
//! `helper_spec.rs` and `postgres_url.rs` are the shared formats. The
//! kernels Sandboxes run on live in the
//! `layerfs-machine` crate (host only).
// unsafe only in local/os/macos/darwin.rs: macOS process facts come
// from proc_pidinfo, a system call (`ps` is setuid, which Seatbelt refuses)
#![deny(unsafe_code)]
mod backend;
pub use backend::Backend;
mod docker_engine;
mod helper_spec;
pub mod json;
pub mod platform_policy;
mod postgres_url;
mod runtime;
mod types;
pub use backend::docker::{self, Docker, DockerProvider};
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use backend::host_process;
#[cfg(target_os = "linux")]
pub use backend::namespaces::{self, NamespaceProvider};
pub mod local;
#[cfg(target_os = "macos")]
pub use backend::host_process::HostProcessProvider;
pub use helper_spec::HelperSpec;
pub use postgres_url::PostgresUrl;
pub use runtime::{LaunchFailure, Runtime, Sandbox};
pub use types::{
    Bind, CommandIdentity, ContainerId, DockerRecord, ErrorDiagnostic, ExecExit, ExecId,
    ExecInspection, ExecRequest, Image, IsolationModel, LaunchPhase, LaunchRequest, LocalRecord,
    MountDriver, NamespaceRecord, PendingRequest, ProviderCapabilities, ProviderLaunchOptions,
    RuntimeError, SandboxId, SandboxInspection, SandboxRecord, SearchPathPolicy, WireFailure,
};
