//! Phase-4 candidate file mapping primitives.
//!
//! The candidate selector lives in the benchmark.  This module only owns the
//! byte grammar shared by the candidate layouts.

use crate::identity::{ChunkId, ObjectId, DIGEST_BYTES};
use crate::object::{encode_object, Object};
use crate::{CoreError, CoreResult};

pub const MAPPING_MAGIC: [u8; 8] = *b"LFS4MAP\0";
pub const MAPPING_VERSION: u16 = 1;
pub const FILE_ROOT_TAG: u8 = 0x01;
pub const FILE_LEAF_TAG: u8 = 0x02;
pub const FILE_BRANCH_TAG: u8 = 0x07;
pub const DIR_INDEX_TAG: u8 = 0x03;
pub const DIR_METADATA_TAG: u8 = 0x04;
pub const DELTA_INDEX_TAG: u8 = 0x05;
pub const DELTA_PAGE_TAG: u8 = 0x06;
pub const FILE_REF_BYTES: usize = 68;
pub const FILE_DESCRIPTOR_BYTES: usize = 40;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileReference {
    pub raw_id: ChunkId,
    pub raw_length: u32,
    pub object_id: ObjectId,
}

impl FileReference {
    pub fn encode(self, output: &mut Vec<u8>) {
        output.extend_from_slice(self.raw_id.as_bytes());
        output.extend_from_slice(&self.raw_length.to_be_bytes());
        output.extend_from_slice(self.object_id.as_bytes());
    }

