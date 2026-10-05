//! Owning authenticated length facts.
use crate::WorkspaceResult;
/// Owning authorized Store metadata, distinct from a payload-integrity assertion.
/// This port retains exact errors and serves one bounded immutable file fact.
pub trait FileLengths {
    fn file_length(&self, id: layerfs_content::ObjectId) -> WorkspaceResult<u64>;
}
