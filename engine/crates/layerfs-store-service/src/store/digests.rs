//! The Store's saved file roots by digest, as the captured construction's port.
use layerfs_history::{FileDigest, HistoryCatalog};
use layerfs_workspace::{FileDigestRow, FileDigests, WorkspaceError, WorkspaceResult};

/// Records and reads `file_digest` rows through the History catalog: each
/// call is one bounded statement window outside the publication lane.
pub struct StoreDigests<'a>(pub &'a dyn HistoryCatalog);

fn service(error: layerfs_history::HistoryError) -> WorkspaceError {
    WorkspaceError::Service(Box::new(error))
}

impl FileDigests for StoreDigests<'_> {
    fn record_file_digests(&self, rows: &[FileDigestRow]) -> WorkspaceResult<()> {
        let rows: Vec<FileDigest> = rows
            .iter()
            .map(|row| FileDigest {
                root: row.root,
                length: row.length,
                digest: row.digest,
            })
            .collect();
        self.0.record_file_digests(&rows).map_err(service)
    }
    fn file_roots(&self, contents: &[([u8; 32], u64)]) -> WorkspaceResult<Vec<Option<[u8; 32]>>> {
        self.0.file_roots(contents).map_err(service)
    }
}
