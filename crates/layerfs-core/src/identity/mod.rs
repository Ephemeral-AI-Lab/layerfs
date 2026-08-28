mod digest;
mod ids;

pub(crate) use digest::ObjectHashWriter;
pub use digest::{chunk_id, hash_object_bytes, ContentDigestWriter, DIGEST_BYTES};
pub use ids::ObjectId;

pub type ChunkId = ObjectId;
