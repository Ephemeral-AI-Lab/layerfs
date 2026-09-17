//! Checked canonical codec for extent nodes, file state and chunk payloads.
//!
//! Every encoder validates before it writes and every decoder revalidates the
//! fields it read, so a decoded node is canonical by construction. Lengths are
//! computed with checked arithmetic and compared against the actual value width,
//! which rejects both truncation and trailing bytes.

use crate::error::{ContentError, ContentResult};
use crate::file::cdc;
use crate::file::mapping::types::{
    ChildDescriptor, ExtentNode, ExtentSlice, FileState, MAX_ENTRIES, MAX_LEVEL,
    MAX_NODE_OBJECT_BYTES, MINIMUM_ROOT_ENTRIES, MINIMUM_ROOT_LEAF_ENTRIES, MIN_ENTRIES,
};
use crate::object::{
    canonical_len, codec, encode_bytes_object_to, ObjectId, HEADER_LEN, OBJECT_MAGIC,
};

const MAGIC: &[u8; 8] = b"LFS4MAP\0";
/// Chunk payload magic.
pub const CHUNK_MAGIC: &[u8; 8] = b"LFS4CHK\0";
const VERSION: u16 = 3;
const LEAF: u8 = 0x08;
const BRANCH: u8 = 0x09;
const FILE_STATE: u8 = 0x0a;
const NODE_HEADER_LEN: usize = 31;
const LEAF_ENTRY_LEN: usize = 40;
const BRANCH_ENTRY_LEN: usize = 48;
const FILE_STATE_VALUE_LEN: usize = 93;

/// Frozen mapping profile identity.
pub fn profile_id() -> ObjectId {
    static PROFILE_ID: std::sync::OnceLock<ObjectId> = std::sync::OnceLock::new();
    *PROFILE_ID.get_or_init(|| {
        let mut bytes = Vec::with_capacity(96);
        bytes.extend_from_slice(b"layerfs/mapping-profile/bplus-extent/v3\0");
        bytes.extend_from_slice(&VERSION.to_be_bytes());
        bytes.extend_from_slice(&[LEAF, BRANCH, FILE_STATE, 0]);
        // Every accepted partition bound is hashed through the constant that
        // enforces it, so moving one of them moves this identity.
        bytes.extend_from_slice(&(MIN_ENTRIES as u16).to_be_bytes());
        bytes.extend_from_slice(&(MAX_ENTRIES as u16).to_be_bytes());
        bytes.extend_from_slice(&(MINIMUM_ROOT_LEAF_ENTRIES as u16).to_be_bytes());
        bytes.extend_from_slice(&(MINIMUM_ROOT_ENTRIES as u16).to_be_bytes());
        bytes.push(MAX_LEVEL);
        bytes.extend_from_slice(&(cdc::MAXIMUM_CHUNK_BYTES as u32).to_be_bytes());
        bytes.extend_from_slice(&[4, 4, 8, 1, 1, 1, 1, 1]);
        bytes.extend_from_slice(&cdc::profile_id());
        ObjectId::from_bytes(blake3::hash(&bytes).as_bytes()).expect("BLAKE3 digest width")
    })
}

/// Canonical chunk object: `LFS4CHK\0` followed by the chunk payload.
///
/// The envelope and value header are written into the final allocation once; the
/// payload is copied exactly once, from the scanner buffer into its final home.
pub fn encode_chunk_object(bytes: &[u8]) -> ContentResult<Vec<u8>> {
    if bytes.len() > cdc::MAXIMUM_CHUNK_BYTES {
        return Err(ContentError::ObjectLimitExceeded {
            limit: cdc::MAXIMUM_CHUNK_BYTES,
            actual: bytes.len(),
        });
    }
    let value_len = CHUNK_MAGIC
        .len()
        .checked_add(bytes.len())
        .ok_or(ContentError::LengthOverflow)?;
    // One declaration of the chunk's canonical width, shared with the read path.
    // The payload bound checked above keeps it inside the object field ceiling.
    let canonical_length = chunk_canonical_len(bytes.len());
    let payload_len = u32::try_from(value_len + 4).map_err(|_| ContentError::LengthOverflow)?;
    let mut canonical = Vec::with_capacity(canonical_length);
    canonical.extend_from_slice(&OBJECT_MAGIC);
    canonical.push(codec::BYTES_KIND);
    canonical.extend_from_slice(&payload_len.to_be_bytes());
    canonical.extend_from_slice(
        &u32::try_from(value_len)
            .map_err(|_| ContentError::LengthOverflow)?
            .to_be_bytes(),
    );
    canonical.extend_from_slice(CHUNK_MAGIC);
    canonical.extend_from_slice(bytes);
    debug_assert_eq!(canonical.len(), canonical_length);
    Ok(canonical)
}

