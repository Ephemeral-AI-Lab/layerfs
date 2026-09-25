//! Bounded Workspace reads and file edits, projection coherence, and stage/Commit operations.
//! Unix ownership checks and explicit private payload/metadata resource ownership.
//! Transport and the Linux FUSE projection are assembled by their own libraries.
#![forbid(unsafe_code)]
pub mod backing;
mod commit;
mod commit_types;
pub mod filesystem;
mod overlay;
mod runtime;
mod types;
pub use backing::payload::OwnedPayload;
/// The page-store seam an external test drives to check one file's extent
/// sequence without a mount or a service. Product callers use the Workspace.
pub mod sequence {
    pub use crate::backing::{
        budget::Budget,
        directory::Directory,
        metadata::{Arena, MetadataHost, RootOwner},
        metadata_pages::{self, PageRef, MAX_EXTENT},
        metadata_pieces::{self, Cursor, PieceStore, Replacement, Splice},
        payload::PayloadHost,
        segments::Window,
    };
}
pub use backing::reader::PayloadReader;
pub use commit_types::*;
pub use layerfs_bridge::contract::Code as ServiceCode;
pub use overlay::pieces::{Piece, PieceKind};
pub use runtime::coherence::{ProjectionMutationPermit, ProjectionReplyPermit};
pub use runtime::host::WorkspaceHost;
pub use runtime::lifecycle::MountLease;
pub use runtime::state::Workspace;
pub use types::*;
