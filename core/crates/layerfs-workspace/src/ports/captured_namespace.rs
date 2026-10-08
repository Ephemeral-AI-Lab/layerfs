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
    /// At most PAGE_ROWS rows of exactly this parent, strictly after `after`
    /// in binary name order, whiteouts included. Empty ends the sequence.
    fn captured_directory_entries(
        &self,
        reader: CapturedReader,
        parent: u64,
        after: Option<Vec<u8>>,
    ) -> WorkspaceResult<Vec<DirectoryEntry>>;
    /// The exact sealed row for this name, or None when the capture has none.
    fn captured_directory_entry(
        &self,
        reader: CapturedReader,
        parent: u64,
        name: &[u8],
    ) -> WorkspaceResult<Option<DirectoryEntry>>;
    /// The exact captured target of a live symlink in the reader's sealed range.
    fn captured_symlink(&self, reader: CapturedReader, serial: u64) -> WorkspaceResult<Vec<u8>>;
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

    fn captured_directory_entries(
        &self,
        reader: CapturedReader,
        parent: u64,
        after: Option<Vec<u8>>,
    ) -> WorkspaceResult<Vec<DirectoryEntry>> {
        self.reader_parent_directory_entries(reader, parent, after.as_deref())
            .map_err(Into::into)
    }

    fn captured_directory_entry(
        &self,
        reader: CapturedReader,
        parent: u64,
        name: &[u8],
    ) -> WorkspaceResult<Option<DirectoryEntry>> {
        self.reader_directory_entry(reader, parent, name)
            .map_err(Into::into)
    }

    fn captured_symlink(&self, reader: CapturedReader, serial: u64) -> WorkspaceResult<Vec<u8>> {
        self.reader_symlink(reader, serial).map_err(Into::into)
    }
}
