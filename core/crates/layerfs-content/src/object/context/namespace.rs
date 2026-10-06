//! Direct child context for directory and attribute pages.

use crate::error::{ContentError, ContentResult};
use crate::filesystem::attributes::{decode_attribute_page, AttributePage};
use crate::filesystem::directory::codec::filled_page;
use crate::filesystem::directory::{decode_directory_page, DirectoryPage};
use crate::filesystem::{InodeIdentity, InodeScope};
use crate::object::AuthenticatedObjects;

pub(super) fn directory(
    canonical: &[u8],
    reader: &dyn AuthenticatedObjects,
    inode_scope: InodeScope,
    root_serial: u64,
) -> ContentResult<()> {
    let page = decode_directory_page(canonical)?;
    match page {
        DirectoryPage::Leaf { entries } => {
            for (_, serial) in entries {
                InodeIdentity::new(inode_scope, serial)?;
                if serial == root_serial {
                    return Err(ContentError::InvalidRecord("root directory binding"));
                }
            }
            Ok(())
        }
        DirectoryPage::Branch {
            level,
            children,
            subtree_count,
            subtree_bytes,
        } => {
            let mut count = 0_u64;
            let mut bytes = 0_u64;
            let mut previous = None;
            for (maximum, id) in children {
                let canonical = reader.read_canonical(id)?;
                let child = decode_directory_page(&canonical)?;
                if !filled_page(canonical.len()) {
                    return Err(ContentError::NonCanonicalPagePartition);
                }
                let (first, last) = match &child {
                    DirectoryPage::Leaf { entries } => {
                        for (_, serial) in entries {
                            InodeIdentity::new(inode_scope, *serial)?;
                            if *serial == root_serial {
                                return Err(ContentError::InvalidRecord("root directory binding"));
                            }
                        }
                        (
                            entries.first().map(|row| &row.0),
                            entries.last().map(|row| &row.0),
                        )
                    }
                    DirectoryPage::Branch { children, .. } => (
                        children.first().map(|row| &row.0),
                        children.last().map(|row| &row.0),
                    ),
                };
                if child.level().checked_add(1) != Some(level) || last != Some(&maximum) {
                    return Err(ContentError::InvalidRecord("directory child summary"));
                }
                if first.is_some_and(|first| previous.as_ref().is_some_and(|prior| first <= prior))
                {
                    return Err(ContentError::NonCanonicalOrdering);
                }
                count = count
                    .checked_add(child.subtree_count())
                    .ok_or(ContentError::LengthOverflow)?;
                bytes = bytes
                    .checked_add(child.subtree_bytes())
                    .ok_or(ContentError::LengthOverflow)?;
                previous = Some(maximum);
            }
            if count != subtree_count || bytes != subtree_bytes {
                return Err(ContentError::InvalidRecord("directory subtree summary"));
            }
            Ok(())
        }
    }
}

pub(super) fn attributes(canonical: &[u8], reader: &dyn AuthenticatedObjects) -> ContentResult<()> {
    match decode_attribute_page(canonical)? {
        AttributePage::Leaf { entries, .. } => {
            for entry in entries {
                let canonical = reader.read_canonical(entry.value_root)?;
                let state = crate::file::mapping::decode_file_state(&canonical)?;
                let limit = crate::filesystem::limits::MAXIMUM_ATTRIBUTE_VALUE_BYTES;
                if state.logical_len == 0 || state.logical_len > limit as u64 {
                    return Err(ContentError::InvalidRecord("attribute value length"));
                }
                super::mapping::state(&canonical, reader)?;
            }
            Ok(())
        }
        AttributePage::Branch {
            level,
            children,
            subtree_count,
            subtree_bytes,
        } => {
            let mut count = 0_u64;
            let mut bytes = 0_u64;
            let mut previous = None;
            for (maximum, id) in children {
                let canonical = reader.read_canonical(id)?;
                let child = decode_attribute_page(&canonical)?;
                if !child.filled()? {
                    return Err(ContentError::NonCanonicalPagePartition);
                }
                let (first, last, child_bytes) = match &child {
                    AttributePage::Leaf {
                        entries,
                        subtree_bytes,
                    } => (
                        entries.first().map(|row| &row.key),
                        entries.last().map(|row| &row.key),
                        *subtree_bytes,
                    ),
                    AttributePage::Branch {
                        children,
                        subtree_bytes,
                        ..
                    } => (
                        children.first().map(|row| &row.0),
                        children.last().map(|row| &row.0),
                        *subtree_bytes,
                    ),
                };
                if child.level().checked_add(1) != Some(level) || last != Some(&maximum) {
                    return Err(ContentError::InvalidRecord("attribute child summary"));
                }
                if first.is_some_and(|first| previous.as_ref().is_some_and(|prior| first <= prior))
                {
                    return Err(ContentError::NonCanonicalOrdering);
                }
                count = count
                    .checked_add(child.subtree_count())
                    .ok_or(ContentError::LengthOverflow)?;
                bytes = bytes
                    .checked_add(child_bytes)
                    .ok_or(ContentError::LengthOverflow)?;
                previous = Some(maximum);
            }
            if count != subtree_count || bytes != subtree_bytes {
                return Err(ContentError::InvalidRecord("attribute subtree summary"));
            }
            Ok(())
        }
    }
}
