//! Typed independently owned read windows through the real SQL owner.
use crate::{Command, OwnerClient, Response};
use layerfs_overlay::{CapturedReader, FileRead, LocalRead};
use layerfs_workspace::{OverlayFileRead, WorkspaceError, WorkspaceResult};
impl OverlayFileRead for OwnerClient {
    fn file_read(
        &self,
        read: FileRead,
        offset: u64,
        length: u32,
    ) -> WorkspaceResult<Option<LocalRead>> {
        let done = self.read_job(
            read.source(),
            Command::FileRead {
                read,
                offset,
                length,
            },
        )?;
        match done.result() {
            Ok(Response::Read(value)) => Ok(value.clone()),
            _ => Err(WorkspaceError::Service(Box::new(done))),
        }
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
            .try_submit(
                Some(reader.capture().route()),
                Command::CapturedRead {
                    reader,
                    serial,
                    offset,
                    length,
                },
            )
            .map_err(|(cause, command)| {
                WorkspaceError::Service(Box::new(crate::OwnerError::Unattempted {
                    cause: Box::new(cause),
                    command: Box::new(command),
                }))
            })?;
        let done = pending
            .wait()
            .map_err(|error| WorkspaceError::Service(Box::new(error)))?;
        match done.result() {
            Ok(Response::Read(value)) => Ok(value.clone()),
            _ => Err(WorkspaceError::Service(Box::new(done))),
        }
    }
}
