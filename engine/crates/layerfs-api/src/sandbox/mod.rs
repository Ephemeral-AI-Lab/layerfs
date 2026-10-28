//! Actual ordinary Sandbox lifecycle and its Workspaces, with exact logical and kernel readiness.
#[cfg(unix)]
mod close;
mod commit;
#[cfg(unix)]
pub(crate) mod create;
#[cfg(unix)]
pub(crate) mod exec;
mod mount;
mod status;
mod unmount;
pub mod values;
pub use commit::CommitReply;
#[cfg(unix)]
pub use create::{connect_address, SandboxCause, SandboxCreate, SandboxFailure, SandboxPhase};
#[cfg(unix)]
pub use create::{ManagedSandbox, SandboxApi};
pub use mount::BoundWorkspace;
pub use mount::WorkspaceApi;
pub use mount::{MountFailure, MountedWorkspace};
pub use unmount::Unmounted;
