use crate::tree::inode::InodeKind;
use crate::{CoreError, CoreResult, ObjectId};

pub const SUPPORTED_BSD_FLAGS: u32 = 0x0000_800f;
pub const ACL_RIGHTS_MASK: u64 = 0x0010_3ffe;
pub const ACL_FLAGS_MASK: u64 = 0x0002_01f0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PortableMetadataV1 {
    pub permission_mode: u32,
    pub mtime_seconds: i64,
    pub mtime_nanoseconds: u32,
}

impl PortableMetadataV1 {
    pub fn validate(self, kind: InodeKind) -> CoreResult<()> {
        let mask = match kind {
            InodeKind::RegularFile => 0o777,
            InodeKind::Directory => 0o1777,
            InodeKind::Symlink => 0o777,
        };
        if self.permission_mode & !mask != 0
            || (kind == InodeKind::Symlink && self.permission_mode != 0o777)
            || self.mtime_nanoseconds > 999_999_999
        {
            return Err(CoreError::InvalidRecord("portable metadata"));
        }
        Ok(())
    }

    pub fn mode_bytes(self, kind: InodeKind) -> CoreResult<[u8; 4]> {
        self.validate(kind)?;
        Ok(self.permission_mode.to_be_bytes())
    }
    pub fn mtime_bytes(self) -> CoreResult<[u8; 12]> {
        if self.mtime_nanoseconds > 999_999_999 {
            return Err(CoreError::InvalidRecord("mtime"));
        }
        let mut bytes = [0; 12];
        bytes[..8].copy_from_slice(&self.mtime_seconds.to_be_bytes());
        bytes[8..].copy_from_slice(&self.mtime_nanoseconds.to_be_bytes());
        Ok(bytes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct MetadataKey {
    pub domain: String,
    pub key: Vec<u8>,
}

impl MetadataKey {
    pub fn new(domain: String, key: Vec<u8>) -> CoreResult<Self> {
        if domain.is_empty()
            || domain.len() > 64
            || domain.as_bytes().contains(&0)
            || key.len() > 255
            || key.contains(&0)
        {
            return Err(CoreError::InvalidRecord("metadata key"));
        }
        let valid = match domain.as_str() {
            "portable" => key == b"mode" || key == b"mtime",
            "apple.xattr" => !key.is_empty() && key.len() <= 127,
            "apple.acl" | "apple.bsd-flags" => key.is_empty(),
            _ => false,
        };
        if !valid {
            return Err(CoreError::InvalidRecord("metadata domain"));
        }
        Ok(Self { domain, key })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataEntryV1 {
    pub key: MetadataKey,
    pub value_file_root: ObjectId,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MetadataCounters {
    pub nodes_read: u64,
    pub nodes_created: u64,
}
