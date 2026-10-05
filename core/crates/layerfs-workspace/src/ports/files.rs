//! Independently owned file/captured read windows, serviced without provider IO.
use crate::WorkspaceResult;
use layerfs_overlay::{CapturedReader, FileRead, LocalRead};
/// Each method is one short owner job. Acquisition/release use explicit lifetime
/// commands; the caller retains the token until all external consumers finish.
pub trait OverlayFileRead {
    fn file_read(
        &self,
        read: FileRead,
        offset: u64,
        length: u32,
    ) -> WorkspaceResult<Option<LocalRead>>;
    fn captured_read(
        &self,
        reader: CapturedReader,
        serial: u64,
        offset: u64,
        length: u32,
    ) -> WorkspaceResult<Option<LocalRead>>;
}
impl OverlayFileRead for layerfs_overlay::Overlay {
    fn file_read(
        &self,
        read: FileRead,
        offset: u64,
        length: u32,
    ) -> WorkspaceResult<Option<LocalRead>> {
        self.read_file(read, offset, length).map_err(Into::into)
    }
    fn captured_read(
        &self,
        reader: CapturedReader,
        serial: u64,
        offset: u64,
        length: u32,
    ) -> WorkspaceResult<Option<LocalRead>> {
        self.read_captured(reader, serial, offset, length)
            .map_err(Into::into)
    }
}
