//! Replayable namespace pages from an independently retained captured reader.
use crate::{OverlayCapturedRuns, WorkspaceResult};
use layerfs_overlay::{CapturedReader, DirectoryEntry, Inode};

/// Stable local namespace changes, over the same exact root/floor as captured
/// inode points and sparse runs. These are captured changes, not a full merged
/// namespace and not current active rows. The caller retains the reader until
/// every cursor/point consumer and original failure disposition has ended.
///
/// Each page contains at most PAGE_ROWS rows. Inodes are strictly increasing by
/// serial; names by (parent, full binary name), including whiteouts. Resume after
/// the last returned key; an empty page terminates that captured sequence. No
/// query refreshes the capture or releases custody. Replaying a sealed pass is
/// distinct from retrying a failed original operation, which is never allowed.
pub trait OverlayCapturedNamespace: OverlayCapturedRuns {
    fn captured_inode_page(
        &self,
        reader: CapturedReader,
        after: u64,
    ) -> WorkspaceResult<Vec<Inode>>;
    fn captured_directory_entry_page(
        &self,
        reader: CapturedReader,
        after: Option<(u64, Vec<u8>)>,
    ) -> WorkspaceResult<Vec<DirectoryEntry>>;
}

impl OverlayCapturedNamespace for layerfs_overlay::Overlay {
    fn captured_inode_page(
        &self,
        reader: CapturedReader,
        after: u64,
    ) -> WorkspaceResult<Vec<Inode>> {
        self.reader_inodes(reader, after).map_err(Into::into)
    }

    fn captured_directory_entry_page(
        &self,
        reader: CapturedReader,
        after: Option<(u64, Vec<u8>)>,
    ) -> WorkspaceResult<Vec<DirectoryEntry>> {
        self.reader_directory_entries(
            reader,
            after
                .as_ref()
                .map(|(parent, name)| (*parent, name.as_slice())),
        )
        .map_err(Into::into)
    }
}
