//! Typed independently owned read windows through the real SQL owner.
use crate::OwnerClient;
use layerfs_overlay::{CapturedReader, FileRead, LocalRead};
use layerfs_workspace::{OverlayFileRead, WorkspaceError, WorkspaceResult};
impl OverlayFileRead for OwnerClient {
    fn file_read(
        &self,
        read: FileRead,
        offset: u64,
        length: u32,
    ) -> WorkspaceResult<Option<LocalRead>> {
        let done = self.read_job(|job_client: &crate::OwnerClient, route| {
            job_client.file_read(route, read, offset, length)
        })?;
        Ok(done)
    }
    fn captured_read(
        &self,
        reader: CapturedReader,
        serial: u64,
        offset: u64,
        length: u32,
    ) -> WorkspaceResult<Option<LocalRead>> {
        if length as usize > layerfs_overlay::READ_WINDOW {
            return Err(layerfs_overlay::OverlayError::Invalid("read window").into());
        }
        let pending = self
            .captured_read(
                self.bound_route()
                    .map_err(|cause| WorkspaceError::Service(Box::new(cause)))?,
                reader,
                serial,
                offset,
                length,
            )
            .map_err(|cause| WorkspaceError::Service(Box::new(cause)))?;
        let done = pending;
        Ok(done)
    }
}
