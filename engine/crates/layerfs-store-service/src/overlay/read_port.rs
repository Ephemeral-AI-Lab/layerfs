//! Workspace read and namespace-job composition through real short owner jobs,
//! outside provider IO.
use crate::{OwnerClient, OwnerError};
use layerfs_overlay::{BaseSource, Cell, DirectoryEntry, DirectoryEntryWindow, Inode, LocalRead};
use layerfs_workspace::{
    JobOutcome, NamespaceJob, OverlayJobs, OverlayRead, WorkspaceError, WorkspaceResult,
};
fn error(error: OwnerError) -> WorkspaceError {
    WorkspaceError::Service(Box::new(error))
}
impl OwnerClient {
    pub fn read_job<T>(
        &self,
        command: impl FnOnce(&crate::OwnerClient, layerfs_workspace::Route) -> Result<T, crate::OwnerError>
            + Send,
    ) -> Result<T, WorkspaceError> {
        command(self, self.bound_route().map_err(error)?).map_err(error)
    }
}
impl OverlayRead for OwnerClient {
    fn inode(&self, source: BaseSource, serial: u64) -> WorkspaceResult<Option<Inode>> {
        let done = self.read_job(|job_client: &crate::OwnerClient, route| {
            job_client.source_inode(route, source, serial)
        })?;
        Ok(done)
    }
    fn directory_entry(
        &self,
        source: BaseSource,
        parent: u64,
        name: &[u8],
    ) -> WorkspaceResult<Option<DirectoryEntry>> {
        if name.is_empty() || name.len() > 255 {
            return Err(layerfs_overlay::OverlayError::Invalid("source name window").into());
        }
        let done = self.read_job(|job_client: &crate::OwnerClient, route| {
            job_client.source_directory_entry(route, source, parent, name.to_vec())
        })?;
        Ok(done)
    }
    fn directory_entries(
        &self,
        source: BaseSource,
        parent: u64,
        after: Option<&[u8]>,
    ) -> WorkspaceResult<DirectoryEntryWindow> {
        if after.is_some_and(|key| key.len() > 255) {
            return Err(layerfs_overlay::OverlayError::Invalid("source name cursor").into());
        }
        let done = self.read_job(|job_client: &crate::OwnerClient, route| {
            job_client.source_directory_entries(route, source, parent, after.map(<[u8]>::to_vec))
        })?;
        Ok(done)
    }
    fn read(
        &self,
        source: BaseSource,
        serial: u64,
        offset: u64,
        length: u32,
    ) -> WorkspaceResult<Option<LocalRead>> {
        if length as usize > layerfs_overlay::READ_WINDOW {
            return Err(layerfs_overlay::OverlayError::Invalid("read window").into());
        }
        let done = self.read_job(|job_client: &crate::OwnerClient, route| {
            job_client.source_read(route, source, serial, offset, length)
        })?;
        Ok(done)
    }
    fn cell(
        &self,
        source: BaseSource,
        serial: u64,
        generation: u64,
        offset: u64,
    ) -> WorkspaceResult<Option<Cell>> {
        let done = self.read_job(|job_client: &crate::OwnerClient, route| {
            job_client.source_cell(route, source, serial, generation, offset)
        })?;
        Ok(done)
    }
}
impl OverlayJobs for OwnerClient {
    /// One owner round. An unadmitted job returns its exact refusal and the
    /// original command; an attempted job's error is its exact outcome.
    fn namespace(&self, job: &NamespaceJob) -> WorkspaceResult<JobOutcome> {
        let done = self.read_job(|job_client: &crate::OwnerClient, route| {
            job_client.namespace(route, Box::new(job.clone()))
        })?;
        Ok(done)
    }
}
