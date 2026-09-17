//! Extent-only attribute value roots built from existing mapping bodies.
//!
//! An attribute value is a complete file representation, always carrying an
//! extent leaf: the regular-file small/large cutoff must not reclassify it, so a
//! four-byte mode value is stored exactly like any other extent-backed value. The
//! chunk payload, leaf and file-state objects are the existing canonical
//! grammars; no second CDC path or metadata-specific file format is introduced.

use crate::error::{ContentError, ContentResult};
use crate::file::mapping::{
    encode_chunk_object, encode_file_state, encode_node, profile_id, ExtentNode, ExtentSlice,
    FileState,
};
use crate::filesystem::objects::FilesystemObjects;
use crate::object::{FinalizedObject, ObjectId, ObjectRole};

/// Emits one extent-only value root and returns its file-state identity.
///
/// The declared value bound applies here and not only on the read side: a value
/// this function accepts is one a bounded read can return whole, so a caller
/// cannot create an attribute the read path would refuse to hand back.
pub fn emit_value(objects: &mut FilesystemObjects<'_>, bytes: &[u8]) -> ContentResult<ObjectId> {
    if bytes.is_empty() {
        return Err(ContentError::InvalidRecord("attribute value"));
    }
    if bytes.len() > crate::filesystem::limits::MAXIMUM_ATTRIBUTE_VALUE_BYTES {
        return Err(ContentError::ObjectLimitExceeded {
            limit: crate::filesystem::limits::MAXIMUM_ATTRIBUTE_VALUE_BYTES,
            actual: bytes.len(),
        });
    }
    let payload = FinalizedObject::new(ObjectRole::Chunk, encode_chunk_object(bytes)?)?;
    let payload_id = objects.emit(payload)?;
    let length = u32::try_from(bytes.len()).map_err(|_| ContentError::LengthOverflow)?;
    let leaf = FinalizedObject::new(
        ObjectRole::ExtentLeaf,
        encode_node(&ExtentNode::Leaf {
            subtree_logical_bytes: u64::from(length),
            extents: vec![ExtentSlice::new(payload_id, 0, length)?],
        })?,
    )?
    .with_references(vec![payload_id]);
    let leaf_id = objects.emit(leaf)?;
    let state = FinalizedObject::new(
        ObjectRole::FileState,
        encode_file_state(FileState {
            logical_len: u64::from(length),
            extent_count: 1,
            tree_level: 0,
            profile_id: profile_id(),
            mapping_root: leaf_id,
        })?,
    )?
    .with_references(vec![leaf_id]);
    objects.emit(state)
}

/// Reads one attribute value root completely, bounded by its declared length.
pub fn read_value(
    reader: &dyn crate::object::AuthenticatedObjects,
    root: ObjectId,
    maximum_bytes: usize,
) -> ContentResult<Vec<u8>> {
    use crate::file::mapping::{decode_file_state, read_range};
    let canonical = reader.read_canonical(root)?;
    let state = decode_file_state(&canonical)?;
    let length = usize::try_from(state.logical_len).map_err(|_| ContentError::LengthOverflow)?;
    if length > maximum_bytes {
        return Err(ContentError::ObjectLimitExceeded {
            limit: maximum_bytes,
            actual: length,
        });
    }
    let mut output = Vec::with_capacity(length);
    // An attribute value read has no timing scope of its own yet, so its mapping
    // waves run under a disabled node rather than under the caller's duration.
    layerfs_telemetry::timer::Timing::disabled("attributes.value", |scope| {
        read_range(reader, state, 0..length as u64, &mut output, scope)
    })
    .0?;
    if output.len() != length {
        return Err(ContentError::LengthMismatch {
            expected: state.logical_len,
            actual: output.len() as u64,
        });
    }
    Ok(output)
}
