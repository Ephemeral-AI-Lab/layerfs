//! Launching a Sandbox: its local process and the provider facade that
//! starts its daemon and connects to it.
#[cfg(unix)]
mod connection;
#[cfg(unix)]
#[doc(hidden)]
pub mod create;
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[doc(hidden)]
pub mod local;
#[cfg(unix)]
mod provider;
#[cfg(unix)]
pub use create::SandboxApi;
#[cfg(unix)]
pub use create::{connect_address, SandboxCause, SandboxPhase};
#[cfg(unix)]
pub use provider::{ProviderCreate, ProviderFailure, ProviderSandbox};