/// Borrows the payload of a chunk object value.
pub fn decode_chunk_payload(value: &[u8]) -> ContentResult<&[u8]> {
    let bytes = value
        .strip_prefix(CHUNK_MAGIC)
        .ok_or(ContentError::WrongLogicalRole)?;
    if bytes.len() > cdc::MAXIMUM_CHUNK_BYTES {
        return Err(ContentError::ObjectLimitExceeded {
            limit: cdc::MAXIMUM_CHUNK_BYTES,
            actual: bytes.len(),
        });
    }
    Ok(bytes)
}

/// Encodes one extent-tree page after validating it **in its own context**.
///
/// `root` is the context the page will be decoded in: a non-root page must
/// satisfy the canonical partition, so a short non-root page that this function
/// accepted would be publishable and unreadable. The caller knows the context -
/// it is the one that decides which page is the tree's root - so the encoder asks
/// for it instead of assuming every page it sees is a root.
pub fn encode_node(node: &ExtentNode, root: bool) -> ContentResult<Vec<u8>> {
    node.validate(root)?;
    let entry_len = match node {
        ExtentNode::Leaf { .. } => LEAF_ENTRY_LEN,
        ExtentNode::Branch { .. } => BRANCH_ENTRY_LEN,
    };
    let value_len = NODE_HEADER_LEN
        .checked_add(
            node.entry_count()
                .checked_mul(entry_len)
                .ok_or(ContentError::LengthOverflow)?,
        )
        .ok_or(ContentError::LengthOverflow)?;
    let canonical_length = canonical_len(value_len)?;
    if canonical_length > MAX_NODE_OBJECT_BYTES {
        return Err(ContentError::ObjectLimitExceeded {
            limit: MAX_NODE_OBJECT_BYTES,
            actual: canonical_length,
        });
    }
    let mut value = Vec::with_capacity(value_len);
    value.extend_from_slice(MAGIC);
    value.extend_from_slice(&VERSION.to_be_bytes());
    value.push(if matches!(node, ExtentNode::Leaf { .. }) {
        LEAF
    } else {
        BRANCH
    });
    value.push(node.level());
    value.push(0);
    value.extend_from_slice(
        &u16::try_from(node.entry_count())
            .map_err(|_| ContentError::LengthOverflow)?
            .to_be_bytes(),
    );
    value.extend_from_slice(&node.logical_len().to_be_bytes());
    value.extend_from_slice(&node.extent_count().to_be_bytes());
    match node {
        ExtentNode::Leaf { extents, .. } => {
            for extent in extents {
                value.extend_from_slice(extent.payload_object_id().as_bytes());
                value.extend_from_slice(&extent.source_offset().to_be_bytes());
                value.extend_from_slice(&extent.logical_length().to_be_bytes());
            }
        }
        ExtentNode::Branch { children, .. } => {
            for child in children {
                value.extend_from_slice(&child.cumulative_logical_end.to_be_bytes());
                value.extend_from_slice(&child.cumulative_extent_end.to_be_bytes());
                value.extend_from_slice(child.child_object_id.as_bytes());
            }
        }
    }
    let mut canonical = Vec::with_capacity(canonical_length);
    encode_bytes_object_to(&value, &mut canonical)?;
    Ok(canonical)
}

/// Decodes and validates one extent-tree page.
pub fn decode_node(canonical: &[u8]) -> ContentResult<ExtentNode> {
    decode_node_with_context(canonical, true)
}

