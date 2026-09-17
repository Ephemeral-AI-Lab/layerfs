//! Compact directory pages: typed views over the sorted engine's grammar.
//!
//! One directory page is either a leaf of `name -> inode serial` rows or a
//! branch of `name -> child page` summaries. This module owns the typed view and
//! the exact size arithmetic shared with the engine's partition decisions; the
//! engine itself owns mutation.

use crate::error::{ContentError, ContentResult};
use crate::filesystem::path::PathName;
use crate::filesystem::sorted::format::{
    CompactDirectory, Format, PageView, RowView, DIRECTORY_BRANCH_ROLE, DIRECTORY_LEAF_ROLE,
    EMPTY_PAGE_BYTES, NAME_LENGTH_BYTES,
};
use crate::object::ObjectId;

/// One decoded directory page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DirectoryPage {
    /// `name -> inode serial` rows.
    Leaf {
        /// Rows in key order.
        entries: Vec<(PathName, u64)>,
    },
    /// Child summaries of the directory tree.
    Branch {
        /// Page level; children are one level lower.
        level: u8,
        /// Entries in this subtree.
        subtree_count: u64,
        /// Encoded row bytes in this subtree.
        subtree_bytes: u64,
        /// Children in key order.
        children: Vec<(PathName, ObjectId)>,
    },
}

impl DirectoryPage {
    /// Page level; a leaf is level zero.
    pub const fn level(&self) -> u8 {
        match self {
            Self::Leaf { .. } => 0,
            Self::Branch { level, .. } => *level,
        }
    }

    /// Entries in this page's subtree.
    pub fn subtree_count(&self) -> u64 {
        match self {
            Self::Leaf { entries } => entries.len() as u64,
            Self::Branch { subtree_count, .. } => *subtree_count,
        }
    }

    /// Encoded bytes of this page's rows.
    pub fn subtree_bytes(&self) -> u64 {
        match self {
            Self::Leaf { entries } => entries
                .iter()
                .map(|(name, _)| (NAME_LENGTH_BYTES + 8 + name.as_bytes().len()) as u64)
                .sum(),
            Self::Branch { subtree_bytes, .. } => *subtree_bytes,
        }
    }
}

/// Exact canonical bytes one page of these rows needs.
pub fn page_bytes(page: &DirectoryPage) -> ContentResult<usize> {
    let rows = match page {
        DirectoryPage::Leaf { entries } => {
            entries
                .iter()
                .try_fold(EMPTY_PAGE_BYTES, |total, (name, _)| {
                    total
                        .checked_add(NAME_LENGTH_BYTES + 8 + name.as_bytes().len())
                        .ok_or(ContentError::LengthOverflow)
                })?
        }
        DirectoryPage::Branch { children, .. } => {
            children
                .iter()
                .try_fold(EMPTY_PAGE_BYTES, |total, (name, _)| {
                    total
                        .checked_add(NAME_LENGTH_BYTES + 32 + name.as_bytes().len())
                        .ok_or(ContentError::LengthOverflow)
                })?
        }
    };
    Ok(rows)
}

/// Decodes one authenticated canonical directory page.
pub fn decode_directory_page(canonical: &[u8]) -> ContentResult<DirectoryPage> {
    let wire = <CompactDirectory as Format>::decode(canonical)?;
    Ok(match wire.level {
        0 => DirectoryPage::Leaf {
            entries: wire
                .entries
                .into_iter()
                .map(|entry| {
                    Ok((
                        entry.key,
                        entry
                            .value
                            .ok_or(ContentError::InvalidRecord("directory leaf value"))?,
                    ))
                })
                .collect::<ContentResult<Vec<_>>>()?,
        },
        level => DirectoryPage::Branch {
            level,
            subtree_count: wire.count,
            subtree_bytes: wire.bytes,
            children: wire
                .entries
                .into_iter()
                .map(|entry| {
                    Ok((
                        entry.key,
                        entry
                            .child
                            .ok_or(ContentError::InvalidRecord("directory child"))?,
                    ))
                })
                .collect::<ContentResult<Vec<_>>>()?,
        },
    })
}

/// Encodes one final directory page.
pub fn encode_directory_page(page: &DirectoryPage) -> ContentResult<Vec<u8>> {
    match page {
        DirectoryPage::Leaf { entries } => {
            let rows = entries
                .iter()
                .map(|(name, serial)| RowView {
                    key: name,
                    child: None,
                    value: Some(serial),
                    count: 1,
                    bytes: 0,
                })
                .collect::<Vec<_>>();
            <CompactDirectory as Format>::encode(&PageView {
                level: 0,
                rows: &rows,
            })
        }
        DirectoryPage::Branch {
            level, children, ..
        } => {
            let rows = children
                .iter()
                .map(|(name, child)| RowView {
                    key: name,
                    child: Some(*child),
                    value: None,
                    count: 0,
                    bytes: 0,
                })
                .collect::<Vec<_>>();
            <CompactDirectory as Format>::encode(&PageView {
                level: *level,
                rows: &rows,
            })
        }
    }
}

/// True when a page of `bytes` canonical length satisfies the non-root fill rule.
pub fn filled_page(bytes: usize) -> bool {
    <CompactDirectory as Format>::filled(bytes, 0, 0)
}

/// Logical role code of a directory page at `level`.
pub const fn role(level: u8) -> u8 {
    if level == 0 {
        DIRECTORY_LEAF_ROLE
    } else {
        DIRECTORY_BRANCH_ROLE
    }
}

/// Decodes a page and reports its recorded row count and level.
pub fn page_shape(canonical: &[u8]) -> ContentResult<(u8, u64)> {
    let wire = <CompactDirectory as Format>::decode(canonical)?;
    Ok((wire.level, wire.count))
}
