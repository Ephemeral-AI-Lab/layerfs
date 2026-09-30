//! Observes actual calls to the retained directory sequence and point contract.

use layerfs_content::filesystem::rows::{
    BindingLookup, DirectoryRowSource, InodeRowSource, PreparedRows, RowSource, SerialRowSource,
};
use layerfs_content::filesystem::{
    DirectoryUpdate, FilesystemInput, FilesystemResources, FilesystemRootId, InodeScope,
};
use layerfs_content::object::inode_leaf::InodeValue;
use layerfs_content::ContentResult;
use std::cell::Cell;

pub struct LegacyObserver<'a> {
    input: &'a FilesystemInput<'a>,
    pub sequence_opens: Cell<usize>,
    pub point_calls: Cell<usize>,
    pub binding_queries: Cell<usize>,
}

impl<'a> LegacyObserver<'a> {
    pub fn new(input: &'a FilesystemInput<'a>) -> Self {
        Self {
            input,
            sequence_opens: Cell::new(0),
            point_calls: Cell::new(0),
            binding_queries: Cell::new(0),
        }
    }
}

impl RowSource for LegacyObserver<'_> {
    fn directory_rows(&self) -> usize {
        self.input.directory_rows()
    }
    fn inode_rows(&self) -> usize {
        self.input.inode_rows()
    }
    fn new_rows(&self) -> usize {
        self.input.new_rows()
    }
    fn directories(&self) -> ContentResult<Box<dyn DirectoryRowSource + '_>> {
        self.sequence_opens.set(self.sequence_opens.get() + 1);
        self.input.directories()
    }
    fn directory_for(&self, parent: u64) -> ContentResult<Option<DirectoryUpdate>> {
        self.point_calls.set(self.point_calls.get() + 1);
        self.input.directory_for(parent)
    }
    fn legacy_binding_lookup(&self, parent: u64, name: &[u8]) -> ContentResult<BindingLookup> {
        self.binding_queries.set(self.binding_queries.get() + 1);
        self.input.legacy_binding_lookup(parent, name)
    }
    fn inodes(&self) -> ContentResult<Box<dyn InodeRowSource + '_>> {
        self.input.inodes()
    }
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>> {
        self.input.new_inodes()
    }
    fn value_for(&self, serial: u64) -> ContentResult<Option<InodeValue>> {
        self.input.value_for(serial)
    }
    fn new_position(&self, serial: u64) -> ContentResult<Option<usize>> {
        self.input.new_position(serial)
    }
}

impl PreparedRows for LegacyObserver<'_> {
    fn base(&self) -> Option<FilesystemRootId> {
        self.input.base
    }
    fn scope(&self) -> InodeScope {
        self.input.scope
    }
    fn root_serial(&self) -> u64 {
        self.input.root_serial
    }
    fn resources(&self) -> FilesystemResources {
        self.input.resources
    }
}
