//! Ordinary runtime operations, separate from filesystem admission and data ownership.
//!
//! `backend/` holds one folder per Sandbox backend (`docker`, `native`,
//! `namespaces`), named as `LAYERFS_SANDBOX` names them. `docker_engine/` is
//! the Docker Engine HTTP client. `runtime.rs` is the one `match` over the
//! backends; `types/` holds what they share; `json/` and `helper_spec.rs` are
//! the shared formats. `machine/` (feature `machine`, host only) makes and
//! starts the kernels Sandboxes run on.
// unsafe only in backend/native/macos/darwin.rs: macOS process facts come
// from proc_pidinfo, a system call (`ps` is setuid, which Seatbelt refuses)
#![deny(unsafe_code)]
#[cfg(unix)]
mod backend;
#[cfg(unix)]
mod docker_engine;
#[cfg(unix)]
mod helper_spec;
#[cfg(unix)]
mod json;
#[cfg(all(unix, feature = "machine"))]
pub mod machine;
#[cfg(unix)]
mod runtime;
mod types;
#[cfg(unix)]
pub use backend::docker::{self, Docker, DockerProvider};
#[cfg(target_os = "linux")]
pub use backend::namespaces::{self, NamespaceProvider};
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use backend::native::{self, NativeLinuxProvider, NativeMacosProvider};
#[cfg(unix)]
pub use helper_spec::HelperSpec;
#[cfg(unix)]
pub use runtime::{LaunchFailure, Runtime, Sandbox};
pub use types::{
    CommandIdentity, ContainerId, DockerRecord, ErrorDiagnostic, ExecExit, ExecId, ExecInspection,
    ExecRequest, Image, IsolationModel, LaunchPhase, LaunchRequest, LocalRecord, MountDriver,
    NamespaceRecord, PendingRequest, ProviderCapabilities, ProviderLaunchOptions, RuntimeError,
    SandboxId, SandboxInspection, SandboxRecord, SearchPathPolicy, WireFailure,
};
