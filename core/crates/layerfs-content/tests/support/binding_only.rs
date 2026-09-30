//! A real-file provider whose independently selected API exposes scalar names.

use layerfs_content::filesystem::rows::{
    BindingLookup, BindingRowSource, BindingRows, DirectoryHeader, DirectoryHeaderSource,
    DirectoryRowSource, InodeRowSource, RowSource, RowSpool, SerialRowSource,
};
use layerfs_content::filesystem::DirectoryUpdate;
use layerfs_content::object::inode_leaf::InodeValue;
use layerfs_content::{ContentError, ContentResult};

pub struct BindingOnly<'a> {
    spool: &'a RowSpool,
}

impl<'a> BindingOnly<'a> {
    pub fn new(spool: &'a RowSpool) -> Self {
        Self { spool }
    }
}

impl RowSource for BindingOnly<'_> {
    fn directory_rows(&self) -> usize {
        self.spool.directory_rows()
    }
    fn inode_rows(&self) -> usize {
        self.spool.inode_rows()
    }
    fn new_rows(&self) -> usize {
        self.spool.new_rows()
    }
    fn directories(&self) -> ContentResult<Box<dyn DirectoryRowSource + '_>> {
        Err(ContentError::UnsupportedProfile {
            what: "whole-directory compatibility",
        })
    }
    fn directory_for(&self, _: u64) -> ContentResult<Option<DirectoryUpdate>> {
        Err(ContentError::UnsupportedProfile {
            what: "whole-directory compatibility",
        })
    }
    fn legacy_binding_lookup(&self, parent: u64, name: &[u8]) -> ContentResult<BindingLookup> {
        self.spool.legacy_binding_lookup(parent, name)
    }
    fn inodes(&self) -> ContentResult<Box<dyn InodeRowSource + '_>> {
        self.spool.inodes()
    }
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>> {
        self.spool.new_inodes()
    }
    fn value_for(&self, serial: u64) -> ContentResult<Option<InodeValue>> {
        self.spool.value_for(serial)
    }
    fn new_position(&self, serial: u64) -> ContentResult<Option<usize>> {
        self.spool.new_position(serial)
    }
}

impl BindingRows for BindingOnly<'_> {
    fn directory_headers(&self) -> ContentResult<Box<dyn DirectoryHeaderSource + '_>> {
        self.spool.directory_headers()
    }
    fn directory_header(&self, parent: u64) -> ContentResult<Option<DirectoryHeader>> {
        self.spool.directory_header(parent)
    }
    fn bindings(&self, header: &DirectoryHeader) -> ContentResult<Box<dyn BindingRowSource + '_>> {
        self.spool.bindings(header)
    }
    fn binding_for(&self, parent: u64, name: &[u8]) -> ContentResult<BindingLookup> {
        self.spool.binding_for(parent, name)
    }
}
