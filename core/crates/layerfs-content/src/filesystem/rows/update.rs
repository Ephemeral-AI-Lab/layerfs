//! One prepared update: the fixed operation fields over any row source.
//!
//! The rows and the fields that address them arrive separately on the wire - the
//! fields in the request, the rows in the body that follows it - so the operation
//! is handed this pair rather than a struct that owns both. A caller whose rows are
//! already resident passes [`FilesystemInput`] instead, which implements the same
//! contract over its own slices.
//!
//! [`FilesystemInput`]: crate::filesystem::input::FilesystemInput

use super::{DirectoryRowSource, InodeRowSource, PreparedRows, RowSource, SerialRowSource};
use crate::error::ContentResult;
use crate::filesystem::identity::InodeScope;
use crate::filesystem::input::{DirectoryUpdate, FilesystemResources};
use crate::filesystem::root::FilesystemRootId;
use crate::object::inode_leaf::InodeValue;

/// One prepared update addressed by its fixed fields and read through `rows`.
pub struct PreparedUpdate<'a> {
    /// Checked immutable base root; `None` builds a new filesystem.
    pub base: Option<FilesystemRootId>,
    /// Allocation scope of every identity in the result.
    pub scope: InodeScope,
    /// Root directory serial.
    pub root_serial: u64,
    /// Declared resource ceilings.
    pub resources: FilesystemResources,
    /// The ordered rows this update applies.
    pub rows: &'a dyn RowSource,
}

impl RowSource for PreparedUpdate<'_> {
    fn directory_rows(&self) -> usize {
        self.rows.directory_rows()
    }
    fn inode_rows(&self) -> usize {
        self.rows.inode_rows()
    }
    fn new_rows(&self) -> usize {
        self.rows.new_rows()
    }
    fn directories(&self) -> ContentResult<Box<dyn DirectoryRowSource + '_>> {
        self.rows.directories()
    }
    fn inodes(&self) -> ContentResult<Box<dyn InodeRowSource + '_>> {
        self.rows.inodes()
    }
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>> {
        self.rows.new_inodes()
    }
    fn directory_for(&self, parent: u64) -> ContentResult<Option<DirectoryUpdate>> {
        self.rows.directory_for(parent)
    }
    fn value_for(&self, serial: u64) -> ContentResult<Option<InodeValue>> {
        self.rows.value_for(serial)
    }
    fn new_position(&self, serial: u64) -> ContentResult<Option<usize>> {
        self.rows.new_position(serial)
    }
}

impl PreparedRows for PreparedUpdate<'_> {
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
