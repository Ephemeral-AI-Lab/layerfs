//! Launching a Sandbox: its local process and the provider facade that
//! starts its daemon and connects to it.
mod connection;
pub(crate) mod create;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) mod local;
mod provider;
pub use create::SandboxApi;
pub use create::{connect_address, SandboxCause, SandboxPhase};
pub use provider::{ProviderCreate, ProviderFailure, ProviderSandbox};
