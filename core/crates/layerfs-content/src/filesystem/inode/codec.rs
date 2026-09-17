//! Compact inline inode pages: typed views and branch context checks.
//!
//! The leaf grammar and the 73-byte value are owned by
//! [`crate::object::inode_leaf`]; this module adds the typed page view and the
//! branch grammar so the same value definition feeds the tree, C2 pooling and
//! reads. A page's recorded byte total is the encoded row width.

use crate::error::{ContentError, ContentResult};
use crate::filesystem::limits::{MAXIMUM_INODE_BRANCH_CHILDREN, MAXIMUM_INODE_LEAF_ROWS};
use crate::filesystem::sorted::format::{
    CompactInodes, Format, PageView, RowView, EMPTY_PAGE_BYTES, INODE_BRANCH_ROLE,
    INODE_BRANCH_ROW_BYTES, INODE_LEAF_ROLE,
};
use crate::object::inode_leaf::{InodeValue, LEAF_ROW_BYTES};
use crate::object::ObjectId;

/// One decoded inode page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InodePage {
    /// `serial -> typed inode value` rows.
    Leaf {
        /// Rows in key order.
        entries: Vec<(u64, InodeValue)>,
    },
    /// Child summaries of the inode table.
    Branch {
        /// Page level; children are one level lower.
        level: u8,
        /// Entries in this subtree.
        subtree_count: u64,
        /// Children in key order.
        children: Vec<(u64, ObjectId)>,
    },
}

impl InodePage {
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

    /// Encoded row bytes of this page's subtree.
    pub fn subtree_bytes(&self) -> u64 {
        match self {
            Self::Leaf { entries } => entries.len() as u64 * LEAF_ROW_BYTES as u64,
            Self::Branch { .. } => self.subtree_count() * LEAF_ROW_BYTES as u64,
        }
    }
}

/// Decodes one authenticated canonical inode page.
pub fn decode_inode_page(canonical: &[u8]) -> ContentResult<InodePage> {
    let wire = <CompactInodes as Format>::decode(canonical)?;
    Ok(match wire.level {
        0 => InodePage::Leaf {
            entries: wire
                .entries
                .into_iter()
                .map(|entry| {
                    Ok((
                        entry.key,
                        entry
                            .value
                            .ok_or(ContentError::InvalidRecord("inode leaf value"))?,
                    ))
                })
                .collect::<ContentResult<Vec<_>>>()?,
        },
        level => InodePage::Branch {
            level,
            subtree_count: wire.count,
            children: wire
                .entries
                .into_iter()
                .map(|entry| {
                    Ok((
                        entry.key,
                        entry
                            .child
                            .ok_or(ContentError::InvalidRecord("inode child"))?,
                    ))
                })
                .collect::<ContentResult<Vec<_>>>()?,
        },
    })
}

/// Encodes one final inode page.
pub fn encode_inode_page(page: &InodePage) -> ContentResult<Vec<u8>> {
    match page {
        InodePage::Leaf { entries } => {
            let rows = entries
                .iter()
                .map(|(serial, value)| RowView {
                    key: serial,
                    child: None,
                    value: Some(value),
                })
                .collect::<Vec<_>>();
            <CompactInodes as Format>::encode(&PageView {
                level: 0,
                count: entries.len() as u64,
                bytes: 0,
                rows: &rows,
            })
        }
        InodePage::Branch {
            level,
            children,
            subtree_count,
        } => {
            let rows = children
                .iter()
                .map(|(serial, child)| RowView {
                    key: serial,
                    child: Some(*child),
                    value: None,
                })
                .collect::<Vec<_>>();
            <CompactInodes as Format>::encode(&PageView {
                level: *level,
                count: *subtree_count,
                bytes: 0,
                rows: &rows,
            })
        }
    }
}

/// Exact canonical bytes one inode page of this shape needs.
pub fn page_bytes(page: &InodePage) -> ContentResult<usize> {
    Ok(match page {
        InodePage::Leaf { entries } => EMPTY_PAGE_BYTES
            .checked_add(entries.len() * LEAF_ROW_BYTES)
            .ok_or(ContentError::LengthOverflow)?,
        InodePage::Branch { children, .. } => EMPTY_PAGE_BYTES
            .checked_add(children.len() * INODE_BRANCH_ROW_BYTES)
            .ok_or(ContentError::LengthOverflow)?,
    })
}

/// True when a page of this shape satisfies the non-root fill rule.
pub fn filled_page(rows: usize, level: u8) -> bool {
    if level == 0 {
        rows as u64 >= crate::filesystem::limits::MINIMUM_INODE_LEAF_ROWS
    } else {
        rows as u64 >= crate::filesystem::limits::MINIMUM_INODE_BRANCH_CHILDREN
    }
}

/// Largest rows one page of `level` may hold.
pub const fn maximum_rows(level: u8) -> u64 {
    if level == 0 {
        MAXIMUM_INODE_LEAF_ROWS
    } else {
        MAXIMUM_INODE_BRANCH_CHILDREN
    }
}

/// Logical role code of an inode page at `level`.
pub const fn role(level: u8) -> u8 {
    if level == 0 {
        INODE_LEAF_ROLE
    } else {
        INODE_BRANCH_ROLE
    }
}
