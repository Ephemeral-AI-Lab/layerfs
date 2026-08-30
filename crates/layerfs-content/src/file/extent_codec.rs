use super::extent::{
    ChildDescriptorV3, ExtentNodeV3, ExtentSliceV3, FileStateV3, MAX_NODE_OBJECT_BYTES,
};
use crate::{decode_bytes_object, encode_bytes_object, CoreError, CoreResult, ObjectId};

const MAGIC: &[u8; 8] = b"LFS4MAP\0";
pub const CHUNK_MAGIC: &[u8; 8] = b"LFS4CHK\0";
const VERSION: u16 = 3;
const LEAF: u8 = 0x08;
const BRANCH: u8 = 0x09;
const FILE_STATE: u8 = 0x0a;

pub fn encode_chunk_object(bytes: &[u8]) -> CoreResult<Vec<u8>> {
    if bytes.len() > crate::file::cdc::MAXIMUM_CHUNK_BYTES {
        return Err(CoreError::ObjectLimitExceeded);
    }
    let mut value = Vec::with_capacity(CHUNK_MAGIC.len() + bytes.len());
    value.extend_from_slice(CHUNK_MAGIC);
    value.extend_from_slice(bytes);
    encode_bytes_object(&value)
}

pub fn decode_chunk_payload(value: &[u8]) -> CoreResult<&[u8]> {
    let bytes = value
        .strip_prefix(CHUNK_MAGIC)
        .ok_or(CoreError::WrongLogicalRole)?;
    if bytes.len() > crate::file::cdc::MAXIMUM_CHUNK_BYTES {
        return Err(CoreError::ChunkLengthMismatch);
    }
    Ok(bytes)
}

pub fn profile_id() -> ObjectId {
    let mut bytes = Vec::with_capacity(96);
    bytes.extend_from_slice(b"layerfs/mapping-profile/bplus-extent/v3\0");
    bytes.extend_from_slice(&VERSION.to_be_bytes());
    bytes.extend_from_slice(&[LEAF, BRANCH, FILE_STATE, 0]);
    bytes.extend_from_slice(&64_u16.to_be_bytes());
    bytes.extend_from_slice(&128_u16.to_be_bytes());
    bytes.extend_from_slice(&0_u16.to_be_bytes());
    bytes.extend_from_slice(&2_u16.to_be_bytes());
    bytes.push(31);
    bytes.extend_from_slice(&32_768_u32.to_be_bytes());
    bytes.extend_from_slice(&[4, 4, 8, 1, 1, 1, 1, 1]);
    bytes.extend_from_slice(&crate::file::cdc::profile_id());
    ObjectId::from_bytes(blake3::hash(&bytes).as_bytes()).expect("BLAKE3 digest width")
}

pub fn encode_node(node: &ExtentNodeV3) -> CoreResult<Vec<u8>> {
    node.validate(true)?;
    let mut value = Vec::new();
    value.extend_from_slice(MAGIC);
    value.extend_from_slice(&VERSION.to_be_bytes());
    value.push(if matches!(node, ExtentNodeV3::Leaf { .. }) {
        LEAF
    } else {
        BRANCH
    });
    value.push(node.level());
    value.push(0);
    value.extend_from_slice(&(node.entry_count() as u16).to_be_bytes());
    value.extend_from_slice(&node.logical_len().to_be_bytes());
    value.extend_from_slice(&node.extent_count().to_be_bytes());
    match node {
        ExtentNodeV3::Leaf { extents, .. } => {
            for extent in extents {
                value.extend_from_slice(extent.payload_object_id.as_bytes());
                value.extend_from_slice(&extent.source_offset.to_be_bytes());
                value.extend_from_slice(&extent.logical_length.to_be_bytes());
            }
        }
        ExtentNodeV3::Branch { children, .. } => {
            for child in children {
                value.extend_from_slice(&child.cumulative_logical_end.to_be_bytes());
                value.extend_from_slice(&child.cumulative_extent_end.to_be_bytes());
                value.extend_from_slice(child.child_object_id.as_bytes());
            }
        }
    }
    let canonical = encode_bytes_object(&value)?;
    if canonical.len() > MAX_NODE_OBJECT_BYTES {
        return Err(CoreError::ObjectLimitExceeded);
    }
    Ok(canonical)
}

pub fn decode_node(canonical: &[u8]) -> CoreResult<ExtentNodeV3> {
    decode_node_with_context(canonical, true)
}

