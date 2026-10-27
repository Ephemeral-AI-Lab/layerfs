//! Owning authenticated length facts.
use crate::WorkspaceResult;
/// Owning authorized Store metadata, distinct from a payload-integrity assertion.
/// This port retains exact errors and serves one bounded immutable file fact.
pub trait FileLengths {
    fn file_length(&self, id: layerfs_content::ObjectId) -> WorkspaceResult<u64>;
    /// The lengths of `ids`, in order. A provider that can answer several in
    /// one demand overrides this; the default asks one at a time.
    fn file_lengths(&self, ids: &[layerfs_content::ObjectId]) -> WorkspaceResult<Vec<u64>> {
        ids.iter().map(|id| self.file_length(*id)).collect()
    }
}
