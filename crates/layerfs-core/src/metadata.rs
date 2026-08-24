use crate::content::rope::{ObjectRead, ObjectStore};
use crate::inode::InodeKind;
use crate::namespace_codec::{decode_metadata_node, encode_metadata_node, MetadataNodeV1};
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

pub fn build_metadata_tree<S: ObjectStore>(
    store: &mut S,
    entries: &[MetadataEntryV1],
) -> CoreResult<ObjectId> {
    if entries.is_empty() {
        return Ok(emit_metadata(store, metadata_leaf(Vec::new())?)?.id);
    }
    for pair in entries.windows(2) {
        if pair[0].key >= pair[1].key {
            return Err(CoreError::NonCanonicalOrdering);
        }
    }
    let mut groups: Vec<Vec<MetadataEntryV1>> = Vec::new();
    for entry in entries.iter().cloned() {
        if groups.is_empty() {
            groups.push(Vec::new());
        }
        groups.last_mut().unwrap().push(entry);
        if encode_metadata_node(&metadata_leaf(groups.last().unwrap().clone())?).is_err() {
            let entry = groups.last_mut().unwrap().pop().unwrap();
            groups.push(vec![entry]);
        }
    }
    rebalance_leaf_tail(&mut groups)?;
    let mut level = groups
        .into_iter()
        .map(|group| emit_metadata(store, metadata_leaf(group)?))
        .collect::<CoreResult<Vec<_>>>()?;
    while level.len() > 1 {
        let next_level = level[0]
            .level
            .checked_add(1)
            .ok_or(CoreError::MappingDepthExceeded)?;
        let mut groups: Vec<Vec<MetadataSummary>> = Vec::new();
        for child in level {
            if groups.is_empty() {
                groups.push(Vec::new());
            }
            groups.last_mut().unwrap().push(child);
            if encode_metadata_node(&metadata_branch(next_level, groups.last().unwrap())?).is_err()
            {
                let child = groups.last_mut().unwrap().pop().unwrap();
                groups.push(vec![child]);
            }
        }
        rebalance_branch_tail(next_level, &mut groups)?;
        level = groups
            .into_iter()
            .map(|group| emit_metadata(store, metadata_branch(next_level, &group)?))
            .collect::<CoreResult<Vec<_>>>()?;
    }
    Ok(level.remove(0).id)
}

pub fn metadata_tree_entries<S: ObjectRead>(
    store: &S,
    root: ObjectId,
) -> CoreResult<Vec<MetadataEntryV1>> {
    let mut output = Vec::new();
    visit_metadata_entries(store, root, |entries| {
        output.extend_from_slice(entries);
        Ok(())
    })?;
    Ok(output)
}

pub fn visit_metadata_entries<S: ObjectRead>(
    store: &S,
    root: ObjectId,
    mut visitor: impl FnMut(&[MetadataEntryV1]) -> CoreResult<()>,
) -> CoreResult<()> {
    read_metadata_node(store, root, true, None, None, &mut Vec::new(), &mut visitor)?;
    Ok(())
}

fn read_metadata_node<S: ObjectRead>(
    store: &S,
    id: ObjectId,
    root: bool,
    expected_level: Option<u8>,
    expected_max: Option<&MetadataKey>,
    ancestors: &mut Vec<ObjectId>,
    visitor: &mut impl FnMut(&[MetadataEntryV1]) -> CoreResult<()>,
) -> CoreResult<MetadataSummary> {
    if ancestors.contains(&id) {
        return Err(CoreError::MappingCycle);
    }
    ancestors.push(id);
    let node = store.with_authenticated_canonical(id, |canonical| {
        if !root && canonical.len() * 5 < 8192 * 2 {
            return Err(CoreError::NonCanonicalPagePartition);
        }
        decode_metadata_node(canonical)
    })?;
    let (actual_level, actual_max) = match &node {
        MetadataNodeV1::Leaf { entries, .. } => (0, entries.last().map(|entry| entry.key.clone())),
        MetadataNodeV1::Branch {
            level, children, ..
        } => (*level, children.last().map(|child| child.0.clone())),
    };
    if expected_level.is_some_and(|level| actual_level != level)
        || expected_max.is_some_and(|maximum| actual_max.as_ref() != Some(maximum))
    {
        return Err(CoreError::InvalidRecord("metadata child summary"));
    }
    let summary = match node {
        MetadataNodeV1::Leaf {
            subtree_encoded_bytes,
            entries,
        } => {
            let max = entries.last().map(|entry| entry.key.clone());
            if !root && max.is_none() {
                return Err(CoreError::NonCanonicalPagePartition);
            }
            let summary = MetadataSummary {
                id,
                min: entries.first().map(|entry| entry.key.clone()),
                max,
                entries: entries.len() as u64,
                encoded_bytes: subtree_encoded_bytes,
                level: 0,
            };
            visitor(&entries)?;
            summary
        }
        MetadataNodeV1::Branch {
            level,
            subtree_entry_count,
            subtree_encoded_bytes,
            children,
        } => {
            if children.len() < 2 {
                return Err(CoreError::NonCanonicalPagePartition);
            }
            let mut count = 0_u64;
            let mut bytes = 0_u64;
            let mut minimum = None;
            let mut previous_max: Option<MetadataKey> = None;
            let child_level = level
                .checked_sub(1)
                .ok_or(CoreError::InvalidRecord("metadata child summary"))?;
            for (maximum, child) in &children {
                let child = read_metadata_node(
                    store,
                    *child,
                    false,
                    Some(child_level),
                    Some(maximum),
                    ancestors,
                    visitor,
                )?;
                if previous_max
                    .as_ref()
                    .zip(child.min.as_ref())
                    .is_some_and(|(previous, next)| previous >= next)
                {
                    return Err(CoreError::NonCanonicalOrdering);
                }
                if minimum.is_none() {
                    minimum = child.min.clone();
                }
                count = count
                    .checked_add(child.entries)
                    .ok_or(CoreError::LengthOverflow)?;
                bytes = bytes
                    .checked_add(child.encoded_bytes)
                    .ok_or(CoreError::LengthOverflow)?;
                previous_max = child.max.clone();
            }
            if count != subtree_entry_count || bytes != subtree_encoded_bytes {
                return Err(CoreError::InvalidRecord("metadata branch summary"));
            }
            MetadataSummary {
                id,
                min: minimum,
                max: children.last().map(|child| child.0.clone()),
                entries: count,
                encoded_bytes: bytes,
                level,
            }
        }
    };
    ancestors.pop();
    Ok(summary)
}

