mod digest;
mod ids;

pub(crate) use digest::ObjectHashWriter;
pub use digest::{hash_object_bytes, ContentDigestWriter, DIGEST_BYTES};
pub use ids::ObjectId;

/// Chunk identities reuse the Phase 1 object domain over raw chunk bytes.
pub type ChunkId = ObjectId;

pub fn chunk_id(bytes: &[u8]) -> ChunkId {
    ObjectId::for_bytes(bytes)
}
