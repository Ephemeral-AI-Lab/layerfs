//! Portable mode and mtime values and their checked grammar.
//!
//! These are the only typed attribute values. A file or symlink carries `0o777`
//! of permission bits, a directory additionally carries the sticky bit (the
//! reference's `0o1777` mask), a symlink's mode is exactly `0o777`, and the
//! fractional second is below one billion. No uid, gid or atime semantics are
//! added, and no platform-specific value is interpreted.

use crate::error::{ContentError, ContentResult};
use crate::object::inode_leaf::InodeKind;

/// Largest representable fractional second.
pub const MAXIMUM_NANOSECONDS: u32 = 999_999_999;

/// Checked portable metadata fields.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PortableMetadata {
    /// Permission mode bits.
    pub mode: u32,
    /// Seconds since the Unix epoch.
    pub mtime_seconds: i64,
    /// Fractional second.
    pub mtime_nanoseconds: u32,
}

impl PortableMetadata {
    /// Checks the mode and mtime against the inode kind they belong to.
    pub fn validate(&self, kind: InodeKind) -> ContentResult<()> {
        let mask = match kind {
            InodeKind::RegularFile | InodeKind::Symlink => 0o777,
            InodeKind::Directory => 0o1777,
        };
        if self.mode & !mask != 0
            || (kind == InodeKind::Symlink && self.mode != 0o777)
            || self.mtime_nanoseconds > MAXIMUM_NANOSECONDS
        {
            return Err(ContentError::InvalidRecord("portable metadata"));
        }
        Ok(())
    }

    /// Four big-endian mode bytes.
    pub fn mode_bytes(&self, kind: InodeKind) -> ContentResult<[u8; 4]> {
        self.validate(kind)?;
        Ok(self.mode.to_be_bytes())
    }

    /// Twelve big-endian mtime bytes: seconds then nanoseconds.
    pub fn mtime_bytes(&self) -> ContentResult<[u8; 12]> {
        if self.mtime_nanoseconds > MAXIMUM_NANOSECONDS {
            return Err(ContentError::InvalidRecord("mtime"));
        }
        let mut bytes = [0_u8; 12];
        bytes[..8].copy_from_slice(&self.mtime_seconds.to_be_bytes());
        bytes[8..].copy_from_slice(&self.mtime_nanoseconds.to_be_bytes());
        Ok(bytes)
    }

    /// Decodes the stored mode value.
    pub fn decode_mode(bytes: &[u8], kind: InodeKind) -> ContentResult<u32> {
        let value = u32::from_be_bytes(
            bytes
                .try_into()
                .map_err(|_| ContentError::InvalidRecord("mode width"))?,
        );
        Self {
            mode: value,
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
        }
        .validate(kind)?;
        Ok(value)
    }

    /// Decodes the stored mtime value.
    pub fn decode_mtime(bytes: &[u8]) -> ContentResult<(i64, u32)> {
        if bytes.len() != 12 {
            return Err(ContentError::InvalidRecord("mtime width"));
        }
        let seconds = i64::from_be_bytes(
            bytes[..8]
                .try_into()
                .map_err(|_| ContentError::InvalidRecord("mtime width"))?,
        );
        let nanoseconds = u32::from_be_bytes(
            bytes[8..]
                .try_into()
                .map_err(|_| ContentError::InvalidRecord("mtime width"))?,
        );
        if nanoseconds > MAXIMUM_NANOSECONDS {
            return Err(ContentError::InvalidRecord("mtime"));
        }
        Ok((seconds, nanoseconds))
    }
}
