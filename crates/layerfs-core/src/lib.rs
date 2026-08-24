//! LayerFS logical canonical model and algorithms.

#![forbid(unsafe_code)]

pub const COMPONENT: &str = "layerfs-core";

pub mod canonical_v2;
pub mod cas;
pub mod cdc;
pub mod content;
pub mod cow;
pub mod delta;
mod error;
pub mod format;
pub mod identity;
pub mod inode;
pub mod limits;
pub mod metadata;
pub mod namespace;
pub mod namespace_codec;
pub mod object;
pub mod validation;

pub use error::{CoreError, CoreResult};
pub use format::{CanonicalName, CanonicalPath};
pub use identity::{chunk_id, ChunkId, ObjectId};
pub use object::{
    authenticate_identity, decode_bytes_object, decode_object, decode_object_from,
    encode_bytes_object, encode_bytes_object_to, encode_object, encode_object_to,
    validate_bytes_identity, validate_identity, validate_object_from, DirectoryEntry, Object,
    ObjectKind, ObjectReference, ObjectSummary,
};

#[cfg(test)]
extern crate self as layerfs_core;

#[cfg(test)]
mod legacy_selected_goldens {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/phase4_selected_goldens.rs.inc"
    ));
}
