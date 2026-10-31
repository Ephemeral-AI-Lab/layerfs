//! Ordinary runtime operations, separate from filesystem admission and data ownership.
//!
//! `platform/` holds one folder per Sandbox backend (`docker`, `native`,
//! `namespaces`), named as `LAYERFS_SANDBOX` names them; it means the backend
//! here, while `@layerfs/runtime`'s `platform/` means placement (#527 D7).
//! `runtime.rs` is the one `match` over the backends; `types/` holds what they
//! share; `json/` and `helper_spec.rs` are the shared formats.
// unsafe only in platform/native/macos/darwin.rs: macOS process facts come
// from proc_pidinfo, a system call (`ps` is setuid, which Seatbelt refuses)
#![deny(unsafe_code)]
#[cfg(unix)]
mod helper_spec;
#[cfg(unix)]
mod json;
#[cfg(unix)]
mod platform;
#[cfg(unix)]
mod runtime;
mod types;
#[cfg(unix)]
pub use helper_spec::HelperSpec;
#[cfg(unix)]
pub use platform::docker::{self, Docker, DockerProvider};
#[cfg(target_os = "linux")]
pub use platform::namespaces::{self, NamespaceProvider};
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use platform::native::{self, NativeLinuxProvider, NativeMacosProvider};
#[cfg(unix)]
pub use runtime::{LaunchFailure, Runtime, Sandbox};
pub use types::{
    CommandIdentity, ContainerId, DockerRecord, ErrorDiagnostic, ExecExit, ExecId, ExecInspection,
    ExecRequest, Image, IsolationModel, LaunchPhase, LaunchRequest, LocalRecord, MountDriver,
    NamespaceRecord, PendingRequest, ProviderCapabilities, ProviderLaunchOptions, RuntimeError,
    SandboxId, SandboxInspection, SandboxRecord, SearchPathPolicy, WireFailure,
};
