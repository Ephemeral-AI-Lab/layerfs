//! Launching a Sandbox: its local process and the provider facade that
//! starts its daemon and connects to it.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[doc(hidden)]
pub mod boot;
#[cfg(unix)]
mod connection;
pub(crate) mod create;
/// The forced close of a `native` Sandbox, from its own records alone.
pub mod force_remove;
#[cfg(unix)]
pub(crate) mod launch;
/// The Sandbox side of `machine stop --force`.
#[cfg(feature = "machine")]
pub(crate) mod machine_stop;
#[doc(hidden)]
pub mod mount_intents;
pub mod network;
#[doc(hidden)]
pub mod registry;
pub(crate) mod remove;
#[doc(hidden)]
pub mod serve;
#[cfg(unix)]
#[doc(hidden)]
pub mod startup;
#[cfg(unix)]
pub use launch::{ProviderCreate, ProviderFailure, ProviderSandbox};
#[cfg(unix)]
pub use startup::SandboxApi;
#[cfg(unix)]
pub use startup::{SandboxCause, SandboxPhase};
