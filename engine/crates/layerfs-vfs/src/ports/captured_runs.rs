//! Stable captured sparse input through one bounded owner unit at a time.
use crate::WorkspaceResult;
use layerfs_overlay::{CapturedReader, CapturedRunCursor, CapturedRunReply, Inode};

/// The caller retains the captured reader and first original failure. Returned
/// continuations consume original stable metadata, not failed-operation replay.
pub trait OverlayCapturedRuns {
    fn captured_inode(&self, reader: CapturedReader, serial: u64)
        -> WorkspaceResult<Option<Inode>>;
    fn captured_run_step(&self, cursor: CapturedRunCursor) -> WorkspaceResult<CapturedRunReply>;
}
impl OverlayCapturedRuns for layerfs_overlay::Overlay {
    fn captured_inode(
        &self,
        reader: CapturedReader,
        serial: u64,
    ) -> WorkspaceResult<Option<Inode>> {
        self.reader_inode(reader, serial).map_err(Into::into)
    }
    fn captured_run_step(&self, cursor: CapturedRunCursor) -> WorkspaceResult<CapturedRunReply> {
        layerfs_overlay::Overlay::captured_run_step(self, cursor).map_err(Into::into)
    }
}
