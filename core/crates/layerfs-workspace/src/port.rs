//! Short read-job boundary; canonical demand belongs outside its owner.
use crate::WorkspaceResult;
use layerfs_overlay::{BaseSource, Dentry, Inode, NameWindow};
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
}
impl OverlayRead for layerfs_overlay::Overlay {
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
