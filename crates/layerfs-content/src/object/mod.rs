pub mod access;
mod canonical;
mod codec;
mod digest;
mod id;
pub mod references;

pub use canonical::{DirectoryEntry, Object, ObjectKind, ObjectReference};
pub use codec::{
    authenticate_identity, decode_bytes_object, decode_object, decode_object_from,
    encode_bytes_object, encode_bytes_object_to, encode_object, encode_object_to,
    validate_bytes_identity, validate_identity, validate_object_from, ObjectSummary, HEADER_LEN,
    MAGIC,
};
pub(crate) use digest::ObjectHashWriter;
pub use digest::{hash_object_bytes, ContentDigestWriter, DIGEST_BYTES};
pub use id::{chunk_id, ObjectId};

pub type ChunkId = ObjectId;
