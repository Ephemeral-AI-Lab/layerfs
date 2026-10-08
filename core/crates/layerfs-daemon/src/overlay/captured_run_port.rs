//! Original scoped captured-run jobs through the existing fair read class.
//! Constructor-thread port: each call waits for a credit before its one attempt.
use crate::{Command, OwnerClient, Response};
use layerfs_overlay::{CapturedReader, CapturedRunCursor, CapturedRunReply, Inode};
use layerfs_workspace::{OverlayCapturedRuns, WorkspaceError, WorkspaceResult};

impl OverlayCapturedRuns for OwnerClient {
    fn captured_inode(
        &self,
        reader: CapturedReader,
        serial: u64,
    ) -> WorkspaceResult<Option<Inode>> {
        let pending = self
            .submit_waiting(
                Some(reader.capture().route()),
                Command::ReaderInode { reader, serial },
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
            Ok(Response::Inode(value)) => Ok(value.clone()),
            _ => Err(WorkspaceError::Service(Box::new(done))),
        }
    }
    fn captured_run_step(&self, cursor: CapturedRunCursor) -> WorkspaceResult<CapturedRunReply> {
        let pending = self
            .submit_waiting(
                Some(cursor.reader().capture().route()),
                Command::CapturedRun(Box::new(cursor)),
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
            // The independently returned bounded window overlaps its original
            // credited completion during this clone; no zero-copy claim.
            Ok(Response::CapturedRun(value)) => Ok(value.as_ref().clone()),
            _ => Err(WorkspaceError::Service(Box::new(done))),
        }
    }
}
