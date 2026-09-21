//! Bounded immutable Workspace reads with Unix ownership checks.
//! Transport and the Linux FUSE projection are assembled by their own libraries.
#![forbid(unsafe_code)]
mod backing;
mod filesystem;
mod runtime;
mod types;
pub use layerfs_bridge::contract::Code as ServiceCode;
pub use runtime::host::WorkspaceHost;
pub use runtime::lifecycle::MountLease;
pub use runtime::state::Workspace;
pub use types::*;
