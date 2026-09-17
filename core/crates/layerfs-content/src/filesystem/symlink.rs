//! Exact stored symbolic-link target grammar and its readback.
//!
//! A symlink target is opaque bytes: at most 4,096 of them and without NUL. No
//! resolution, normalization or platform interpretation happens here, and a
//! symlink inode's content root is exactly this object.

use crate::error::{ContentError, ContentResult};
use crate::filesystem::limits::MAXIMUM_SYMLINK_TARGET_BYTES;
use crate::object::{codec, FinalizedObject, ObjectId, ObjectRole};

/// Magic of one stored symlink target.
pub const SYMLINK_MAGIC: [u8; 8] = *b"LFS4LNK\0";
/// Grammar version of one stored symlink target.
pub const SYMLINK_VERSION: u16 = 1;
/// Persisted role inside the symlink header.
pub const SYMLINK_ROLE: u8 = 5;

/// One checked symlink target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SymlinkTarget {
    bytes: Vec<u8>,
}

impl SymlinkTarget {
    /// Checks and owns one target.
    pub fn new(bytes: Vec<u8>) -> ContentResult<Self> {
        if bytes.len() > MAXIMUM_SYMLINK_TARGET_BYTES || bytes.contains(&0) {
            return Err(ContentError::InvalidRecord("symlink target"));
        }
        Ok(Self { bytes })
    }

    /// The stored target bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Encodes the canonical target object.
    pub fn encode(&self) -> ContentResult<Vec<u8>> {
        let length = u16::try_from(self.bytes.len()).map_err(|_| ContentError::LengthOverflow)?;
        let mut value = Vec::with_capacity(14 + self.bytes.len());
        value.extend_from_slice(&SYMLINK_MAGIC);
        value.extend_from_slice(&SYMLINK_VERSION.to_be_bytes());
        value.extend_from_slice(&[SYMLINK_ROLE, 0]);
        value.extend_from_slice(&length.to_be_bytes());
        value.extend_from_slice(&self.bytes);
        codec::encode_bytes_object(&value)
    }

    /// Decodes one canonical target object with exact framing checks.
    pub fn decode(canonical: &[u8]) -> ContentResult<Self> {
        let value = codec::decode_bytes_object(canonical)?;
        if value.len() < 14 {
            return Err(ContentError::UnexpectedEof);
        }
        if !value.starts_with(&SYMLINK_MAGIC) {
            return Err(ContentError::UnsupportedFraming);
        }
        let version = u16::from_be_bytes([value[8], value[9]]);
        if version != SYMLINK_VERSION {
            return Err(ContentError::UnsupportedMappingVersion { version });
        }
        if value[10] != SYMLINK_ROLE {
            return Err(ContentError::WrongLogicalRole);
        }
        if value[11] != 0 {
            return Err(ContentError::InvalidRecord("symlink flags"));
        }
        let length = usize::from(u16::from_be_bytes([value[12], value[13]]));
        if length > MAXIMUM_SYMLINK_TARGET_BYTES {
            return Err(ContentError::PathLimitExceeded);
        }
        if value.len() < 14 + length {
            return Err(ContentError::UnexpectedEof);
        }
        if value.len() > 14 + length {
            return Err(ContentError::TrailingBytes);
        }
        Self::new(value[14..].to_vec())
    }

    /// Finalizes the target object for the caller's consumer.
    pub fn finalize(&self) -> ContentResult<FinalizedObject> {
        FinalizedObject::new(ObjectRole::Symlink, self.encode()?)
    }
}

/// Emits one symlink target through the caller's operation boundary.
pub fn emit_symlink(
    objects: &mut crate::filesystem::objects::FilesystemObjects<'_>,
    target: SymlinkTarget,
) -> ContentResult<ObjectId> {
    objects.emit(target.finalize()?)
}
