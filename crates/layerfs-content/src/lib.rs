//! LayerFS logical canonical model and algorithms.

#![forbid(unsafe_code)]

pub const COMPONENT: &str = "layerfs-content";

mod error;
pub mod file;
pub mod filesystem;
pub mod limits;
pub mod object;
pub mod tree;

pub use error::{CoreError, CoreResult};
pub use object::{
    authenticate_identity, decode_bytes_object, decode_object, decode_object_from,
    encode_bytes_object, encode_bytes_object_to, encode_object, encode_object_to,
    validate_bytes_identity, validate_identity, validate_object_from, DirectoryEntry, Object,
    ObjectKind, ObjectReference, ObjectSummary,
};
pub use object::{chunk_id, ChunkId, ObjectId};
pub use tree::{CanonicalName, CanonicalPath};
