//! Exact declared sequence counts and scalar directory completion before effects.

use super::{CompatibilityBindingRows, PreparedBindingRows, PreparedRows};
use crate::error::{ContentError, ContentResult};

/// Explicit legacy-profile shape checking through the common scalar checker.
pub fn check_input(rows: &dyn PreparedRows) -> ContentResult<()> {
    let selected = CompatibilityBindingRows::new(rows)?;
    check_binding_input(&selected)
}

/// Checks each scalar directory cursor and every declared sequence exactly.
/// A finished cursor must match its selected issuer/ordinal/count/byte totals.
pub fn check_binding_input(rows: &dyn PreparedBindingRows) -> ContentResult<()> {
    rows.resources().check()?;
    if !super::serial_in_range(rows.root_serial()) {
        return Err(ContentError::InvalidRecord("root inode serial"));
    }
    let mut previous = 0_u64;
    let mut seen = 0_usize;
    let mut headers = rows.directory_headers()?;
    while let Some(header) = headers.next_header()? {
        if seen >= rows.directory_rows() {
            return Err(ContentError::InvalidRecord("directory row count"));
        }
        if !super::serial_in_range(header.parent()) {
            return Err(ContentError::InvalidRecord("directory parent"));
        }
        if seen > 0 && previous >= header.parent() {
            return Err(ContentError::NonCanonicalOrdering);
        }
        let mut cursor = rows.bindings(&header)?;
        let mut count = 0_u32;
        let mut bytes = 0_u64;
        let mut previous_name = None;
        while let Some((name, binding)) = cursor.next_binding()? {
            if count >= header.binding_count() {
                return Err(ContentError::InvalidRecord("directory binding count"));
            }
            if previous_name.as_ref().is_some_and(|before| before >= &name) {
                return Err(ContentError::NonCanonicalOrdering);
            }
            if binding.is_some_and(|serial| !super::serial_in_range(serial)) {
                return Err(ContentError::InvalidRecord("inode serial"));
            }
            bytes = bytes
                .checked_add(10 + name.as_bytes().len() as u64)
                .ok_or(ContentError::LengthOverflow)?;
            if bytes > header.wire_name_bytes() {
                return Err(ContentError::InvalidRecord("directory binding bytes"));
            }
            count += 1;
            previous_name = Some(name);
        }
        if count != header.binding_count()
            || bytes != header.wire_name_bytes()
            || !cursor.finish()?.matches(&header)
        {
            return Err(ContentError::InvalidRecord("directory completion"));
        }
        previous = header.parent();
        seen += 1;
    }
    if seen != rows.directory_rows() {
        return Err(ContentError::InvalidRecord("directory row count"));
    }
    check_scalars(rows)
}

fn check_scalars<T: PreparedRows + ?Sized>(rows: &T) -> ContentResult<()> {
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
        seen = seen.checked_add(1).ok_or(ContentError::LengthOverflow)?;
    }
    if seen != rows.inode_rows() {
        return Err(ContentError::InvalidRecord("inode row count"));
    }
    let base = rows.base();
    let root_serial = rows.root_serial();
    let mut previous = 0_u64;
    let mut seen = 0_usize;
    let mut cursor = rows.new_inodes()?;
    while let Some(serial) = cursor.next_row()? {
        if !super::serial_in_range(serial) || (base.is_some() && serial == root_serial) {
            return Err(ContentError::InvalidRecord("new inode serial"));
        }
        if seen > 0 && previous >= serial {
            return Err(ContentError::NonCanonicalOrdering);
        }
        previous = serial;
        seen = seen.checked_add(1).ok_or(ContentError::LengthOverflow)?;
    }
    if seen != rows.new_rows() {
        return Err(ContentError::InvalidRecord("new inode row count"));
    }
    if base.is_none() && !rows.is_new(root_serial)? {
        return Err(ContentError::InvalidRecord("root inode allocation"));
    }
    Ok(())
}
