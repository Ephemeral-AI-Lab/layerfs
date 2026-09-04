use crate::tree::directory::{DirectoryStateV1, SymlinkStateV1};
use crate::tree::inode::InodeId;
use crate::tree::NamespaceRootV1;
use crate::{
    decode_bytes_object, encode_bytes_object, CanonicalName, CoreError, CoreResult, ObjectId,
};

pub(crate) const VERSION: u16 = 1;
pub(crate) const NODE_LIMIT: usize = 8192;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DirectoryNodeV1 {
    Leaf {
        subtree_encoded_bytes: u64,
        entries: Vec<(CanonicalName, InodeId)>,
    },
    Branch {
        level: u8,
        subtree_entry_count: u64,
        subtree_encoded_bytes: u64,
        children: Vec<(CanonicalName, ObjectId)>,
    },
}

pub fn encode_directory_node(node: &DirectoryNodeV1) -> CoreResult<Vec<u8>> {
    match node {
        DirectoryNodeV1::Leaf {
            subtree_encoded_bytes,
            entries,
        } => encode_directory_page(
            0,
            entries.len() as u64,
            *subtree_encoded_bytes,
            entries.iter().map(|(name, inode)| (name, inode.as_bytes())),
        ),
        DirectoryNodeV1::Branch {
            level,
            subtree_entry_count,
            subtree_encoded_bytes,
            children,
        } => {
            validate_node_header(*level, children.len())?;
            if *level == 0 {
                return Err(CoreError::InvalidRecord("directory branch level"));
            }
            encode_directory_page(
                *level,
                *subtree_entry_count,
                *subtree_encoded_bytes,
                children
                    .iter()
                    .map(|(name, child)| (name, child.as_bytes())),
            )
        }
    }
}

/// Borrowed ordered pairs encoded directly into one owned canonical buffer.
/// Shared by the public node codec and the bounded final-state writer.
pub(crate) fn encode_directory_page<'a>(
    level: u8,
    subtree_count: u64,
    subtree_bytes: u64,
    entries: impl ExactSizeIterator<Item = (&'a CanonicalName, &'a [u8; 32])> + Clone,
) -> CoreResult<Vec<u8>> {
    let count = entries.len();
    validate_node_header(level, count)?;
    ordered(entries.clone().map(|(name, _)| name.as_bytes()))?;
    let entry_bytes = entries.clone().try_fold(0usize, |sum, (name, _)| {
        sum.checked_add(34 + name.as_bytes().len())
            .ok_or(CoreError::LengthOverflow)
    })?;
    if level == 0 && (subtree_count != count as u64 || subtree_bytes != entry_bytes as u64) {
        return Err(CoreError::LengthMismatch {
            expected: subtree_bytes,
            actual: entry_bytes as u64,
        });
    }
    let mut canonical = canonical_node_header(
        b"LFS4NSP\0",
        if level == 0 { 1 } else { 2 },
        level,
        count,
        subtree_count,
        subtree_bytes,
        entry_bytes,
    )?;
    for (name, target) in entries {
        put_bytes(&mut canonical, name.as_bytes())?;
        canonical.extend_from_slice(target);
    }
    Ok(canonical)
}

pub(crate) fn encode_directory_leaf_from_iter(
    count: usize,
    bytes: u64,
    entries: impl Iterator<Item = CoreResult<(CanonicalName, ObjectId)>>,
) -> CoreResult<Vec<u8>> {
    validate_node_header(0, count)?;
    let mut canonical = canonical_node_header(
        b"LFS4NSP\0",
        1,
        0,
        count,
        count as u64,
        bytes,
        usize::try_from(bytes).map_err(|_| CoreError::LengthOverflow)?,
    )?;
    let mut prior = None;
    let mut actual = 0usize;
    let mut logical = 0u64;
    for entry in entries {
        let (key, id) = entry?;
        if prior.as_ref().is_some_and(|prior| prior >= &key) {
            return Err(CoreError::NonCanonicalOrdering);
        }
        logical = logical
            .checked_add(34 + key.as_bytes().len() as u64)
            .ok_or(CoreError::LengthOverflow)?;
        if actual >= count || logical > bytes {
            return Err(CoreError::InvalidRecord("spilled directory leaf summary"));
        }
        put_bytes(&mut canonical, key.as_bytes())?;
        canonical.extend_from_slice(id.as_bytes());
        actual += 1;
        prior = Some(key);
    }
    if actual != count || logical != bytes {
        return Err(CoreError::InvalidRecord("spilled directory leaf summary"));
    }
    Ok(canonical)
}

