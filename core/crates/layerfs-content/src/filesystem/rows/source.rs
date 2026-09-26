//! The row contract: ordered cursors, key lookups and the declared totals.
//!
//! A cursor is a pass over one sequence, replayable from its start, and it returns
//! one decoded row. The lookup methods answer one key each; an implementation whose
//! rows are outside the process answers them from its own bounded window rather
//! than from a resident vector.
use crate::error::ContentResult;
use crate::filesystem::identity::InodeScope;
use crate::filesystem::input::FilesystemResources;
use crate::filesystem::input::{DirectoryUpdate, InodeUpdate};
use crate::filesystem::root::FilesystemRootId;
use crate::object::inode_leaf::InodeValue;

/// True for the serials the compact profile can store.
///
/// The root's own identity has a separate check; this one covers the serials a
/// binding or a supplied value names, which is where a serial the leaf grammar
/// would reject enters the operation.
pub(crate) const fn serial_in_range(serial: u64) -> bool {
    serial > 0 && serial <= crate::filesystem::identity::MAXIMUM_INODE_SERIAL
}

/// One ordered pass over a prepared update's directory rows.
pub trait DirectoryRowSource {
    /// The next row in parent order, or `None` at the end of the sequence.
    fn next_row(&mut self) -> ContentResult<Option<DirectoryUpdate>>;
}

/// One ordered pass over a prepared update's typed inode values.
pub trait InodeRowSource {
    /// The next row in serial order, or `None` at the end of the sequence.
    fn next_row(&mut self) -> ContentResult<Option<InodeUpdate>>;
}

/// One ordered pass over the serials the caller's allocator just created.
pub trait SerialRowSource {
    /// The next serial, or `None` at the end of the sequence.
    fn next_row(&mut self) -> ContentResult<Option<u64>>;
}

/// The three ordered row sequences of one prepared update.
///
/// The declaration methods answer from the caller's own counts without reading a
/// row; the cursor methods open a pass; the lookup methods answer one key. An
/// implementation may hold its rows in memory or outside it - the operation only
/// requires that a cursor starts at the beginning of its sequence and that the
/// two lookups answer the same rows the cursors would reach.
pub trait RowSource {
    /// Directory rows this update declares.
    fn directory_rows(&self) -> usize;
    /// Typed inode values this update declares.
    fn inode_rows(&self) -> usize;
    /// Fresh serials this update declares.
    fn new_rows(&self) -> usize;
    /// Opens one ordered pass over the directory rows.
    fn directories(&self) -> ContentResult<Box<dyn DirectoryRowSource + '_>>;
    /// Opens one ordered pass over the typed inode values.
    fn inodes(&self) -> ContentResult<Box<dyn InodeRowSource + '_>>;
    /// Opens one ordered pass over the fresh serials.
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>>;
    /// The final bindings of one changed directory, when it has a row.
    fn directory_for(&self, parent: u64) -> ContentResult<Option<DirectoryUpdate>>;
    /// The typed value the caller supplied for one serial, when it supplied one.
    fn value_for(&self, serial: u64) -> ContentResult<Option<InodeValue>>;
    /// The position of one fresh serial in the fresh sequence, when it is one.
    fn new_position(&self, serial: u64) -> ContentResult<Option<usize>>;
    /// True when the caller's allocator just created one serial.
    fn is_new(&self, serial: u64) -> ContentResult<bool> {
        Ok(self.new_position(serial)?.is_some())
    }
}

/// One prepared update: its rows plus the fields the operation is addressed with.
pub trait PreparedRows: RowSource {
    /// Checked immutable base root; `None` builds a new filesystem.
    fn base(&self) -> Option<FilesystemRootId>;
    /// Allocation scope of every identity in the result.
    fn scope(&self) -> InodeScope;
    /// Root directory serial.
    fn root_serial(&self) -> u64;
    /// Declared resource ceilings.
    fn resources(&self) -> FilesystemResources;
}

/// One ordered pass over a resident directory row slice.
pub struct SliceDirectoryRows<'a> {
    rows: &'a [DirectoryUpdate],
    at: usize,
}

impl<'a> SliceDirectoryRows<'a> {
    /// Opens a pass at the first row of `rows`.
    pub const fn new(rows: &'a [DirectoryUpdate]) -> Self {
        Self { rows, at: 0 }
    }
}

impl DirectoryRowSource for SliceDirectoryRows<'_> {
    fn next_row(&mut self) -> ContentResult<Option<DirectoryUpdate>> {
        let row = self.rows.get(self.at).cloned();
        self.at += usize::from(row.is_some());
        Ok(row)
    }
}

/// One ordered pass over a resident typed-value slice.
pub struct SliceInodeRows<'a> {
    rows: &'a [InodeUpdate],
    at: usize,
}

impl<'a> SliceInodeRows<'a> {
    /// Opens a pass at the first row of `rows`.
    pub const fn new(rows: &'a [InodeUpdate]) -> Self {
        Self { rows, at: 0 }
    }
}

impl InodeRowSource for SliceInodeRows<'_> {
    fn next_row(&mut self) -> ContentResult<Option<InodeUpdate>> {
        let row = self.rows.get(self.at).copied();
        self.at += usize::from(row.is_some());
        Ok(row)
    }
}

/// One ordered pass over a resident fresh-serial slice.
pub struct SliceSerialRows<'a> {
    rows: &'a [u64],
    at: usize,
}

impl<'a> SliceSerialRows<'a> {
    /// Opens a pass at the first row of `rows`.
    pub const fn new(rows: &'a [u64]) -> Self {
        Self { rows, at: 0 }
    }
}

impl SerialRowSource for SliceSerialRows<'_> {
    fn next_row(&mut self) -> ContentResult<Option<u64>> {
        let row = self.rows.get(self.at).copied();
        self.at += usize::from(row.is_some());
        Ok(row)
    }
}
