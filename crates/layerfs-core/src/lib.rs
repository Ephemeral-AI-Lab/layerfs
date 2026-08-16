//! LayerFS logical canonical model and algorithms.

#![forbid(unsafe_code)]

pub const COMPONENT: &str = "layerfs-core";

mod error;
pub mod format;
pub mod identity;
pub mod limits;
pub mod object;

pub use error::{CoreError, CoreResult};
pub use format::{CanonicalName, CanonicalPath};
pub use identity::ObjectId;
pub use object::{
    decode_object, decode_object_from, encode_object, encode_object_to, DirectoryEntry, Object,
    ObjectKind, ObjectReference,
};