pub(crate) fn canonical_node_header(
    magic: &[u8; 8],
    role: u8,
    level: u8,
    count: usize,
    subtree_count: u64,
    subtree_bytes: u64,
    entry_bytes: usize,
) -> CoreResult<Vec<u8>> {
    let size = 44usize
        .checked_add(entry_bytes)
        .ok_or(CoreError::LengthOverflow)?;
    if size > NODE_LIMIT {
        return Err(CoreError::ObjectLimitExceeded);
    }
    let mut canonical = Vec::with_capacity(size);
    crate::object::encode_bytes_object_header_to(size - 13, &mut canonical)?;
    canonical.extend_from_slice(&node_header(
        magic,
        role,
        level,
        count,
        subtree_count,
        subtree_bytes,
    )?);
    Ok(canonical)
}

pub fn decode_directory_node(canonical: &[u8]) -> CoreResult<DirectoryNodeV1> {
    let value = decode_node_value(canonical, b"LFS4NSP\0")?;
    let role = value[10];
    let level = value[11];
    let count = node_count(value);
    ensure_count_fits(value, count, 35)?;
    let subtree_count = node_subtree_count(value);
    let subtree_bytes = node_subtree_bytes(value);
    let mut cursor = 31;
    let node = match role {
        1 if level == 0 && subtree_count == count as u64 => {
            let mut entries = Vec::with_capacity(count);
            for _ in 0..count {
                let name = CanonicalName::from_bytes(take_bytes(value, &mut cursor, 255)?)?;
                let inode = InodeId::from_slice(take(value, &mut cursor, 32)?)?;
                entries.push((name, inode));
            }
            DirectoryNodeV1::Leaf {
                subtree_encoded_bytes: subtree_bytes,
                entries,
            }
        }
        2 if level > 0 => {
            let mut children = Vec::with_capacity(count);
            for _ in 0..count {
                let name = CanonicalName::from_bytes(take_bytes(value, &mut cursor, 255)?)?;
                let child = ObjectId::from_bytes(take(value, &mut cursor, 32)?)?;
                children.push((name, child));
            }
            DirectoryNodeV1::Branch {
                level,
                subtree_entry_count: subtree_count,
                subtree_encoded_bytes: subtree_bytes,
                children,
            }
        }
        1 | 2 => return Err(CoreError::InvalidRecord("directory node level")),
        tag => return Err(CoreError::InvalidMappingTag { tag }),
    };
    if cursor != value.len() {
        return Err(CoreError::TrailingBytes);
    }
    match &node {
        DirectoryNodeV1::Leaf { entries, .. } => {
            ordered(entries.iter().map(|(name, _)| name.as_bytes()))?
        }
        DirectoryNodeV1::Branch { children, .. } => {
            ordered(children.iter().map(|(name, _)| name.as_bytes()))?
        }
    }
    verify_subtree_bytes(&node, subtree_bytes)?;
    Ok(node)
}

pub fn encode_directory_state(value: DirectoryStateV1) -> CoreResult<Vec<u8>> {
    if value.profile_id != profile_id() || value.tree_level > 31 {
        return Err(CoreError::ProfileMismatch);
    }
    let mut bytes = Vec::with_capacity(84);
    bytes.extend_from_slice(b"LFS4DIR\0");
    bytes.extend_from_slice(&VERSION.to_be_bytes());
    bytes.extend_from_slice(&[3, 0]);
    bytes.extend_from_slice(&value.entry_count.to_be_bytes());
    bytes.push(value.tree_level);
    bytes.extend_from_slice(value.profile_id.as_bytes());
    bytes.extend_from_slice(value.mapping_root.as_bytes());
    encode_bytes_object(&bytes)
}

pub fn decode_directory_state(canonical: &[u8]) -> CoreResult<DirectoryStateV1> {
    let bytes = exact_value(canonical, b"LFS4DIR\0", 85, 3)?;
    let value = DirectoryStateV1 {
        entry_count: u64::from_be_bytes(bytes[12..20].try_into().unwrap()),
        tree_level: bytes[20],
        profile_id: ObjectId::from_bytes(&bytes[21..53])?,
        mapping_root: ObjectId::from_bytes(&bytes[53..85])?,
    };
    if value.profile_id != profile_id() {
        return Err(CoreError::ProfileMismatch);
    }
    if value.tree_level > 31 {
        return Err(CoreError::MappingDepthExceeded);
    }
    Ok(value)
}

pub fn encode_symlink(value: &SymlinkStateV1) -> CoreResult<Vec<u8>> {
    let value = SymlinkStateV1::new(value.target.clone())?;
    let mut bytes = Vec::with_capacity(14 + value.target.len());
    bytes.extend_from_slice(b"LFS4LNK\0");
    bytes.extend_from_slice(&VERSION.to_be_bytes());
    bytes.extend_from_slice(&[5, 0]);
    bytes.extend_from_slice(&(value.target.len() as u16).to_be_bytes());
    bytes.extend_from_slice(&value.target);
    encode_bytes_object(&bytes)
}

