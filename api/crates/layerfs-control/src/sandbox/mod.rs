//! Actual ordinary Sandbox lifecycle and its Workspaces, with exact logical and kernel readiness.
mod close;
mod create;
pub(crate) mod deploy;
#[doc(hidden)]
pub mod dispatch;
pub(crate) mod exec;
#[doc(hidden)]
pub mod family;
pub(crate) mod files;
mod get;
pub(crate) mod launch;
mod list;
#[doc(hidden)]
pub mod output;
#[doc(hidden)]
pub mod record;
pub(crate) mod recorded;
pub(crate) mod registry;
pub(crate) mod route;
pub(crate) mod runtime;
pub(crate) mod serve;
#[cfg(unix)]
pub use launch::SandboxApi;
#[cfg(unix)]
pub use launch::{connect_address, SandboxCause, SandboxPhase};
#[cfg(unix)]
pub use launch::{ProviderCreate, ProviderFailure, ProviderSandbox};
