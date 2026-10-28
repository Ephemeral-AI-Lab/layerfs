//! Ordinary runtime operations, separate from filesystem admission and data ownership.
#![forbid(unsafe_code)]

/// The daemon's entry point inside its Sandbox container, as the deployment
/// installs it. The create request, the observed-topology check and the
/// `ns-exec` helper argv all name this one string.
pub const DAEMON_ENTRY: &str = "/usr/local/bin/layerfs-daemon";
#[cfg(unix)]
pub mod backend;
mod types;
pub use types::{
    CommandIdentity, ContainerId, ErrorDiagnostic, ExecId, ExecInspection, PendingRequest,
    RuntimeError, WireFailure,
};