pub fn decode_symlink(canonical: &[u8]) -> CoreResult<SymlinkStateV1> {
    let bytes = decode_bytes_object(canonical)?;
    if bytes.len() < 14 {
        return Err(CoreError::UnexpectedEof);
    }
    header(bytes, b"LFS4LNK\0", 5)?;
    let len = usize::from(u16::from_be_bytes([bytes[12], bytes[13]]));
    if bytes.len() < 14 + len {
        return Err(CoreError::UnexpectedEof);
    }
    if bytes.len() > 14 + len {
        return Err(CoreError::TrailingBytes);
    }
    SymlinkStateV1::new(bytes[14..].to_vec())
}

pub fn encode_namespace_root(value: NamespaceRootV1) -> CoreResult<Vec<u8>> {
    if value.profile_id != profile_id() {
        return Err(CoreError::ProfileMismatch);
    }
    let mut bytes = Vec::with_capacity(108);
    bytes.extend_from_slice(b"LFS4FSR\0");
    bytes.extend_from_slice(&VERSION.to_be_bytes());
    bytes.extend_from_slice(&[6, 0]);
    bytes.extend_from_slice(value.profile_id.as_bytes());
    bytes.extend_from_slice(value.root_directory_inode.as_bytes());
    bytes.extend_from_slice(value.inode_table_root.as_bytes());
    encode_bytes_object(&bytes)
}

pub fn decode_namespace_root(canonical: &[u8]) -> CoreResult<NamespaceRootV1> {
    let bytes = exact_value(canonical, b"LFS4FSR\0", 108, 6)?;
    let value = NamespaceRootV1 {
        profile_id: ObjectId::from_bytes(&bytes[12..44])?,
        root_directory_inode: InodeId::from_slice(&bytes[44..76])?,
        inode_table_root: ObjectId::from_bytes(&bytes[76..108])?,
    };
    if value.profile_id != profile_id() {
        return Err(CoreError::ProfileMismatch);
    }
    Ok(value)
}

pub fn profile_id() -> ObjectId {
    static PROFILE_ID: std::sync::OnceLock<ObjectId> = std::sync::OnceLock::new();
    *PROFILE_ID.get_or_init(|| {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"layerfs/namespace-profile/bplus/v1\0");
        bytes.extend_from_slice(&1_u16.to_be_bytes());
        bytes.extend_from_slice(&8192_u32.to_be_bytes());
        for value in [13_u16, 31, 2, 5] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        bytes.push(31);
        for value in [255_u16, 4096, 64, 255, 127, 64, 127, 128, 64, 64, 2] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        bytes.extend_from_slice(&[
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 1, 2, 3, 1, 1, 1, 2, 4, 8, 8, 32, 32, 1,
        ]);
        ObjectId::from_bytes(blake3::hash(&bytes).as_bytes()).expect("BLAKE3 digest width")
    })
}

pub(crate) fn exact_value<'a>(
    canonical: &'a [u8],
    magic: &[u8; 8],
    len: usize,
    role: u8,
) -> CoreResult<&'a [u8]> {
    let bytes = decode_bytes_object(canonical)?;
    if bytes.len() < len {
        return Err(CoreError::UnexpectedEof);
    }
    if bytes.len() > len {
        return Err(CoreError::TrailingBytes);
    }
    header(bytes, magic, role)?;
    Ok(bytes)
}

fn header(bytes: &[u8], magic: &[u8; 8], role: u8) -> CoreResult<()> {
    if &bytes[..8] != magic {
        return Err(CoreError::Unsupported);
    }
    let version = u16::from_be_bytes([bytes[8], bytes[9]]);
    if version != VERSION {
        return Err(CoreError::UnsupportedMappingVersion { version });
    }
    if bytes[10] != role {
        return Err(CoreError::WrongLogicalRole);
    }
    if bytes[11] != 0 {
        return Err(CoreError::InvalidRecord("namespace flags"));
    }
    Ok(())
}

pub(crate) fn node_header(
    magic: &[u8; 8],
    role: u8,
    level: u8,
    count: usize,
    subtree_count: u64,
    subtree_bytes: u64,
) -> CoreResult<Vec<u8>> {
    let count = u16::try_from(count).map_err(|_| CoreError::LengthOverflow)?;
    let mut value = Vec::with_capacity(31);
    value.extend_from_slice(magic);
    value.extend_from_slice(&VERSION.to_be_bytes());
    value.extend_from_slice(&[role, level, 0]);
    value.extend_from_slice(&count.to_be_bytes());
    value.extend_from_slice(&subtree_count.to_be_bytes());
    value.extend_from_slice(&subtree_bytes.to_be_bytes());
    Ok(value)
}

