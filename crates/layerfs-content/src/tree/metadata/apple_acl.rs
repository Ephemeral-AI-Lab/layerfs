use super::{ACL_FLAGS_MASK, ACL_RIGHTS_MASK, SUPPORTED_BSD_FLAGS};
use crate::{CoreError, CoreResult};

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