    pub fn decode(bytes: &[u8]) -> CoreResult<Self> {
        if bytes.len() != FILE_REF_BYTES {
            return Err(CoreError::UnexpectedEof);
        }
        Ok(Self {
            raw_id: ObjectId::from_bytes(&bytes[..DIGEST_BYTES])?,
            raw_length: u32::from_be_bytes(
                bytes[32..36]
                    .try_into()
                    .map_err(|_| CoreError::UnexpectedEof)?,
            ),
            object_id: ObjectId::from_bytes(&bytes[36..])?,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileChild {
    pub object_id: ObjectId,
    pub cumulative_end: u64,
}

impl FileChild {
    pub fn encode(self, output: &mut Vec<u8>) {
        output.extend_from_slice(self.object_id.as_bytes());
        output.extend_from_slice(&self.cumulative_end.to_be_bytes());
    }

    pub fn decode(bytes: &[u8]) -> CoreResult<Self> {
        if bytes.len() != FILE_DESCRIPTOR_BYTES {
            return Err(CoreError::UnexpectedEof);
        }
        Ok(Self {
            object_id: ObjectId::from_bytes(&bytes[..DIGEST_BYTES])?,
            cumulative_end: u64::from_be_bytes(
                bytes[32..]
                    .try_into()
                    .map_err(|_| CoreError::UnexpectedEof)?,
            ),
        })
    }
}

pub fn mapping_bytes(tag: u8, body: &[u8]) -> CoreResult<Vec<u8>> {
    let mut output = Vec::with_capacity(
        MAPPING_MAGIC
            .len()
            .checked_add(2)
            .and_then(|value| value.checked_add(1))
            .and_then(|value| value.checked_add(body.len()))
            .ok_or(CoreError::LengthOverflow)?,
    );
    output.extend_from_slice(&MAPPING_MAGIC);
    output.extend_from_slice(&MAPPING_VERSION.to_be_bytes());
    output.push(tag);
    output.extend_from_slice(body);
    Ok(output)
}

pub fn canonical_mapping(tag: u8, body: &[u8]) -> CoreResult<(ObjectId, Vec<u8>)> {
    let bytes = encode_object(&Object::bytes(mapping_bytes(tag, body)?)?)?;
    let id = ObjectId::for_bytes(&bytes);
    Ok((id, bytes))
}

pub fn decode_mapping(bytes: &[u8], expected_tag: u8) -> CoreResult<Vec<u8>> {
    let object = crate::object::decode_object(bytes)?;
    let Object::Bytes(payload) = object else {
        return Err(CoreError::WrongLogicalRole);
    };
    if payload.len() < MAPPING_MAGIC.len() + 3 {
        return Err(CoreError::UnexpectedEof);
    }
    if payload[..MAPPING_MAGIC.len()] != MAPPING_MAGIC {
        return Err(CoreError::InvalidMappingTag { tag: 0 });
    }
    let version = u16::from_be_bytes([payload[8], payload[9]]);
    if version != MAPPING_VERSION {
        return Err(CoreError::UnsupportedMappingVersion { version });
    }
    if payload[10] != expected_tag {
        return Err(CoreError::InvalidMappingTag { tag: payload[10] });
    }
    Ok(payload[11..].to_vec())
}

pub fn encode_file_leaf(references: &[FileReference]) -> CoreResult<Vec<u8>> {
    let count = u32::try_from(references.len()).map_err(|_| CoreError::LengthOverflow)?;
    let mut body = Vec::with_capacity(
        4usize
            .checked_add(
                references
                    .len()
                    .checked_mul(FILE_REF_BYTES)
                    .ok_or(CoreError::LengthOverflow)?,
            )
            .ok_or(CoreError::LengthOverflow)?,
    );
    body.extend_from_slice(&count.to_be_bytes());
    for reference in references {
        reference.encode(&mut body);
    }
    mapping_bytes(FILE_LEAF_TAG, &body)
}

pub fn encode_file_branch(level: u8, children: &[FileChild]) -> CoreResult<Vec<u8>> {
    let count = u32::try_from(children.len()).map_err(|_| CoreError::LengthOverflow)?;
    let mut body = Vec::with_capacity(
        1usize
            .checked_add(4)
            .and_then(|value| value.checked_add(children.len().checked_mul(FILE_DESCRIPTOR_BYTES)?))
            .ok_or(CoreError::LengthOverflow)?,
    );
    body.push(level);
    body.extend_from_slice(&count.to_be_bytes());
    for child in children {
        child.encode(&mut body);
    }
    mapping_bytes(FILE_BRANCH_TAG, &body)
}

pub fn encode_file_root(
    mode: u32,
    total_raw: u64,
    reference_count: u64,
    level: u8,
    children: &[FileChild],
) -> CoreResult<Vec<u8>> {
    let count = u32::try_from(children.len()).map_err(|_| CoreError::LengthOverflow)?;
    let mut body = Vec::with_capacity(
        4usize
            .checked_add(8)
            .and_then(|value| value.checked_add(8))
            .and_then(|value| value.checked_add(1))
            .and_then(|value| value.checked_add(4))
            .and_then(|value| value.checked_add(children.len().checked_mul(FILE_DESCRIPTOR_BYTES)?))
            .ok_or(CoreError::LengthOverflow)?,
    );
    body.extend_from_slice(&mode.to_be_bytes());
    body.extend_from_slice(&total_raw.to_be_bytes());
    body.extend_from_slice(&reference_count.to_be_bytes());
    body.push(level);
    body.extend_from_slice(&count.to_be_bytes());
    for child in children {
        child.encode(&mut body);
    }
    mapping_bytes(FILE_ROOT_TAG, &body)
}

pub fn parse_file_leaf(payload: &[u8]) -> CoreResult<Vec<FileReference>> {
    if payload.len() < 4 {
        return Err(CoreError::UnexpectedEof);
    }
    let count = usize::try_from(u32::from_be_bytes(
        payload[..4]
            .try_into()
            .map_err(|_| CoreError::UnexpectedEof)?,
    ))
    .map_err(|_| CoreError::LengthOverflow)?;
    let expected = 4usize
        .checked_add(
            count
                .checked_mul(FILE_REF_BYTES)
                .ok_or(CoreError::LengthOverflow)?,
        )
        .ok_or(CoreError::LengthOverflow)?;
    if expected != payload.len() {
        return Err(if expected > payload.len() {
            CoreError::UnexpectedEof
        } else {
            CoreError::TrailingBytes
        });
    }
    payload[4..]
        .chunks_exact(FILE_REF_BYTES)
        .map(FileReference::decode)
        .collect()
}

pub fn parse_file_children(payload: &[u8], with_level: bool) -> CoreResult<(u8, Vec<FileChild>)> {
    let prefix = if with_level { 5 } else { 0 };
    if payload.len() < prefix + 4 {
        return Err(CoreError::UnexpectedEof);
    }
    let level = if with_level { payload[0] } else { 0 };
    let count_offset = if with_level { 1 } else { 0 };
    let count = usize::try_from(u32::from_be_bytes(
        payload[count_offset..count_offset + 4]
            .try_into()
            .map_err(|_| CoreError::UnexpectedEof)?,
    ))
    .map_err(|_| CoreError::LengthOverflow)?;
    let expected = prefix
        .checked_add(
            count
                .checked_mul(FILE_DESCRIPTOR_BYTES)
                .ok_or(CoreError::LengthOverflow)?,
        )
        .ok_or(CoreError::LengthOverflow)?;
    if expected != payload.len() {
        return Err(if expected > payload.len() {
            CoreError::UnexpectedEof
        } else {
            CoreError::TrailingBytes
        });
    }
    let start = prefix;
    let children = payload[start..]
        .chunks_exact(FILE_DESCRIPTOR_BYTES)
        .map(FileChild::decode)
        .collect::<CoreResult<Vec<_>>>()?;
    let mut previous = 0_u64;
    for child in &children {
        if child.cumulative_end < previous {
            return Err(CoreError::NonCanonicalOrdering);
        }
        previous = child.cumulative_end;
    }
    Ok((level, children))
}

pub fn parse_file_root(payload: &[u8]) -> CoreResult<(u64, u64, u8, Vec<FileChild>)> {
    if payload.len() < 25 {
        return Err(CoreError::UnexpectedEof);
    }
    let total = u64::from_be_bytes(
        payload[4..12]
            .try_into()
            .map_err(|_| CoreError::UnexpectedEof)?,
    );
    let count_refs = u64::from_be_bytes(
        payload[12..20]
            .try_into()
            .map_err(|_| CoreError::UnexpectedEof)?,
    );
    let level = payload[20];
    let (_, children) = parse_file_children(&payload[20..], true)?;
    let final_end = children.last().map_or(0, |child| child.cumulative_end);
    if final_end != total || (total == 0 && count_refs != 0) {
        return Err(CoreError::LengthMismatch {
            expected: total,
            actual: final_end,
        });
    }
    Ok((total, count_refs, level, children))
}
