//! Bounded Workspace reads and unmounted Branch-backed local range edits.
//! Unix ownership checks and explicit private payload/metadata resource ownership.
//! Transport and the Linux FUSE projection are assembled by their own libraries.
#![forbid(unsafe_code)]
mod backing;
mod commit;
mod filesystem;
mod overlay;
mod runtime;
mod types;
pub use backing::payload::OwnedPayload;
pub use backing::reader::PayloadReader;
pub use layerfs_bridge::contract::Code as ServiceCode;
pub use runtime::host::WorkspaceHost;
pub use runtime::lifecycle::MountLease;
pub use runtime::state::Workspace;
pub use types::*;
