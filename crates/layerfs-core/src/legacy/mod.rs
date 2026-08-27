//! Read-only compatibility decoders for retained Phase-4 v1/v2 mappings.
//! No type in this module can construct or publish a filesystem root.

use crate::object::access::ObjectRead;
use crate::{decode_bytes_object, CanonicalName, CanonicalPath, CoreError, CoreResult, ObjectId};

const MAGIC: &[u8; 8] = b"LFS4MAP\0";
const MAX_ENTRIES: usize = 100_000;
const FILE_FANOUT: usize = 64;
const DESCRIPTOR_BYTES: usize = 40;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MappingVersion {
    V1,
    V2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileReferenceV1 {
    pub raw_id: ObjectId,
    pub raw_length: u32,
    pub object_id: ObjectId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileReferenceV2 {
    pub raw_length: u32,
    pub object_id: ObjectId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileChild {
    pub cumulative_end: u64,
    pub object_id: ObjectId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileRoot {
    pub mode: u32,
    pub total_raw: u64,
    pub reference_count: u64,
    pub level: u8,
    pub children: Vec<FileChild>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectoryPageRef {
    pub count: u32,
    pub first_name: CanonicalName,
    pub object_id: ObjectId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyTransition {
    pub parent: Option<ObjectId>,
    pub child: ObjectId,
    pub entry_count: u32,
    pub pages: Vec<ObjectId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransitionOperation {
    Add {
        path: CanonicalPath,
        after: ObjectId,
    },
    Remove {
        path: CanonicalPath,
        before: ObjectId,
    },
    Replace {
        path: CanonicalPath,
        before: ObjectId,
        after: ObjectId,
    },
    Metadata {
        path: CanonicalPath,
        before: ObjectId,
        before_mode: u32,
        after: ObjectId,
        after_mode: u32,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegacyMapping {
    FileRoot(MappingVersion, FileRoot),
    FileLeafV1(Vec<FileReferenceV1>),
    FileLeafV2(Vec<FileReferenceV2>),
    FileBranch(MappingVersion, u8, Vec<FileChild>),
    DirectoryIndex(MappingVersion, u32, Vec<DirectoryPageRef>),
    DirectoryMetadata(MappingVersion, u32),
    DeltaIndex(MappingVersion, LegacyTransition),
    DeltaPage(MappingVersion, Vec<TransitionOperation>),
}

pub fn read_mapping(store: &impl ObjectRead, id: ObjectId) -> CoreResult<LegacyMapping> {
    store.with_authenticated_canonical(id, decode_mapping)
}

pub fn decode_mapping(canonical: &[u8]) -> CoreResult<LegacyMapping> {
    let payload = decode_bytes_object(canonical)?;
    if payload.len() < 11 {
        return Err(CoreError::UnexpectedEof);
    }
    if &payload[..8] != MAGIC {
        return Err(CoreError::InvalidMappingTag { tag: 0 });
    }
    let version = match u16::from_be_bytes([payload[8], payload[9]]) {
        1 => MappingVersion::V1,
        2 => MappingVersion::V2,
        version => return Err(CoreError::UnsupportedMappingVersion { version }),
    };
    let body = &payload[11..];
    match payload[10] {
        0x01 => Ok(LegacyMapping::FileRoot(version, decode_file_root(body)?)),
        0x02 => match version {
            MappingVersion::V1 => Ok(LegacyMapping::FileLeafV1(decode_leaf_v1(body)?)),
            MappingVersion::V2 => Ok(LegacyMapping::FileLeafV2(decode_leaf_v2(body)?)),
        },
        0x03 => {
            let (total, pages) = decode_directory_index(body)?;
            Ok(LegacyMapping::DirectoryIndex(version, total, pages))
        }
        0x04 if body.len() == 4 => Ok(LegacyMapping::DirectoryMetadata(
            version,
            u32::from_be_bytes(body.try_into().map_err(|_| CoreError::UnexpectedEof)?),
        )),
        0x04 => Err(if body.len() < 4 {
            CoreError::UnexpectedEof
        } else {
            CoreError::TrailingBytes
        }),
        0x05 => Ok(LegacyMapping::DeltaIndex(version, decode_transition(body)?)),
        0x06 => Ok(LegacyMapping::DeltaPage(version, decode_delta_page(body)?)),
        0x07 => {
            let (level, children) = decode_children(body, true)?;
            if level == 0 {
                return Err(CoreError::NonCanonicalPagePartition);
            }
            Ok(LegacyMapping::FileBranch(version, level, children))
        }
        tag => Err(CoreError::InvalidMappingTag { tag }),
    }
}

fn decode_leaf_v1(body: &[u8]) -> CoreResult<Vec<FileReferenceV1>> {
    let count = count(body)?;
    exact_len(body, 4, count, 68)?;
    body[4..]
        .chunks_exact(68)
        .map(|bytes| {
            Ok(FileReferenceV1 {
                raw_id: ObjectId::from_bytes(&bytes[..32])?,
                raw_length: bounded_chunk_length(&bytes[32..36])?,
                object_id: ObjectId::from_bytes(&bytes[36..])?,
            })
        })
        .collect()
}

fn decode_leaf_v2(body: &[u8]) -> CoreResult<Vec<FileReferenceV2>> {
    let count = count(body)?;
    exact_len(body, 4, count, 36)?;
    body[4..]
        .chunks_exact(36)
        .map(|bytes| {
            Ok(FileReferenceV2 {
                raw_length: bounded_chunk_length(&bytes[..4])?,
                object_id: ObjectId::from_bytes(&bytes[4..])?,
            })
        })
        .collect()
}

fn bounded_chunk_length(bytes: &[u8]) -> CoreResult<u32> {
    let length = u32::from_be_bytes(bytes.try_into().map_err(|_| CoreError::UnexpectedEof)?);
    if usize::try_from(length).map_err(|_| CoreError::LengthOverflow)?
        > crate::cdc::MAXIMUM_CHUNK_BYTES
    {
        return Err(CoreError::ObjectLimitExceeded);
    }
    Ok(length)
}

fn decode_file_root(body: &[u8]) -> CoreResult<FileRoot> {
    if body.len() < 25 {
        return Err(CoreError::UnexpectedEof);
    }
    let mode = u32::from_be_bytes(body[..4].try_into().unwrap());
    let total_raw = u64::from_be_bytes(body[4..12].try_into().unwrap());
    let reference_count = u64::from_be_bytes(body[12..20].try_into().unwrap());
    let level = body[20];
    let (_, children) = decode_children(&body[20..], true)?;
    if reference_count == 0 {
        if total_raw != 0 || level != 0 || !children.is_empty() {
            return Err(CoreError::NonCanonicalPagePartition);
        }
    } else if children.is_empty()
        || children.last().map(|child| child.cumulative_end) != Some(total_raw)
        || expected_level(reference_count)? != level
    {
        return Err(CoreError::NonCanonicalPagePartition);
    }
    Ok(FileRoot {
        mode,
        total_raw,
        reference_count,
        level,
        children,
    })
}

fn decode_children(body: &[u8], with_level: bool) -> CoreResult<(u8, Vec<FileChild>)> {
    let header = if with_level { 5 } else { 4 };
    if body.len() < header {
        return Err(CoreError::UnexpectedEof);
    }
    let level = if with_level { body[0] } else { 0 };
    let offset = usize::from(with_level);
    let count = usize::try_from(u32::from_be_bytes(
        body[offset..offset + 4].try_into().unwrap(),
    ))
    .map_err(|_| CoreError::LengthOverflow)?;
    if count > FILE_FANOUT {
        return Err(CoreError::ObjectLimitExceeded);
    }
    exact_len(body, header, count, DESCRIPTOR_BYTES)?;
    let children = body[header..]
        .chunks_exact(DESCRIPTOR_BYTES)
        .map(|bytes| {
            Ok(FileChild {
                cumulative_end: u64::from_be_bytes(bytes[..8].try_into().unwrap()),
                object_id: ObjectId::from_bytes(&bytes[8..])?,
            })
        })
        .collect::<CoreResult<Vec<_>>>()?;
    if children
        .windows(2)
        .any(|pair| pair[0].cumulative_end >= pair[1].cumulative_end)
    {
        return Err(CoreError::NonCanonicalOrdering);
    }
    Ok((level, children))
}

fn decode_directory_index(body: &[u8]) -> CoreResult<(u32, Vec<DirectoryPageRef>)> {
    if body.len() < 8 {
        return Err(CoreError::UnexpectedEof);
    }
    let total = u32::from_be_bytes(body[..4].try_into().unwrap());
    let count = usize::try_from(u32::from_be_bytes(body[4..8].try_into().unwrap()))
        .map_err(|_| CoreError::LengthOverflow)?;
    if count > MAX_ENTRIES {
        return Err(CoreError::ObjectLimitExceeded);
    }
    let mut offset = 8_usize;
    let mut pages = Vec::with_capacity(count.min(1024));
    let mut observed = 0_u64;
    for _ in 0..count {
        let fixed = offset.checked_add(6).ok_or(CoreError::LengthOverflow)?;
        if fixed > body.len() {
            return Err(CoreError::UnexpectedEof);
        }
        let page_count = u32::from_be_bytes(body[offset..offset + 4].try_into().unwrap());
        let name_len = usize::from(u16::from_be_bytes(
            body[offset + 4..fixed].try_into().unwrap(),
        ));
        offset = fixed;
        let end = offset
            .checked_add(name_len)
            .and_then(|value| value.checked_add(32))
            .ok_or(CoreError::LengthOverflow)?;
        if end > body.len() || page_count == 0 {
            return Err(CoreError::UnexpectedEof);
        }
        pages.push(DirectoryPageRef {
            count: page_count,
            first_name: CanonicalName::from_bytes(&body[offset..offset + name_len])?,
            object_id: ObjectId::from_bytes(&body[offset + name_len..end])?,
        });
        observed = observed
            .checked_add(u64::from(page_count))
            .ok_or(CoreError::LengthOverflow)?;
        offset = end;
    }
    if offset != body.len() || observed != u64::from(total) {
        return Err(if offset < body.len() {
            CoreError::TrailingBytes
        } else {
            CoreError::LengthMismatch {
                expected: u64::from(total),
                actual: observed,
            }
        });
    }
    if pages
        .windows(2)
        .any(|pair| pair[0].first_name >= pair[1].first_name)
    {
        return Err(CoreError::NonCanonicalOrdering);
    }
    Ok((total, pages))
}

fn decode_transition(body: &[u8]) -> CoreResult<LegacyTransition> {
    if body.len() < 41 {
        return Err(CoreError::UnexpectedEof);
    }
    let mut offset = 1_usize;
    let parent = match body[0] {
        0 => None,
        1 => {
            let end = offset + 32;
            let id = ObjectId::from_bytes(body.get(offset..end).ok_or(CoreError::UnexpectedEof)?)?;
            offset = end;
            Some(id)
        }
        value => return Err(CoreError::InvalidMappingDiscriminator { value }),
    };
    let child_end = offset.checked_add(32).ok_or(CoreError::LengthOverflow)?;
    let child = ObjectId::from_bytes(
        body.get(offset..child_end)
            .ok_or(CoreError::UnexpectedEof)?,
    )?;
    offset = child_end;
    let fields_end = offset.checked_add(8).ok_or(CoreError::LengthOverflow)?;
    let fields = body
        .get(offset..fields_end)
        .ok_or(CoreError::UnexpectedEof)?;
    let entry_count = u32::from_be_bytes(fields[..4].try_into().unwrap());
    let page_count = usize::try_from(u32::from_be_bytes(fields[4..].try_into().unwrap()))
        .map_err(|_| CoreError::LengthOverflow)?;
    if page_count > MAX_ENTRIES
        || usize::try_from(entry_count).unwrap_or(MAX_ENTRIES + 1) > MAX_ENTRIES
    {
        return Err(CoreError::ObjectLimitExceeded);
    }
    if (entry_count == 0) != (page_count == 0)
        || parent.is_none() && (entry_count != 0 || page_count != 0)
    {
        return Err(CoreError::NonCanonicalPagePartition);
    }
    offset = fields_end;
    exact_len(body, offset, page_count, 32)?;
    let pages = body[offset..]
        .chunks_exact(32)
        .map(ObjectId::from_bytes)
        .collect::<CoreResult<Vec<_>>>()?;
    Ok(LegacyTransition {
        parent,
        child,
        entry_count,
        pages,
    })
}

fn decode_delta_page(body: &[u8]) -> CoreResult<Vec<TransitionOperation>> {
    let count = count(body)?;
    if count == 0 || count > body.len().saturating_sub(4) {
        return Err(CoreError::NonCanonicalPagePartition);
    }
    let mut offset = 4_usize;
    let mut entries = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        let kind = *body.get(offset).ok_or(CoreError::UnexpectedEof)?;
        offset += 1;
        let length_end = offset.checked_add(4).ok_or(CoreError::LengthOverflow)?;
        let path_len = usize::try_from(u32::from_be_bytes(
            body.get(offset..length_end)
                .ok_or(CoreError::UnexpectedEof)?
                .try_into()
                .unwrap(),
        ))
        .map_err(|_| CoreError::LengthOverflow)?;
        offset = length_end;
        let path_end = offset
            .checked_add(path_len)
            .ok_or(CoreError::LengthOverflow)?;
        let path =
            CanonicalPath::from_bytes(body.get(offset..path_end).ok_or(CoreError::UnexpectedEof)?)?;
        offset = path_end;
        let entry = match kind {
            1 => TransitionOperation::Add {
                path,
                after: take_id(body, &mut offset)?,
            },
            2 => TransitionOperation::Remove {
                path,
                before: take_id(body, &mut offset)?,
            },
            3 => TransitionOperation::Replace {
                path,
                before: take_id(body, &mut offset)?,
                after: take_id(body, &mut offset)?,
            },
            4 => TransitionOperation::Metadata {
                path,
                before: take_id(body, &mut offset)?,
                before_mode: take_u32(body, &mut offset)?,
                after: take_id(body, &mut offset)?,
                after_mode: take_u32(body, &mut offset)?,
            },
            value => return Err(CoreError::InvalidMappingDiscriminator { value }),
        };
        entries.push(entry);
    }
    if offset != body.len() {
        return Err(CoreError::TrailingBytes);
    }
    Ok(entries)
}

fn count(body: &[u8]) -> CoreResult<usize> {
    let bytes = body.get(..4).ok_or(CoreError::UnexpectedEof)?;
    let count = usize::try_from(u32::from_be_bytes(bytes.try_into().unwrap()))
        .map_err(|_| CoreError::LengthOverflow)?;
    if count > MAX_ENTRIES {
        return Err(CoreError::ObjectLimitExceeded);
    }
    Ok(count)
}

fn exact_len(body: &[u8], header: usize, count: usize, width: usize) -> CoreResult<()> {
    let expected = header
        .checked_add(count.checked_mul(width).ok_or(CoreError::LengthOverflow)?)
        .ok_or(CoreError::LengthOverflow)?;
    if body.len() != expected {
        return Err(if body.len() < expected {
            CoreError::UnexpectedEof
        } else {
            CoreError::TrailingBytes
        });
    }
    Ok(())
}

fn expected_level(references: u64) -> CoreResult<u8> {
    let mut nodes = references.div_ceil(FILE_FANOUT as u64);
    let mut level = 0_u8;
    while nodes > FILE_FANOUT as u64 {
        nodes = nodes.div_ceil(FILE_FANOUT as u64);
        level = level
            .checked_add(1)
            .ok_or(CoreError::MappingDepthExceeded)?;
    }
    Ok(level)
}

fn take_id(body: &[u8], offset: &mut usize) -> CoreResult<ObjectId> {
    let end = offset.checked_add(32).ok_or(CoreError::LengthOverflow)?;
    let id = ObjectId::from_bytes(body.get(*offset..end).ok_or(CoreError::UnexpectedEof)?)?;
    *offset = end;
    Ok(id)
}

fn take_u32(body: &[u8], offset: &mut usize) -> CoreResult<u32> {
    let end = offset.checked_add(4).ok_or(CoreError::LengthOverflow)?;
    let value = u32::from_be_bytes(
        body.get(*offset..end)
            .ok_or(CoreError::UnexpectedEof)?
            .try_into()
            .unwrap(),
    );
    *offset = end;
    Ok(value)
}
