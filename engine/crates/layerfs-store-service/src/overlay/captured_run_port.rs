//! Original scoped captured-run jobs of the read class.
//! Constructor-thread port: each call is one attempt under the Workspace lock.
use crate::OwnerClient;
use layerfs_overlay::{CapturedReader, CapturedRunCursor, CapturedRunReply, Inode};
use layerfs_workspace::{OverlayCapturedRuns, WorkspaceError, WorkspaceResult};

impl OverlayCapturedRuns for OwnerClient {
    fn captured_inode(
        &self,
        reader: CapturedReader,
        serial: u64,
    ) -> WorkspaceResult<Option<Inode>> {
        let pending = self
            .reader_inode(
                self.bound_route()
                    .map_err(|cause| WorkspaceError::Service(Box::new(cause)))?,
                reader,
                serial,
            )
            .map_err(|cause| WorkspaceError::Service(Box::new(cause)))?;
        let done = pending;
        Ok(done)
    }
    fn captured_run_step(&self, cursor: CapturedRunCursor) -> WorkspaceResult<CapturedRunReply> {
        let route = match self.bound_route() {
            Ok(route) => route,
            Err(cause) => {
                return Err(WorkspaceError::Service(Box::new(
                    crate::OwnerError::Unattempted {
                        cause: Box::new(cause),
                        input: crate::overlay::owner::OriginalInput::new(cursor),
                    },
                )))
            }
        };
        let pending = self
            .captured_run(route, Box::new(cursor))
            .map_err(|cause| WorkspaceError::Service(Box::new(cause)))?;
        let done = pending;
        Ok(done)
    }
}
