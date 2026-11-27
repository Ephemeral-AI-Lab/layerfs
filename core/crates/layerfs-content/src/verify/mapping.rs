//! Actual payload slices and direct extent summaries.

use crate::error::{ContentError, ContentResult};
use crate::file::mapping::{self, ExtentNode, FileState};
use crate::object::{codec, AuthenticatedObjects};

pub(super) fn node(canonical: &[u8], reader: &dyn AuthenticatedObjects) -> ContentResult<()> {
    match mapping::decode_node(canonical)? {
        ExtentNode::Leaf { extents, .. } => {
            for extent in extents {
                let canonical = reader.read_canonical(extent.payload_object_id())?;
                let payload =
                    mapping::decode_chunk_payload(codec::decode_bytes_object(&canonical)?)?;
                let end = extent
                    .source_offset()
                    .checked_add(extent.logical_length())
                    .ok_or(ContentError::LengthOverflow)?;
                if u64::from(end) > payload.len() as u64 {
                    return Err(ContentError::InvalidRecord("extent payload slice"));
                }
            }
            Ok(())
        }
        ExtentNode::Branch {
            level, children, ..
        } => {
            let mut bytes = 0_u64;
            let mut extents = 0_u64;
            for child in children {
                let canonical = reader.read_canonical(child.child_object_id)?;
                let node = mapping::decode_node_with_context(&canonical, false)?;
                if node.level().checked_add(1) != Some(level) {
                    return Err(ContentError::InvalidRecord("extent child level"));
                }
                bytes = bytes
                    .checked_add(node.logical_len())
                    .ok_or(ContentError::LengthOverflow)?;
                extents = extents
                    .checked_add(node.extent_count())
                    .ok_or(ContentError::LengthOverflow)?;
                if bytes != child.cumulative_logical_end || extents != child.cumulative_extent_end {
                    return Err(ContentError::InvalidRecord("extent child summary"));
                }
            }
            Ok(())
        }
    }
}

pub(super) fn state(
    canonical: &[u8],
    reader: &dyn AuthenticatedObjects,
) -> ContentResult<FileState> {
    let state = mapping::decode_file_state(canonical)?;
    let canonical = reader.read_canonical(state.mapping_root)?;
    let root = mapping::decode_node_with_context(&canonical, true)?;
    if root.level() != state.tree_level
        || root.logical_len() != state.logical_len
        || root.extent_count() != state.extent_count
    {
        return Err(ContentError::InvalidRecord("file state root summary"));
    }
    Ok(state)
}
