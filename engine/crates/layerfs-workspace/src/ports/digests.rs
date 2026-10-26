//! Saved file roots by the digest of their bytes (#364).
use crate::WorkspaceResult;

/// BLAKE3 of all `length` bytes of the file whose content root is `root`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileDigestRow {
    pub root: [u8; 32],
    pub length: u64,
    pub digest: [u8; 32],
}

/// At most [`FILE_DIGEST_WINDOW`] rows or contents per call. A recorded row
/// is a fact about the root's bytes, whether or not that root is published.
pub trait FileDigests {
    fn record_file_digests(&self, rows: &[FileDigestRow]) -> WorkspaceResult<()>;
    /// The root recorded for each (digest, length), in order; None where
    /// no root with those bytes is recorded.
    fn file_roots(&self, contents: &[([u8; 32], u64)]) -> WorkspaceResult<Vec<Option<[u8; 32]>>>;
}

/// Rows per call, the Store's own window.
pub const FILE_DIGEST_WINDOW: usize = 64;
