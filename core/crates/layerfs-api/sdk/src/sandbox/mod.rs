//! Actual ordinary Sandbox lifecycle; independent from filesystem admission and drain.
mod api;
mod lifecycle;
mod types;
pub use api::{ManagedSandbox, SandboxApi};
pub use types::{SandboxCause, SandboxCreate, SandboxFailure, SandboxPhase};