/// Decodes a page under its actual root/non-root context.
pub fn decode_node_with_context(canonical: &[u8], root: bool) -> ContentResult<ExtentNode> {
    if canonical.len() > MAX_NODE_OBJECT_BYTES {
        return Err(ContentError::ObjectLimitExceeded {
            limit: MAX_NODE_OBJECT_BYTES,
            actual: canonical.len(),
        });
    }
    let value = codec::decode_bytes_object(canonical)?;
    if value.len() < NODE_HEADER_LEN {
        return Err(ContentError::UnexpectedEof);
    }
    check_prefix(value)?;
    let role = value[10];
    let level = value[11];
    if value[12] != 0 {
        return Err(ContentError::InvalidRecord("extent flags"));
    }
    let count = usize::from(u16::from_be_bytes([value[13], value[14]]));
    let logical = u64::from_be_bytes(
        value[15..23]
            .try_into()
            .map_err(|_| ContentError::UnexpectedEof)?,
    );
    let extent_count = u64::from_be_bytes(
        value[23..31]
            .try_into()
            .map_err(|_| ContentError::UnexpectedEof)?,
    );
    let width = match role {
        LEAF => LEAF_ENTRY_LEN,
        BRANCH => BRANCH_ENTRY_LEN,
        tag => return Err(ContentError::InvalidMappingTag { tag }),
    };
    let expected = NODE_HEADER_LEN
        .checked_add(
            count
                .checked_mul(width)
                .ok_or(ContentError::LengthOverflow)?,
        )
        .ok_or(ContentError::LengthOverflow)?;
    if value.len() < expected {
        return Err(ContentError::UnexpectedEof);
    }
    if value.len() > expected {
        return Err(ContentError::TrailingBytes);
    }
    let node = if role == LEAF {
        if level != 0 || extent_count != count as u64 {
            return Err(ContentError::InvalidRecord("extent leaf header"));
        }
        let mut extents = Vec::with_capacity(count);
        for entry in value[NODE_HEADER_LEN..].chunks_exact(LEAF_ENTRY_LEN) {
            extents.push(ExtentSlice::new(
                ObjectId::from_bytes(&entry[..32])?,
                u32::from_be_bytes(
                    entry[32..36]
                        .try_into()
                        .map_err(|_| ContentError::UnexpectedEof)?,
                ),
                u32::from_be_bytes(
                    entry[36..40]
                        .try_into()
                        .map_err(|_| ContentError::UnexpectedEof)?,
                ),
            )?);
        }
        ExtentNode::Leaf {
            subtree_logical_bytes: logical,
            extents,
        }
    } else {
        if level == 0 {
            return Err(ContentError::InvalidRecord("extent branch level"));
        }
        let mut children = Vec::with_capacity(count);
        for entry in value[NODE_HEADER_LEN..].chunks_exact(BRANCH_ENTRY_LEN) {
            children.push(ChildDescriptor {
                cumulative_logical_end: u64::from_be_bytes(
                    entry[..8]
                        .try_into()
                        .map_err(|_| ContentError::UnexpectedEof)?,
                ),
                cumulative_extent_end: u64::from_be_bytes(
                    entry[8..16]
                        .try_into()
                        .map_err(|_| ContentError::UnexpectedEof)?,
                ),
                child_object_id: ObjectId::from_bytes(&entry[16..48])?,
            });
        }
        ExtentNode::Branch {
            level,
            subtree_logical_bytes: logical,
            subtree_extent_count: extent_count,
            children,
        }
    };
    node.validate(root)?;
    Ok(node)
}

/// Encodes a file-state root after validating it against the frozen profile.
pub fn encode_file_state(state: FileState) -> ContentResult<Vec<u8>> {
    if state.profile_id != profile_id() || state.tree_level > crate::file::mapping::types::MAX_LEVEL
    {
        return Err(ContentError::InvalidRecord("file state profile"));
    }
    let mut value = Vec::with_capacity(FILE_STATE_VALUE_LEN);
    value.extend_from_slice(MAGIC);
    value.extend_from_slice(&VERSION.to_be_bytes());
    value.extend_from_slice(&[FILE_STATE, 0]);
    value.extend_from_slice(&state.logical_len.to_be_bytes());
    value.extend_from_slice(&state.extent_count.to_be_bytes());
    value.push(state.tree_level);
    value.extend_from_slice(state.profile_id.as_bytes());
    value.extend_from_slice(state.mapping_root.as_bytes());
    let canonical = crate::object::encode_bytes_object(&value)?;
    Ok(canonical)
}

/// Decodes a file state, checking width, version, profile and depth.
pub fn decode_file_state(canonical: &[u8]) -> ContentResult<FileState> {
    let value = codec::decode_bytes_object(canonical)?;
    if value.len() < FILE_STATE_VALUE_LEN {
        return Err(ContentError::UnexpectedEof);
    }
    if value.len() > FILE_STATE_VALUE_LEN {
        return Err(ContentError::TrailingBytes);
    }
    check_prefix(value)?;
    if value[10] != FILE_STATE {
        return Err(ContentError::WrongLogicalRole);
    }
    if value[11] != 0 {
        return Err(ContentError::InvalidRecord("file state flags"));
    }
    let state = FileState {
        logical_len: u64::from_be_bytes(
            value[12..20]
                .try_into()
                .map_err(|_| ContentError::UnexpectedEof)?,
        ),
        extent_count: u64::from_be_bytes(
            value[20..28]
                .try_into()
                .map_err(|_| ContentError::UnexpectedEof)?,
        ),
        tree_level: value[28],
        profile_id: ObjectId::from_bytes(&value[29..61])?,
        mapping_root: ObjectId::from_bytes(&value[61..93])?,
    };
    if state.profile_id != profile_id() {
        return Err(ContentError::InvalidRecord("file state profile"));
    }
    if state.tree_level > crate::file::mapping::types::MAX_LEVEL {
        return Err(ContentError::MappingDepthExceeded);
    }
    Ok(state)
}

fn check_prefix(value: &[u8]) -> ContentResult<()> {
    if &value[..MAGIC.len()] != MAGIC {
        return Err(ContentError::UnsupportedFraming);
    }
    let version = u16::from_be_bytes([value[8], value[9]]);
    if version != VERSION {
        return Err(ContentError::UnsupportedMappingVersion { version });
    }
    Ok(())
}

/// Canonical chunk object width for a payload of `raw` bytes.
pub const fn chunk_canonical_len(raw: usize) -> usize {
    HEADER_LEN + 4 + CHUNK_MAGIC.len() + raw
}
