//! Ordinary runtime operations, separate from filesystem admission and data ownership.
// unsafe only in backend/macos/darwin.rs: macOS process facts come from
// proc_pidinfo, a system call (`ps` is setuid, which Seatbelt refuses)
#![deny(unsafe_code)]
#[cfg(unix)]
pub mod backend;
mod types;
#[cfg(unix)]
pub use backend::{LaunchFailure, Runtime, Sandbox};
pub use types::{
    CommandIdentity, ContainerId, DockerRecord, ErrorDiagnostic, ExecExit, ExecId, ExecInspection,
    ExecRequest, Image, IsolationModel, LaunchPhase, LaunchRequest, LocalRecord, MountDriver,
    NamespaceRecord, PendingRequest, ProviderCapabilities, ProviderLaunchOptions, RuntimeError,
    SandboxId, SandboxInspection, SandboxRecord, SearchPathPolicy, WireFailure,
};
