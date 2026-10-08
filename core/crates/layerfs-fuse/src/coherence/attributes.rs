//! One attribute set per kernel inode, always taken from published state.
use crate::operations::Published;
use layerfs_workspace::ViewStat;
use std::{error::Error, fmt};

/// A successful mutation whose published value cannot describe the inode the
/// kernel is updating. The reply is refused rather than composed from request
/// inputs or an older view.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Incoherent {
    /// The publishing job returned no inode for an attribute-bearing reply.
    Unpublished,
    /// The published inode is not the one this reply's node denotes.
    Foreign { published: u64, expected: u64 },
}
impl fmt::Display for Incoherent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "native attribute coherence: {self:?}")
    }
}
impl Error for Incoherent {}

/// The attributes an attribute-bearing mutation reply must carry. Hard-link
/// aliases share one serial, so a change through any name updates the single
/// kernel inode that every alias reads. `expected` is the serial the kernel
/// addressed (SETATTR, LINK); a new entry names its own fresh serial.
pub fn published(value: &Published, expected: Option<u64>) -> Result<&ViewStat, Incoherent> {
    let stat = value.stat.as_ref().ok_or(Incoherent::Unpublished)?;
    match expected {
        Some(expected) if stat.serial != expected => Err(Incoherent::Foreign {
            published: stat.serial,
            expected,
        }),
        _ => Ok(stat),
    }
}
