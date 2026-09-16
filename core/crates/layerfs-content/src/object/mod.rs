//! Canonical object identity, framing, authenticated read and finalized output.
//!
//! Entry module: declarations and re-exports only.

pub mod codec;

mod access;
mod id;
mod output;

pub use crate::policy::{MAX_CANONICAL_OBJECT_BYTES, MAX_OBJECT_FIELD_BYTES};
pub use access::AuthenticatedObjects;
pub use codec::{
    canonical_len, decode_bytes_object, encode_bytes_object, encode_bytes_object_to, BYTES_KIND,
    HEADER_LEN, MAX_PAYLOAD_BYTES, OBJECT_MAGIC,
};
pub use id::{authenticate, ObjectId, DIGEST_BYTES, OBJECT_DOMAIN};
pub use output::{DiscardingConsumer, FinalizedConsumer, FinalizedObject, ObjectRole};
