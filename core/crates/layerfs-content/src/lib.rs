//! Canonical objects and complete-file construction for LayerFS (C1).
//!
//! C1 owns canonical identity, framing, complete-file construction and bounded
//! logical reads. It opens no database, pack or file: construction emits
//! finalized canonical objects to a caller-supplied bounded consumer, and reads
//! ask a caller-supplied authenticated provider for canonical bytes. It depends
//! on no storage, Workspace, history, mount or runtime type.
//!
//! Every entry point takes a [`layerfs_telemetry::timer::TimingScope`] so that
//! construction-only, read-only and integrated callers can measure the same
//! production functions; recording enabled or disabled changes no product work.
//!
//! See the crate `README.md` for the accepted profile, capacities and the exact
//! commands that exercise them.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod error;
pub mod file;
pub mod object;
pub mod policy;

pub use error::{ContentError, ContentResult};
pub use file::{
    construct_bytes, construct_stream, encode_whole_file_payload, read_all, read_all_bounded,
    read_range, whole_file_payload, ConstructedFile, FileContent,
};
pub use object::{
    AuthenticatedObjects, DiscardingConsumer, FinalizedConsumer, FinalizedObject, ObjectId,
    ObjectRole, DIGEST_BYTES, OBJECT_DOMAIN,
};
pub use policy::{ConstructionCapacities, ConstructionPolicy, Representation};
