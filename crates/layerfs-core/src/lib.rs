//! LayerFS logical canonical model and algorithms.

#![forbid(unsafe_code)]

pub const COMPONENT: &str = "layerfs-core";

pub mod cdc;
pub mod content;
mod error;
pub mod format;
pub mod identity;
pub mod inode;
pub mod limits;
pub mod logical;
pub mod metadata;
pub mod namespace;
pub mod namespace_codec;
pub mod object;

pub use error::{CoreError, CoreResult};
pub use format::{CanonicalName, CanonicalPath};
pub use identity::{chunk_id, ChunkId, ObjectId};
pub use object::{
    authenticate_identity, decode_bytes_object, decode_object, decode_object_from,
    encode_bytes_object, encode_bytes_object_to, encode_object, encode_object_to,
    validate_bytes_identity, validate_identity, validate_object_from, DirectoryEntry, Object,
    ObjectKind, ObjectReference, ObjectSummary,
};
