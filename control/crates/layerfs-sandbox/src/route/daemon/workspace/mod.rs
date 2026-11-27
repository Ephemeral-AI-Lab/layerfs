//! A Sandbox's Workspaces over its daemon channel: mount, commit, status
//! and unmount, with exact logical and kernel readiness.
mod commit;
mod mount;
mod recovery;
mod status;
mod unmount;
pub use commit::CommitReply;
pub use mount::BoundWorkspace;
pub use mount::WorkspaceApi;
pub use mount::{MountFailure, MountedWorkspace};
pub use unmount::Unmounted;
