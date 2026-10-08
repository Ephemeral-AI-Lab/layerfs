//! Captured namespace construction over existing bounded original reader jobs.
use crate::{Command, OwnerClient, Response};
use layerfs_overlay::{CapturedReader, DirectoryEntry, Inode};
use layerfs_workspace::{OverlayCapturedNamespace, WorkspaceError, WorkspaceResult};

impl OwnerClient {
    /// This synchronous port serves the constructor thread, never a native
    /// dispatch/service worker. It waits for a credit before its one attempt.
    /// Failure keeps the original command/completion.
    fn captured_namespace_job(
        &self,
        reader: CapturedReader,
        command: Command,
    ) -> WorkspaceResult<crate::Completion> {
        self.submit_waiting(Some(reader.capture().route()), command)
            .map_err(|(cause, command)| {
                WorkspaceError::Service(Box::new(crate::OwnerError::Unattempted {
                    cause: Box::new(cause),
                    command: Box::new(command),
                }))
            })?
            .wait()
            .map_err(|original| WorkspaceError::Service(Box::new(original)))
    }
}

impl OverlayCapturedNamespace for OwnerClient {
    fn captured_inode_page(
        &self,
        reader: CapturedReader,
        after: u64,
    ) -> WorkspaceResult<Vec<Inode>> {
        let done = self.captured_namespace_job(reader, Command::ReaderInodes { reader, after })?;
        match done.result() {
            // The constructor's bounded copy briefly overlaps the credited
            // original completion. This is not a zero-copy service interface.
            Ok(Response::Inodes(rows)) => Ok(rows.clone()),
            _ => Err(WorkspaceError::Service(Box::new(done))),
        }
    }

    fn captured_directory_entry_page(
        &self,
        reader: CapturedReader,
        after: Option<(u64, Vec<u8>)>,
    ) -> WorkspaceResult<Vec<DirectoryEntry>> {
        let done =
            self.captured_namespace_job(reader, Command::ReaderDirectoryEntries { reader, after })?;
        match done.result() {
            // Full names stay in bounded row values; no truncated identity key.
            Ok(Response::DirectoryEntries(rows)) => Ok(rows.clone()),
            _ => Err(WorkspaceError::Service(Box::new(done))),
        }
    }

    fn captured_directory_entries(
        &self,
        reader: CapturedReader,
        parent: u64,
        after: Option<Vec<u8>>,
    ) -> WorkspaceResult<Vec<DirectoryEntry>> {
        let done = self.captured_namespace_job(
            reader,
            Command::ReaderParentDirectoryEntries {
                reader,
                parent,
                after,
            },
        )?;
        match done.result() {
            Ok(Response::DirectoryEntries(rows)) => Ok(rows.clone()),
            _ => Err(WorkspaceError::Service(Box::new(done))),
        }
    }

    fn captured_directory_entry(
        &self,
        reader: CapturedReader,
        parent: u64,
        name: &[u8],
    ) -> WorkspaceResult<Option<DirectoryEntry>> {
        // The command owns its name, so an unattempted job returns it intact.
        let done = self.captured_namespace_job(
            reader,
            Command::ReaderDirectoryEntry {
                reader,
                parent,
                name: name.to_vec(),
            },
        )?;
        match done.result() {
            Ok(Response::DirectoryEntry(row)) => Ok(row.clone()),
            _ => Err(WorkspaceError::Service(Box::new(done))),
        }
    }

    fn captured_symlink(&self, reader: CapturedReader, serial: u64) -> WorkspaceResult<Vec<u8>> {
        let done =
            self.captured_namespace_job(reader, Command::ReaderSymlink { reader, serial })?;
        match done.result() {
            Ok(Response::Symlink(target)) => Ok(target.clone()),
            _ => Err(WorkspaceError::Service(Box::new(done))),
        }
    }
}