pub fn decode_node_with_context(canonical: &[u8], root: bool) -> CoreResult<ExtentNodeV3> {
    let value = decode_bytes_object(canonical)?;
    if canonical.len() > MAX_NODE_OBJECT_BYTES {
        return Err(CoreError::ObjectLimitExceeded);
    }
    if value.len() < 31 {
        return Err(CoreError::UnexpectedEof);
    }
    check_prefix(value)?;
    let role = value[10];
    let level = value[11];
    if value[12] != 0 {
        return Err(CoreError::InvalidRecord("extent flags"));
    }
    let count = usize::from(u16::from_be_bytes([value[13], value[14]]));
    let logical = u64::from_be_bytes(value[15..23].try_into().unwrap());
    let extent_count = u64::from_be_bytes(value[23..31].try_into().unwrap());
    let width = match role {
        LEAF => 40,
        BRANCH => 48,
        tag => return Err(CoreError::InvalidMappingTag { tag }),
    };
    let expected = 31_usize
        .checked_add(count.checked_mul(width).ok_or(CoreError::LengthOverflow)?)
        .ok_or(CoreError::LengthOverflow)?;
    if value.len() < expected {
        return Err(CoreError::UnexpectedEof);
    }
    if value.len() > expected {
        return Err(CoreError::TrailingBytes);
    }
    let node = if role == LEAF {
        if level != 0 || extent_count != count as u64 {
            return Err(CoreError::InvalidRecord("extent leaf header"));
        }
        let mut extents = Vec::with_capacity(count);
        for entry in value[31..].chunks_exact(40) {
            extents.push(ExtentSliceV3::new(
                ObjectId::from_bytes(&entry[..32])?,
                u32::from_be_bytes(entry[32..36].try_into().unwrap()),
                u32::from_be_bytes(entry[36..40].try_into().unwrap()),
            )?);
        }
        ExtentNodeV3::Leaf {
            subtree_logical_bytes: logical,
            extents,
        }
    } else {
        if level == 0 {
            return Err(CoreError::InvalidRecord("extent branch level"));
        }
        let mut children = Vec::with_capacity(count);
        for entry in value[31..].chunks_exact(48) {
            children.push(ChildDescriptorV3 {
                cumulative_logical_end: u64::from_be_bytes(entry[..8].try_into().unwrap()),
                cumulative_extent_end: u64::from_be_bytes(entry[8..16].try_into().unwrap()),
                child_object_id: ObjectId::from_bytes(&entry[16..48])?,
            });
        }
        ExtentNodeV3::Branch {
            level,
            subtree_logical_bytes: logical,
            subtree_extent_count: extent_count,
            children,
        }
    };
    node.validate(root)?;
    Ok(node)
}

pub fn encode_file_state(state: FileStateV3) -> CoreResult<Vec<u8>> {
    if state.profile_id != profile_id() || state.tree_level > 31 {
        return Err(CoreError::ProfileMismatch);
    }
    let mut value = Vec::with_capacity(93);
    value.extend_from_slice(MAGIC);
    value.extend_from_slice(&VERSION.to_be_bytes());
    value.extend_from_slice(&[FILE_STATE, 0]);
    value.extend_from_slice(&state.logical_len.to_be_bytes());
    value.extend_from_slice(&state.extent_count.to_be_bytes());
    value.push(state.tree_level);
    value.extend_from_slice(state.profile_id.as_bytes());
    value.extend_from_slice(state.mapping_root.as_bytes());
    encode_bytes_object(&value)
}

pub fn decode_file_state(canonical: &[u8]) -> CoreResult<FileStateV3> {
    let value = decode_bytes_object(canonical)?;
    if value.len() < 93 {
        return Err(CoreError::UnexpectedEof);
    }
    if value.len() > 93 {
        return Err(CoreError::TrailingBytes);
    }
    check_prefix(value)?;
    if value[10] != FILE_STATE {
        return Err(CoreError::WrongLogicalRole);
    }
    if value[11] != 0 {
        return Err(CoreError::InvalidRecord("file state flags"));
    }
    let state = FileStateV3 {
        logical_len: u64::from_be_bytes(value[12..20].try_into().unwrap()),
        extent_count: u64::from_be_bytes(value[20..28].try_into().unwrap()),
        tree_level: value[28],
        profile_id: ObjectId::from_bytes(&value[29..61])?,
        mapping_root: ObjectId::from_bytes(&value[61..93])?,
    };
    if state.profile_id != profile_id() {
        return Err(CoreError::ProfileMismatch);
    }
    if state.tree_level > 31 {
        return Err(CoreError::MappingDepthExceeded);
    }
    Ok(state)
}

fn check_prefix(value: &[u8]) -> CoreResult<()> {
    if &value[..8] != MAGIC {
        return Err(CoreError::Unsupported);
    }
    let version = u16::from_be_bytes([value[8], value[9]]);
    if version != VERSION {
        return Err(CoreError::UnsupportedMappingVersion { version });
    }
    Ok(())
}
