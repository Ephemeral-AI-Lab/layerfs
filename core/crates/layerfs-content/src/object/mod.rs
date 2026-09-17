//! Canonical object identity, framing, authenticated read and finalized output.
//!
//! Entry module: declarations and re-exports only.

pub mod codec;
pub mod inode_leaf;

mod access;
mod id;
mod output;
mod predecessor;

pub use crate::policy::{MAX_CANONICAL_OBJECT_BYTES, MAX_OBJECT_FIELD_BYTES};
pub use access::AuthenticatedObjects;
pub use codec::{
    canonical_len, decode_bytes_object, encode_bytes_object, encode_bytes_object_to, BYTES_KIND,
    HEADER_LEN, MAX_PAYLOAD_BYTES, OBJECT_MAGIC,
};
pub use id::{authenticate, ObjectId, DIGEST_BYTES, OBJECT_DOMAIN};
pub use inode_leaf::{
    decode_inode_value, decode_pooled_body, decode_pooled_value, encode_inode_value,
    encode_pooled_value, pooled_body, pooled_physical_length, rebuild_leaf, InodeKind, InodeLeaf,
    InodeLeafRow, InodeValue, PooledRow, INODE_VALUE_BYTES, LEAF_ROW_BYTES, MAXIMUM_LEAF_ROWS,
    POOLED_PREFIX_BYTES, POOLED_ROW_BYTES, POOLED_VALUE_CANONICAL_BYTES,
};
pub use output::{DiscardingConsumer, FinalizedConsumer, FinalizedObject, ObjectRole};
pub use predecessor::{
    AdvisoryPredecessor, AdvisoryPredecessors, PredecessorProvenance, MAXIMUM_ADVISORY_PREDECESSORS,
};
