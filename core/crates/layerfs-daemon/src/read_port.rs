//! Workspace read composition through real short owner jobs, outside provider IO.
use crate::{Command, OwnerClient, OwnerError, Response};
use layerfs_overlay::{BaseSource, Dentry, Inode, NameWindow};
use layerfs_workspace::{OverlayRead, WorkspaceError, WorkspaceResult};
fn error(error: OwnerError) -> WorkspaceError {
    WorkspaceError::Service(Box::new(error))
}
impl OwnerClient {
    fn read_job(
        &self,
        source: BaseSource,
        command: Command,
    ) -> Result<crate::Completion, WorkspaceError> {
        let pending =
            self.try_submit(Some(source.route()), command)
                .map_err(|(cause, command)| {
                    error(OwnerError::Unattempted {
                        cause: Box::new(cause),
                        command: Box::new(command),
                    })
                })?;
        let completion = pending.wait().map_err(error)?;
        Ok(completion)
    }
}
impl OverlayRead for OwnerClient {
    fn inode(&self, source: BaseSource, serial: u64) -> WorkspaceResult<Option<Inode>> {
        let done = self.read_job(source, Command::SourceInode { source, serial })?;
        match done.result() {
            Ok(Response::Inode(value)) => Ok(value.clone()),
            _ => Err(WorkspaceError::Service(Box::new(done))),
        }
    }
    fn dentry(
        &self,
        source: BaseSource,
        parent: u64,
        name: &[u8],
    ) -> WorkspaceResult<Option<Dentry>> {
        if name.is_empty() || name.len() > 255 {
            return Err(layerfs_overlay::OverlayError::Invalid("source name window").into());
        }
        let done = self.read_job(
            source,
            Command::SourceDentry {
                source,
                parent,
                name: name.to_vec(),
            },
        )?;
        match done.result() {
            Ok(Response::Dentry(value)) => Ok(value.clone()),
            _ => Err(WorkspaceError::Service(Box::new(done))),
        }
    }
    fn names(
        &self,
        source: BaseSource,
        parent: u64,
        after: Option<&[u8]>,
    ) -> WorkspaceResult<NameWindow> {
        if after.is_some_and(|key| key.len() > 255) {
            return Err(layerfs_overlay::OverlayError::Invalid("source name cursor").into());
        }
        let done = self.read_job(
            source,
            Command::SourceNames {
                source,
                parent,
                after: after.map(<[u8]>::to_vec),
            },
        )?;
        match done.result() {
            Ok(Response::Names(value)) => Ok(value.clone()),
            _ => Err(WorkspaceError::Service(Box::new(done))),
        }
    }
}
