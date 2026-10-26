//! Captured namespace construction over existing bounded original reader jobs.
use crate::OwnerClient;
use layerfs_overlay::{CapturedReader, DirectoryEntry, Inode};
use layerfs_workspace::{OverlayCapturedNamespace, WorkspaceError, WorkspaceResult};

impl OwnerClient {
    /// This synchronous port serves the constructor thread, never a kernel
    /// receive worker. Each call is one attempt under the Workspace lock.
    /// Failure keeps the original input or attempted cause/backing.
    fn captured_namespace_job<T>(
        &self,
        command: impl FnOnce(&crate::OwnerClient, layerfs_workspace::Route) -> Result<T, crate::OwnerError>
            + Send,
    ) -> WorkspaceResult<T> {
        command(
            self,
            self.bound_route()
                .map_err(|cause| WorkspaceError::Service(Box::new(cause)))?,
        )
        .map_err(|cause| WorkspaceError::Service(Box::new(cause)))
    }
}

impl OverlayCapturedNamespace for OwnerClient {
    fn captured_inode_page(
        &self,
        reader: CapturedReader,
        after: u64,
    ) -> WorkspaceResult<Vec<Inode>> {
        let done = self.captured_namespace_job(|job_client: &crate::OwnerClient, route| {
            job_client.reader_inodes(route, reader, after)
        })?;
        Ok(done)
    }

    fn captured_directory_entry_page(
        &self,
        reader: CapturedReader,
        after: Option<(u64, Vec<u8>)>,
    ) -> WorkspaceResult<Vec<DirectoryEntry>> {
        let done = self.captured_namespace_job(|job_client: &crate::OwnerClient, route| {
            job_client.reader_directory_entries(route, reader, after)
        })?;
        Ok(done)
    }

    fn captured_directory_entries(
        &self,
        reader: CapturedReader,
        parent: u64,
        after: Option<Vec<u8>>,
    ) -> WorkspaceResult<Vec<DirectoryEntry>> {
        let done = self.captured_namespace_job(|job_client: &crate::OwnerClient, route| {
            job_client.reader_parent_directory_entries(route, reader, parent, after)
        })?;
        Ok(done)
    }

    fn captured_directory_entry(
        &self,
        reader: CapturedReader,
        parent: u64,
        name: &[u8],
    ) -> WorkspaceResult<Option<DirectoryEntry>> {
        // The command owns its name, so an unattempted job returns it intact.
        let done = self.captured_namespace_job(|job_client: &crate::OwnerClient, route| {
            job_client.reader_directory_entry(route, reader, parent, name.to_vec())
        })?;
        Ok(done)
    }

    fn captured_symlink(&self, reader: CapturedReader, serial: u64) -> WorkspaceResult<Vec<u8>> {
        let done = self.captured_namespace_job(|job_client: &crate::OwnerClient, route| {
            job_client.reader_symlink(route, reader, serial)
        })?;
        Ok(done)
    }

    fn keep_live_row(&self, reader: CapturedReader, serial: u64) -> WorkspaceResult<bool> {
        self.captured_namespace_job(|job_client: &crate::OwnerClient, route| {
            job_client.call(route, crate::ServiceClass::General, None, move |db| {
                db.keep_live_row(reader, serial)
            })
        })
    }

    fn captured_digests(
        &self,
        reader: CapturedReader,
        serials: &[u64],
    ) -> WorkspaceResult<Vec<Option<layerfs_overlay::CapturedDigest>>> {
        let serials = serials.to_vec();
        self.captured_namespace_job(|job_client: &crate::OwnerClient, route| {
            job_client.call(route, crate::ServiceClass::General, None, move |db| {
                db.captured_digests(reader, &serials)
            })
        })
    }

    fn captured_predecessors(
        &self,
        reader: CapturedReader,
        serials: &[u64],
    ) -> WorkspaceResult<Vec<Option<u64>>> {
        let serials = serials.to_vec();
        self.captured_namespace_job(|job_client: &crate::OwnerClient, route| {
            job_client.call(route, crate::ServiceClass::General, None, move |db| {
                db.captured_predecessors(reader, &serials)
            })
        })
    }
}
