//! Fixed operation fields over one caller-owned stable streamed row source.
use super::{
    DirectoryChangeLookup, DirectoryChangeSource, DirectoryHeader, DirectoryHeaderSource,
    InodeRowSource, PreparedDirectoryStreams, SerialRowSource, StreamedRowSource,
};
use crate::error::ContentResult;
use crate::filesystem::{FilesystemResources, FilesystemRootId, InodeScope, PathName};
use crate::object::inode_leaf::InodeValue;

/// Additive filesystem input; existing FilesystemInput/PreparedRows are unchanged.
pub struct StreamedFilesystemInput<'a> {
    /// Immutable base root; `None` builds a new filesystem.
    pub base: Option<FilesystemRootId>,
    /// Allocation scope of every identity in the result.
    pub scope: InodeScope,
    /// Root directory serial.
    pub root_serial: u64,
    /// Declared resource ceilings for the existing operation driver.
    pub resources: FilesystemResources,
    /// Stable rows of this captured operation, owned and retained by the caller.
    pub rows: &'a dyn StreamedRowSource,
}
impl StreamedRowSource for StreamedFilesystemInput<'_> {
    fn directory_rows(&self) -> usize {
        self.rows.directory_rows()
    }
    fn inode_rows(&self) -> usize {
        self.rows.inode_rows()
    }
    fn new_rows(&self) -> usize {
        self.rows.new_rows()
    }
    fn directory_headers(&self) -> ContentResult<Box<dyn DirectoryHeaderSource + '_>> {
        self.rows.directory_headers()
    }
    fn directory_header(&self, parent: u64) -> ContentResult<Option<DirectoryHeader>> {
        self.rows.directory_header(parent)
    }
    fn directory_changes(&self, parent: u64) -> ContentResult<Box<dyn DirectoryChangeSource + '_>> {
        self.rows.directory_changes(parent)
    }
    fn directory_change(
        &self,
        parent: u64,
        name: &PathName,
    ) -> ContentResult<DirectoryChangeLookup> {
        self.rows.directory_change(parent, name)
    }
    fn inodes(&self) -> ContentResult<Box<dyn InodeRowSource + '_>> {
        self.rows.inodes()
    }
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>> {
        self.rows.new_inodes()
    }
    fn value_for(&self, serial: u64) -> ContentResult<Option<InodeValue>> {
        self.rows.value_for(serial)
    }
    fn new_position(&self, serial: u64) -> ContentResult<Option<usize>> {
        self.rows.new_position(serial)
    }
}
impl PreparedDirectoryStreams for StreamedFilesystemInput<'_> {
    fn base(&self) -> Option<FilesystemRootId> {
        self.base
    }
    fn scope(&self) -> InodeScope {
        self.scope
    }
    fn root_serial(&self) -> u64 {
        self.root_serial
    }
    fn resources(&self) -> FilesystemResources {
        self.resources
    }
}