fn metadata_leaf(entries: Vec<MetadataEntryV1>) -> CoreResult<MetadataNodeV1> {
    let bytes = entries.iter().try_fold(0_u64, |sum, entry| {
        sum.checked_add(37 + entry.key.domain.len() as u64 + entry.key.key.len() as u64)
            .ok_or(CoreError::LengthOverflow)
    })?;
    Ok(MetadataNodeV1::Leaf {
        subtree_encoded_bytes: bytes,
        entries,
    })
}

fn metadata_branch(level: u8, children: &[MetadataSummary]) -> CoreResult<MetadataNodeV1> {
    let count = children.iter().try_fold(0_u64, |sum, child| {
        sum.checked_add(child.entries)
            .ok_or(CoreError::LengthOverflow)
    })?;
    let bytes = children.iter().try_fold(0_u64, |sum, child| {
        sum.checked_add(child.encoded_bytes)
            .ok_or(CoreError::LengthOverflow)
    })?;
    Ok(MetadataNodeV1::Branch {
        level,
        subtree_entry_count: count,
        subtree_encoded_bytes: bytes,
        children: children
            .iter()
            .map(|child| {
                Ok((
                    child
                        .max
                        .clone()
                        .ok_or(CoreError::InvalidRecord("empty metadata child"))?,
                    child.id,
                ))
            })
            .collect::<CoreResult<Vec<_>>>()?,
    })
}

fn emit_metadata<S: ObjectStore>(
    store: &mut S,
    node: MetadataNodeV1,
) -> CoreResult<MetadataSummary> {
    let (max, entries, encoded_bytes, level) = match &node {
        MetadataNodeV1::Leaf {
            subtree_encoded_bytes,
            entries,
        } => (
            entries.last().map(|entry| entry.key.clone()),
            entries.len() as u64,
            *subtree_encoded_bytes,
            0,
        ),
        MetadataNodeV1::Branch {
            level,
            subtree_entry_count,
            subtree_encoded_bytes,
            children,
        } => (
            Some(
                children
                    .last()
                    .ok_or(CoreError::InvalidRecord("empty metadata branch"))?
                    .0
                    .clone(),
            ),
            *subtree_entry_count,
            *subtree_encoded_bytes,
            *level,
        ),
    };
    let canonical = encode_metadata_node(&node)?;
    let id = store.put(&canonical)?;
    Ok(MetadataSummary {
        id,
        min: metadata_node_min(&node),
        max,
        entries,
        encoded_bytes,
        level,
    })
}

fn metadata_node_min(node: &MetadataNodeV1) -> Option<MetadataKey> {
    match node {
        MetadataNodeV1::Leaf { entries, .. } => entries.first().map(|entry| entry.key.clone()),
        MetadataNodeV1::Branch { children, .. } => children.first().map(|entry| entry.0.clone()),
    }
}

fn rebalance_leaf_tail(groups: &mut [Vec<MetadataEntryV1>]) -> CoreResult<()> {
    if groups.len() < 2 {
        return Ok(());
    }
    let last = groups.len() - 1;
    while encode_metadata_node(&metadata_leaf(groups[last].clone())?)?.len() * 5 < 8192 * 2 {
        let moved = groups[last - 1]
            .pop()
            .ok_or(CoreError::NonCanonicalPagePartition)?;
        groups[last].insert(0, moved);
    }
    Ok(())
}

fn rebalance_branch_tail(level: u8, groups: &mut [Vec<MetadataSummary>]) -> CoreResult<()> {
    if groups.len() < 2 {
        return Ok(());
    }
    let last = groups.len() - 1;
    while encode_metadata_node(&metadata_branch(level, &groups[last])?)?.len() * 5 < 8192 * 2 {
        let moved = groups[last - 1]
            .pop()
            .ok_or(CoreError::NonCanonicalPagePartition)?;
        groups[last].insert(0, moved);
    }
    Ok(())
}