pub(crate) fn decode_node_value<'a>(canonical: &'a [u8], magic: &[u8; 8]) -> CoreResult<&'a [u8]> {
    if canonical.len() > NODE_LIMIT {
        return Err(CoreError::ObjectLimitExceeded);
    }
    let value = decode_bytes_object(canonical)?;
    if value.len() < 31 {
        return Err(CoreError::UnexpectedEof);
    }
    if &value[..8] != magic {
        return Err(CoreError::Unsupported);
    }
    let version = u16::from_be_bytes([value[8], value[9]]);
    if version != VERSION {
        return Err(CoreError::UnsupportedMappingVersion { version });
    }
    if value[12] != 0 {
        return Err(CoreError::InvalidRecord("namespace node flags"));
    }
    validate_node_header(value[11], node_count(value))?;
    Ok(value)
}

pub(crate) fn validate_node_header(level: u8, count: usize) -> CoreResult<()> {
    if level > 31 {
        return Err(CoreError::MappingDepthExceeded);
    }
    if count > u16::MAX as usize {
        return Err(CoreError::ObjectLimitExceeded);
    }
    Ok(())
}

pub(crate) fn node_count(value: &[u8]) -> usize {
    usize::from(u16::from_be_bytes([value[13], value[14]]))
}
pub(crate) fn ensure_count_fits(
    value: &[u8],
    count: usize,
    minimum_entry_bytes: usize,
) -> CoreResult<()> {
    if count > value.len().saturating_sub(31) / minimum_entry_bytes {
        Err(CoreError::UnexpectedEof)
    } else {
        Ok(())
    }
}
pub(crate) fn node_subtree_count(value: &[u8]) -> u64 {
    u64::from_be_bytes(value[15..23].try_into().unwrap())
}
pub(crate) fn node_subtree_bytes(value: &[u8]) -> u64 {
    u64::from_be_bytes(value[23..31].try_into().unwrap())
}

pub(crate) fn finish_node(value: Vec<u8>) -> CoreResult<Vec<u8>> {
    let canonical = encode_bytes_object(&value)?;
    if canonical.len() > NODE_LIMIT {
        Err(CoreError::ObjectLimitExceeded)
    } else {
        Ok(canonical)
    }
}

pub(crate) fn put_bytes(output: &mut Vec<u8>, bytes: &[u8]) -> CoreResult<()> {
    output.extend_from_slice(
        &u16::try_from(bytes.len())
            .map_err(|_| CoreError::LengthOverflow)?
            .to_be_bytes(),
    );
    output.extend_from_slice(bytes);
    Ok(())
}

pub(crate) fn take<'a>(bytes: &'a [u8], cursor: &mut usize, len: usize) -> CoreResult<&'a [u8]> {
    let end = cursor.checked_add(len).ok_or(CoreError::LengthOverflow)?;
    if end > bytes.len() {
        return Err(CoreError::UnexpectedEof);
    }
    let value = &bytes[*cursor..end];
    *cursor = end;
    Ok(value)
}

pub(crate) fn take_bytes<'a>(
    bytes: &'a [u8],
    cursor: &mut usize,
    max: usize,
) -> CoreResult<&'a [u8]> {
    let len_bytes = take(bytes, cursor, 2)?;
    let len = usize::from(u16::from_be_bytes([len_bytes[0], len_bytes[1]]));
    if len > max {
        return Err(CoreError::ObjectLimitExceeded);
    }
    take(bytes, cursor, len)
}

pub(crate) fn ordered<'a>(values: impl Iterator<Item = &'a [u8]>) -> CoreResult<()> {
    let mut previous: Option<&[u8]> = None;
    for value in values {
        if previous.is_some_and(|prior| prior >= value) {
            return Err(CoreError::NonCanonicalOrdering);
        }
        previous = Some(value);
    }
    Ok(())
}

fn verify_subtree_bytes(node: &DirectoryNodeV1, expected: u64) -> CoreResult<()> {
    if let DirectoryNodeV1::Leaf { entries, .. } = node {
        let actual = entries.iter().try_fold(0_u64, |sum, (name, _)| {
            sum.checked_add(34 + name.as_bytes().len() as u64)
                .ok_or(CoreError::LengthOverflow)
        })?;
        if actual != expected {
            return Err(CoreError::LengthMismatch { expected, actual });
        }
    }
    Ok(())
}
