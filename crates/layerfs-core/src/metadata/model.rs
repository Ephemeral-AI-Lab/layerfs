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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum AppleAclTag {
    Allow = 1,
    Deny = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppleAclEntryV1 {
    pub tag: AppleAclTag,
    pub flags: u64,
    pub rights: u64,
    pub qualifier_uuid: [u8; 16],
}

pub fn encode_apple_acl(entries: &[AppleAclEntryV1]) -> CoreResult<Vec<u8>> {
    if entries.is_empty() || entries.len() > 128 {
        return Err(CoreError::InvalidRecord("Apple ACL count"));
    }
    let mut bytes = Vec::with_capacity(12 + 36 * entries.len());
    bytes.extend_from_slice(b"LFS4ACL\0");
    bytes.extend_from_slice(&1_u16.to_be_bytes());
    bytes.extend_from_slice(&(entries.len() as u16).to_be_bytes());
    for entry in entries {
        if entry.flags & !ACL_FLAGS_MASK != 0 || entry.rights & !ACL_RIGHTS_MASK != 0 {
            return Err(CoreError::InvalidRecord("Apple ACL mask"));
        }
        bytes.extend_from_slice(&[entry.tag as u8, 1, 0, 0]);
        bytes.extend_from_slice(&entry.flags.to_be_bytes());
        bytes.extend_from_slice(&entry.rights.to_be_bytes());
        bytes.extend_from_slice(&entry.qualifier_uuid);
    }
    Ok(bytes)
}

pub fn decode_apple_acl(bytes: &[u8]) -> CoreResult<Vec<AppleAclEntryV1>> {
    if bytes.len() < 12 {
        return Err(CoreError::UnexpectedEof);
    }
    if &bytes[..8] != b"LFS4ACL\0" || u16::from_be_bytes([bytes[8], bytes[9]]) != 1 {
        return Err(CoreError::Unsupported);
    }
    let count = usize::from(u16::from_be_bytes([bytes[10], bytes[11]]));
    if count == 0 || count > 128 {
        return Err(CoreError::InvalidRecord("Apple ACL count"));
    }
    let expected = 12 + 36 * count;
    if bytes.len() < expected {
        return Err(CoreError::UnexpectedEof);
    }
    if bytes.len() > expected {
        return Err(CoreError::TrailingBytes);
    }
    bytes[12..]
        .chunks_exact(36)
        .map(|entry| {
            let tag = match entry[0] {
                1 => AppleAclTag::Allow,
                2 => AppleAclTag::Deny,
                _ => return Err(CoreError::InvalidRecord("Apple ACL tag")),
            };
            if entry[1] != 1 || entry[2..4] != [0, 0] {
                return Err(CoreError::InvalidRecord("Apple ACL qualifier"));
            }
            let flags = u64::from_be_bytes(entry[4..12].try_into().unwrap());
            let rights = u64::from_be_bytes(entry[12..20].try_into().unwrap());
            if flags & !ACL_FLAGS_MASK != 0 || rights & !ACL_RIGHTS_MASK != 0 {
                return Err(CoreError::InvalidRecord("Apple ACL mask"));
            }
            Ok(AppleAclEntryV1 {
                tag,
                flags,
                rights,
                qualifier_uuid: entry[20..36].try_into().unwrap(),
            })
        })
        .collect()
}

pub fn encode_bsd_flags(flags: u32) -> CoreResult<Option<[u8; 4]>> {
    if flags & !SUPPORTED_BSD_FLAGS != 0 {
        return Err(CoreError::InvalidRecord("BSD flags"));
    }
    Ok((flags != 0).then(|| flags.to_be_bytes()))
}

#[derive(Clone)]
struct MetadataSummary {
    id: ObjectId,
    min: Option<MetadataKey>,
    max: Option<MetadataKey>,
    entries: u64,
    encoded_bytes: u64,
    level: u8,
}

struct ValidatedMetadataNode {
    node: MetadataNodeV1,
    summary: MetadataSummary,
}

#[derive(Default)]
struct MetadataBranchPending {
    groups: Vec<Vec<MetadataSummary>>,
}
