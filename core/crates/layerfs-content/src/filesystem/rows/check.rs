//! Declared totals: the row counts a source promises are the counts it produced.

use super::view::{OperationInput, ResidentInput, StreamedInput};
use super::{PreparedDirectoryStreams, PreparedRows};
use crate::error::{ContentError, ContentResult};
/// Checks the shape of every supplied row before any object is touched.
///
/// The checks and their order are the ones the input always had - declared
/// ceilings, the root serial, each row's own shape, the global ordering of the
/// three sequences and the serial ranges - and the row counts each pass observed
/// must equal the declared totals. A source whose cursor ends early is refused
/// here rather than silently applying fewer rows than it declared.
pub fn check_input(rows: &dyn PreparedRows) -> ContentResult<()> {
    check_operation_input(&ResidentInput::new(rows))
}

/// Checks stable streamed headers, exact per-parent counts/order and points.
pub fn check_streamed_input(rows: &dyn PreparedDirectoryStreams) -> ContentResult<()> {
    check_operation_input(&StreamedInput::new(rows))
}

pub(crate) fn check_operation_input(rows: &dyn OperationInput) -> ContentResult<()> {
    let resources = rows.resources();
    resources.check()?;
    let root_serial = rows.root_serial();
    if root_serial == 0 {
        return Err(ContentError::InvalidRecord("root inode serial"));
    }
    let mut previous = 0_u64;
    let mut seen = 0_usize;
    let mut cursor = rows.directories()?;
    while let Some(update) = cursor.next_row()? {
        update.check_header()?;
        for change in update.changes()? {
            let (name, binding) = change?;
            update.check_point(&name, binding)?;
        }
        if seen > 0 && previous >= update.header.parent {
            return Err(ContentError::NonCanonicalOrdering);
        }
        previous = update.header.parent;
        seen += 1;
    }
    if seen != rows.directory_rows() {
        return Err(ContentError::InvalidRecord("directory row count"));
    }
    let mut previous = 0_u64;
    let mut seen = 0_usize;
    let mut cursor = rows.inodes()?;
    while let Some(update) = cursor.next_row()? {
        if !super::serial_in_range(update.serial) {
            return Err(ContentError::InvalidRecord("inode serial"));
        }
        if seen > 0 && previous >= update.serial {
            return Err(ContentError::NonCanonicalOrdering);
        }
        previous = update.serial;
        seen += 1;
    }
    if seen != rows.inode_rows() {
        return Err(ContentError::InvalidRecord("inode row count"));
    }
    let base = rows.base();
    let mut previous = 0_u64;
    let mut seen = 0_usize;
    let mut cursor = rows.new_inodes()?;
    while let Some(serial) = cursor.next_row()? {
        if serial == 0 || (base.is_some() && serial == root_serial) {
            return Err(ContentError::InvalidRecord("new inode serial"));
        }
        if seen > 0 && previous >= serial {
            return Err(ContentError::NonCanonicalOrdering);
        }
        previous = serial;
        seen += 1;
    }
    if seen != rows.new_rows() {
        return Err(ContentError::InvalidRecord("new inode row count"));
    }
    // A build allocates its root like any other inode: without that declaration
    // the operation would reach the absent base with a placeholder identity and
    // fail later for the wrong reason.
    if base.is_none() && !rows.is_new(root_serial)? {
        return Err(ContentError::InvalidRecord("root inode allocation"));
    }
    Ok(())
}
