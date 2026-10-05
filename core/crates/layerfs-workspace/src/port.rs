//! Short read-job boundary; canonical demand belongs outside its owner.
use crate::{JobOutcome, NamespaceJob, WorkspaceResult};
use layerfs_overlay::{BaseSource, Cell, Dentry, Inode, NameWindow};
/// The actual overlay service, independently of kernel/runtime adaptation.
/// Each method is one bounded job; it must not execute provider/content I/O.
pub trait OverlayRead {
    fn inode(&self, source: BaseSource, serial: u64) -> WorkspaceResult<Option<Inode>>;
    fn dentry(
        &self,
        source: BaseSource,
        parent: u64,
        name: &[u8],
    ) -> WorkspaceResult<Option<Dentry>>;
    fn names(
        &self,
        source: BaseSource,
        parent: u64,
        after: Option<&[u8]>,
    ) -> WorkspaceResult<NameWindow>;
    /// One cell written at an exact live generation, such as a local symlink
    /// target at its creation generation.
    fn cell(
        &self,
        source: BaseSource,
        serial: u64,
        generation: u64,
        offset: u64,
    ) -> WorkspaceResult<Option<Cell>>;
}
/// Runs one complete namespace job inside the SQL owner. The job is data: its
/// evaluation and single publication share one owner job, with no provider or
/// content I/O. A returned error after the attempt is the exact outcome.
pub trait OverlayJobs: OverlayRead {
    fn namespace(&self, job: &NamespaceJob) -> WorkspaceResult<JobOutcome>;
}
impl OverlayJobs for layerfs_overlay::Overlay {
    fn namespace(&self, job: &NamespaceJob) -> WorkspaceResult<JobOutcome> {
        job.perform(self)
    }
}
impl OverlayRead for layerfs_overlay::Overlay {
    fn cell(
        &self,
        source: BaseSource,
        serial: u64,
        generation: u64,
        offset: u64,
    ) -> WorkspaceResult<Option<Cell>> {
        self.source_cell(source, serial, generation, offset)
            .map_err(Into::into)
    }
    fn inode(&self, source: BaseSource, serial: u64) -> WorkspaceResult<Option<Inode>> {
        self.source_inode(source, serial).map_err(Into::into)
    }
    fn dentry(
        &self,
        source: BaseSource,
        parent: u64,
        name: &[u8],
    ) -> WorkspaceResult<Option<Dentry>> {
        self.source_dentry(source, parent, name).map_err(Into::into)
    }
    fn names(
        &self,
        source: BaseSource,
        parent: u64,
        after: Option<&[u8]>,
    ) -> WorkspaceResult<NameWindow> {
        self.source_name_window(source, parent, after)
            .map_err(Into::into)
    }
}

/// Owning authorized Store metadata, distinct from a payload-integrity assertion.
/// This port retains exact errors and serves one bounded immutable file fact.
pub trait FileLengths {
    fn file_length(&self, id: layerfs_content::ObjectId) -> WorkspaceResult<u64>;
}
