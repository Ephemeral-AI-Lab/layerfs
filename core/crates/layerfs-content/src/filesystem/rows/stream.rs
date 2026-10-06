//! Stable directory headers, bounded change cursors and exact changed-name points.
use super::{InodeRowSource, SerialRowSource};
use crate::error::ContentResult;
use crate::filesystem::{FilesystemResources, FilesystemRootId, InodeScope, PathName};
use crate::object::inode_leaf::InodeValue;

/// One changed directory. Zero changes is distinct from no directory header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DirectoryHeader {
    /// Serial of the directory whose final bindings change.
    pub parent: u64,
    /// Exact number of final changed-name rows returned by this parent's cursor.
    pub change_rows: u64,
}

/// Exact presence in the changed-name sequence, not the effective directory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirectoryChangeLookup {
    /// This name has no change row; its base binding is retained.
    Unchanged,
    /// This name has a change row declaring final absence.
    Removed,
    /// This name has a change row declaring this final serial.
    Bound(u64),
}

/// One replayable pass over strictly increasing parent headers.
pub trait DirectoryHeaderSource {
    /// The next header in parent order, or `None` at the exact declared end.
    fn next_row(&mut self) -> ContentResult<Option<DirectoryHeader>>;
}

/// One replayable parent-local pass over sorted unique final changed names.
/// A cursor retains at most its own declared processing window, not all names.
pub trait DirectoryChangeSource {
    /// The next final binding in name order, or `None` at the declared end.
    fn next_row(&mut self) -> ContentResult<Option<(PathName, Option<u64>)>>;
}

/// Stable rows of one operation, with directory bindings supplied separately.
///
/// Every cursor starts at the beginning. Header points and changed-name points
/// must answer exactly the same sealed rows the cursors return. All passes and
/// points retain the same captured owner/root/frontier; current mutable rows,
/// refreshed input and error-driven alternate sources do not satisfy this
/// contract. Ownership and the first exact provider failure remain with the
/// caller until every consumer has ended. This trait requires no legacy
/// DirectoryUpdate or resident directory-change vector.
pub trait StreamedRowSource {
    /// Changed directory headers this operation declares.
    fn directory_rows(&self) -> usize;
    /// Typed inode values this operation declares.
    fn inode_rows(&self) -> usize;
    /// Fresh serials this operation declares.
    fn new_rows(&self) -> usize;
    /// Opens a pass over all headers in strictly increasing parent order.
    fn directory_headers(&self) -> ContentResult<Box<dyn DirectoryHeaderSource + '_>>;
    /// The exact header for this parent, or `None` when it has no update.
    fn directory_header(&self, parent: u64) -> ContentResult<Option<DirectoryHeader>>;
    /// Opens the exact declared sequence of an existing directory header.
    fn directory_changes(&self, parent: u64) -> ContentResult<Box<dyn DirectoryChangeSource + '_>>;
    /// Exact presence and final binding in this parent's changed-name sequence.
    /// A name outside that sequence, including an absent header, is Unchanged.
    fn directory_change(
        &self,
        parent: u64,
        name: &PathName,
    ) -> ContentResult<DirectoryChangeLookup>;
    /// Opens an ordered pass over the typed inode values.
    fn inodes(&self) -> ContentResult<Box<dyn InodeRowSource + '_>>;
    /// Opens an ordered pass over the fresh serials.
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>>;
    /// The exact typed value supplied for this serial, when one was supplied.
    fn value_for(&self, serial: u64) -> ContentResult<Option<InodeValue>>;
    /// This serial's exact position in the fresh sequence, when it is present.
    fn new_position(&self, serial: u64) -> ContentResult<Option<usize>>;
    /// True when the caller's allocator just created this serial.
    fn is_new(&self, serial: u64) -> ContentResult<bool> {
        Ok(self.new_position(serial)?.is_some())
    }
}

/// Fixed filesystem context over one stable streamed row source.
pub trait PreparedDirectoryStreams: StreamedRowSource {
    /// Immutable base root; `None` builds a new filesystem.
    fn base(&self) -> Option<FilesystemRootId>;
    /// Allocation scope of every identity in the result.
    fn scope(&self) -> InodeScope;
    /// Root directory serial.
    fn root_serial(&self) -> u64;
    /// Declared resource ceilings for the existing operation driver.
    fn resources(&self) -> FilesystemResources;
}
