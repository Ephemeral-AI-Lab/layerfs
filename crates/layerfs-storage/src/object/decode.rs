//! Exact borrowed decoder for the five frozen `ELSOBJ01` object kinds.
//!
//! The decoder validates declared lengths before entering any payload loop,
//! retains only borrowed slices and scalar state, and reports strong edges one
//! at a time through a transactional visitor. It never constructs an edge
//! vector or a source-sized allocation.

use crate::format::{
    compare_unsigned, validate_chunk_object_count, validate_chunk_reference_len,
    validate_chunk_refs_per_file, validate_chunk_refs_per_version, validate_entry_count,
    validate_extents_per_file, validate_extents_per_version, validate_file_mode,
    validate_logical_length, validate_physical_chunk_payload_len, validate_physical_object_len,
    validate_total_object_count, validate_tree_index_fanout, validate_tree_leaf_fanout,
    validate_tree_object_count, ExtentTagV1, PhysicalObjectKindV1, PhysicalTreeChildKindV1,
    PresenceV1, SliceCursor, TreeSubtypeV1, ValidatedComponent, ValidatedSymlinkTarget,
    MAX_ENTRIES, MAX_LOGICAL_BYTES, MAX_TREE_PAGE_DEPTH, ROOT_DIRECTORY_MODE_SENTINEL_V1,
};
use crate::identity::{
    ChunkerSpecId, DigestSpecId, PhysicalChunkIdV1, PhysicalFileIdV1, PhysicalSymlinkIdV1,
    PhysicalTreeIdV1, ProfileId, VersionIdV1,
};
use crate::profile::ProfileSpecV1;
use crate::{CoreError, CoreResult};

use super::encode::{CANONICAL_OBJECT_MAGIC_V1, OBJECT_HEADER_BYTES, VERSION_RECORD_PAYLOAD_BYTES};
use super::model::*;

pub(super) const TREE_MIN_PAYLOAD_BYTES: u64 = 9;
pub(super) const FILE_MIN_PAYLOAD_BYTES: u64 = 14;
pub(super) const SYMLINK_MIN_PAYLOAD_BYTES: u64 = 5;
pub(super) const MIN_TREE_LEAF_ENTRY_BYTES: u64 = 36;
pub(super) const MIN_TREE_INDEX_ENTRY_BYTES: u64 = 42;
pub(super) const MIN_FILE_EXTENT_BYTES: u64 = 9;
pub(super) const CHUNK_REFERENCE_BYTES: u64 = 36;
pub(super) const HOLE_MIN_BYTES: u64 = 65_536;

pub fn decode_physical_object_v1<'a, V: StrongEdgeVisitorV1 + ?Sized>(
    bytes: &'a [u8],
    visitor: &mut V,
) -> CoreResult<ValidatedPhysicalObjectV1<'a>> {
    let (header, payload_bytes) = decode_envelope(bytes)?;
    visitor.begin_object();
    let decoded = decode_payload(header.kind, payload_bytes, visitor);
    match decoded {
        Ok(payload) => {
            visitor.commit_object();
            Ok(ValidatedPhysicalObjectV1 {
                header,
                payload,
                canonical_object: bytes,
            })
        }
        Err(error) => {
            visitor.abort_object();
            Err(error)
        }
    }
}

fn decode_envelope(bytes: &[u8]) -> CoreResult<(PhysicalObjectHeaderV1, &[u8])> {
    let mut cursor = SliceCursor::new(bytes);
    cursor.expect(CANONICAL_OBJECT_MAGIC_V1, CoreError::TypeDomain)?;
    if cursor.read_u16_be()? != 1 {
        return Err(CoreError::Schema);
    }
    let kind = PhysicalObjectKindV1::try_from(cursor.read_u8()?)?;
    if cursor.read_u8()? != 0 {
        return Err(CoreError::Flags);
    }
    let profile_id = ProfileId::from_digest(*cursor.read_array::<32>()?);
    if profile_id != ProfileSpecV1::frozen().id() {
        return Err(CoreError::TypeDomain);
    }
    let payload_len = cursor.read_u64_be()?;
    let complete_len = OBJECT_HEADER_BYTES
        .checked_add(payload_len)
        .ok_or(CoreError::IntegerOverflow)?;
    validate_physical_object_len(complete_len)?;
    validate_payload_bound(kind, payload_len)?;
    let complete_len_usize =
        usize::try_from(complete_len).map_err(|_| CoreError::IntegerOverflow)?;
    if bytes.len() < complete_len_usize {
        return Err(CoreError::Truncated);
    }
    if bytes.len() > complete_len_usize {
        return Err(CoreError::TrailingBytes);
    }
    let payload_len_usize = usize::try_from(payload_len).map_err(|_| CoreError::IntegerOverflow)?;
    let payload = cursor.read_bytes(payload_len_usize)?;
    cursor.finish()?;
    Ok((
        PhysicalObjectHeaderV1 {
            kind,
            profile_id,
            payload_len,
            complete_len,
        },
        payload,
    ))
}

pub(super) fn validate_payload_bound(
    kind: PhysicalObjectKindV1,
    payload_len: u64,
) -> CoreResult<()> {
    match kind {
        PhysicalObjectKindV1::VersionRecord if payload_len != VERSION_RECORD_PAYLOAD_BYTES => {
            Err(CoreError::LogicalLength)
        }
        PhysicalObjectKindV1::Tree if payload_len < TREE_MIN_PAYLOAD_BYTES => {
            Err(CoreError::LogicalLength)
        }
        PhysicalObjectKindV1::File if payload_len < FILE_MIN_PAYLOAD_BYTES => {
            Err(CoreError::LogicalLength)
        }
        PhysicalObjectKindV1::Symlink if payload_len < SYMLINK_MIN_PAYLOAD_BYTES => {
            Err(CoreError::LogicalLength)
        }
        PhysicalObjectKindV1::Chunk => validate_physical_chunk_payload_len(payload_len),
        _ => Ok(()),
    }
}

fn decode_payload<V: StrongEdgeVisitorV1 + ?Sized>(
    kind: PhysicalObjectKindV1,
    payload: &[u8],
    visitor: &mut V,
) -> CoreResult<PhysicalObjectPayloadV1> {
    match kind {
        PhysicalObjectKindV1::VersionRecord => {
            decode_version(payload, visitor).map(PhysicalObjectPayloadV1::VersionRecord)
        }
        PhysicalObjectKindV1::Tree => {
            decode_tree(payload, visitor).map(PhysicalObjectPayloadV1::Tree)
        }
        PhysicalObjectKindV1::File => {
            decode_file(payload, visitor).map(PhysicalObjectPayloadV1::File)
        }
        PhysicalObjectKindV1::Symlink => {
            decode_symlink(payload).map(PhysicalObjectPayloadV1::Symlink)
        }
        PhysicalObjectKindV1::Chunk => {
            let payload_len =
                u32::try_from(payload.len()).map_err(|_| CoreError::IntegerOverflow)?;
            Ok(PhysicalObjectPayloadV1::Chunk(ChunkRecordV1 {
                payload_len,
            }))
        }
    }
}

fn decode_version<V: StrongEdgeVisitorV1 + ?Sized>(
    payload: &[u8],
    visitor: &mut V,
) -> CoreResult<VersionRecordV1> {
    let mut cursor = SliceCursor::new(payload);
    let version_id = VersionIdV1::from_digest(*cursor.read_array::<32>()?);
    let chunker_spec_id = ChunkerSpecId::from_digest(*cursor.read_array::<32>()?);
    let digest_spec_id = DigestSpecId::from_digest(*cursor.read_array::<32>()?);
    let root_tree_id = PhysicalTreeIdV1::from_digest(*cursor.read_array::<32>()?);
    visitor.visit_edge(StrongEdgeV1::Tree(root_tree_id))?;
    let canonical_len = cursor.read_u64_be()?;
    validate_logical_length(canonical_len)?;
    let logical_file_bytes = cursor.read_u64_be()?;
    validate_logical_length(logical_file_bytes)?;
    let entry_count = cursor.read_u32_be()?;
    validate_entry_count(u64::from(entry_count))?;
    let tree_count = cursor.read_u32_be()?;
    validate_tree_object_count(u64::from(tree_count))?;
    let file_count = cursor.read_u32_be()?;
    validate_entry_count(u64::from(file_count))?;
    let symlink_count = cursor.read_u32_be()?;
    validate_entry_count(u64::from(symlink_count))?;
    let chunk_count = cursor.read_u32_be()?;
    validate_chunk_object_count(u64::from(chunk_count))?;
    let extent_count = cursor.read_u32_be()?;
    validate_extents_per_version(u64::from(extent_count))?;
    let chunk_ref_count = cursor.read_u32_be()?;
    validate_chunk_refs_per_version(u64::from(chunk_ref_count))?;
    let total_object_count = cursor.read_u32_be()?;
    validate_total_object_count(u64::from(total_object_count))?;
    let physical_chunk_bytes = cursor.read_u64_be()?;
    validate_logical_length(physical_chunk_bytes)?;
    cursor.finish()?;
    Ok(VersionRecordV1 {
        version_id,
        chunker_spec_id,
        digest_spec_id,
        root_tree_id,
        canonical_len,
        logical_file_bytes,
        entry_count,
        tree_count,
        file_count,
        symlink_count,
        chunk_count,
        extent_count,
        chunk_ref_count,
        total_object_count,
        physical_chunk_bytes,
    })
}

fn decode_tree<V: StrongEdgeVisitorV1 + ?Sized>(
    payload: &[u8],
    visitor: &mut V,
) -> CoreResult<TreeRecordV1> {
    let mut cursor = SliceCursor::new(payload);
    let subtype = TreeSubtypeV1::try_from(cursor.read_u8()?)?;
    let value = match subtype {
        TreeSubtypeV1::Directory => {
            TreeRecordV1::Directory(decode_directory(&mut cursor, visitor)?)
        }
        TreeSubtypeV1::Leaf => TreeRecordV1::Leaf(decode_leaf(&mut cursor, visitor)?),
        TreeSubtypeV1::Index => TreeRecordV1::Index(decode_index(&mut cursor, visitor)?),
    };
    cursor.finish()?;
    Ok(value)
}

fn decode_directory<V: StrongEdgeVisitorV1 + ?Sized>(
    cursor: &mut SliceCursor<'_>,
    visitor: &mut V,
) -> CoreResult<DirectoryTreeRecordV1> {
    let mode = cursor.read_u16_be()?;
    if mode > ROOT_DIRECTORY_MODE_SENTINEL_V1 {
        return Err(CoreError::FileMode);
    }
    let entry_count = cursor.read_u32_be()?;
    validate_entry_count(u64::from(entry_count))?;
    let page_depth = cursor.read_u8()?;
    if u64::from(page_depth) > MAX_TREE_PAGE_DEPTH {
        return Err(CoreError::CountCap);
    }
    let presence = PresenceV1::try_from(cursor.read_u8()?)?;
    let expected_presence = if entry_count == 0 {
        PresenceV1::Absent
    } else {
        PresenceV1::Present
    };
    if presence != expected_presence {
        return Err(CoreError::TypedEdge);
    }
    let root_page_id = if presence == PresenceV1::Present {
        let id = PhysicalTreeIdV1::from_digest(*cursor.read_array::<32>()?);
        visitor.visit_edge(StrongEdgeV1::Tree(id))?;
        Some(id)
    } else {
        None
    };
    let minimum_depth = match entry_count {
        0..=192 => 0,
        193..=18_432 => 1,
        _ => 2,
    };
    if page_depth != minimum_depth {
        return Err(CoreError::NonCanonicalOrder);
    }
    Ok(DirectoryTreeRecordV1 {
        mode,
        entry_count,
        page_depth,
        root_page_id,
    })
}

fn decode_leaf<V: StrongEdgeVisitorV1 + ?Sized>(
    cursor: &mut SliceCursor<'_>,
    visitor: &mut V,
) -> CoreResult<LeafTreeRecordV1> {
    let depth = cursor.read_u8()?;
    if depth != 0 {
        return Err(CoreError::CountCap);
    }
    let count = cursor.read_u16_be()?;
    validate_tree_leaf_fanout(u64::from(count))?;
    require_repetition(cursor, u64::from(count), MIN_TREE_LEAF_ENTRY_BYTES)?;
    let mut previous: Option<&[u8]> = None;
    for _ in 0..count {
        let name = read_component(cursor)?;
        if previous.is_some_and(|prior| {
            compare_unsigned(prior, name.as_bytes()) != core::cmp::Ordering::Less
        }) {
            return Err(CoreError::NonCanonicalOrder);
        }
        previous = Some(name.as_bytes());
        let kind = PhysicalTreeChildKindV1::try_from(cursor.read_u8()?)?;
        let raw_id = *cursor.read_array::<32>()?;
        let edge = match kind {
            PhysicalTreeChildKindV1::Tree => {
                StrongEdgeV1::Tree(PhysicalTreeIdV1::from_digest(raw_id))
            }
            PhysicalTreeChildKindV1::File => {
                StrongEdgeV1::File(PhysicalFileIdV1::from_digest(raw_id))
            }
            PhysicalTreeChildKindV1::Symlink => {
                StrongEdgeV1::Symlink(PhysicalSymlinkIdV1::from_digest(raw_id))
            }
        };
        visitor.visit_edge(edge)?;
    }
    Ok(LeafTreeRecordV1 { depth, count })
}

fn decode_index<V: StrongEdgeVisitorV1 + ?Sized>(
    cursor: &mut SliceCursor<'_>,
    visitor: &mut V,
) -> CoreResult<IndexTreeRecordV1> {
    let depth = cursor.read_u8()?;
    if depth == 0 || u64::from(depth) > MAX_TREE_PAGE_DEPTH {
        return Err(CoreError::CountCap);
    }
    let count = cursor.read_u16_be()?;
    validate_tree_index_fanout(u64::from(count))?;
    require_repetition(cursor, u64::from(count), MIN_TREE_INDEX_ENTRY_BYTES)?;
    let child_capacity = if depth == 1 { 192 } else { 18_432 };
    let mut previous_last: Option<&[u8]> = None;
    let mut total = 0_u64;
    for index in 0..count {
        let subtree_entry_count = cursor.read_u32_be()?;
        if subtree_entry_count == 0 || u64::from(subtree_entry_count) > MAX_ENTRIES {
            return Err(CoreError::CountCap);
        }
        let first = read_component(cursor)?;
        if previous_last.is_some_and(|prior| {
            compare_unsigned(prior, first.as_bytes()) != core::cmp::Ordering::Less
        }) {
            return Err(CoreError::NonCanonicalOrder);
        }
        let last = read_component(cursor)?;
        if compare_unsigned(first.as_bytes(), last.as_bytes()).is_gt() {
            return Err(CoreError::NonCanonicalOrder);
        }
        let child = PhysicalTreeIdV1::from_digest(*cursor.read_array::<32>()?);
        let final_child = index + 1 == count;
        if (!final_child && subtree_entry_count != child_capacity)
            || (final_child && subtree_entry_count > child_capacity)
        {
            return Err(CoreError::NonCanonicalOrder);
        }
        total = total
            .checked_add(u64::from(subtree_entry_count))
            .ok_or(CoreError::IntegerOverflow)?;
        validate_entry_count(total)?;
        previous_last = Some(last.as_bytes());
        visitor.visit_edge(StrongEdgeV1::Tree(child))?;
    }
    if depth == 2 && total <= 18_432 {
        return Err(CoreError::NonCanonicalOrder);
    }
    Ok(IndexTreeRecordV1 {
        depth,
        count,
        subtree_entry_count: total,
    })
}

fn decode_file<V: StrongEdgeVisitorV1 + ?Sized>(
    payload: &[u8],
    visitor: &mut V,
) -> CoreResult<FileRecordV1> {
    let mut cursor = SliceCursor::new(payload);
    let mode = cursor.read_u16_be()?;
    validate_file_mode(mode)?;
    let logical_len = cursor.read_u64_be()?;
    validate_logical_length(logical_len)?;
    let extent_count = cursor.read_u32_be()?;
    validate_extents_per_file(u64::from(extent_count))?;
    require_repetition(&cursor, u64::from(extent_count), MIN_FILE_EXTENT_BYTES)?;
    let mut previous_tag = None;
    let mut coverage = 0_u64;
    let mut total_chunk_refs = 0_u64;
    for _ in 0..extent_count {
        let tag = ExtentTagV1::try_from(cursor.read_u8()?)?;
        if previous_tag == Some(tag) {
            return Err(CoreError::NonCanonicalOrder);
        }
        previous_tag = Some(tag);
        let length = cursor.read_u64_be()?;
        match tag {
            ExtentTagV1::Hole => {
                if !(HOLE_MIN_BYTES..=MAX_LOGICAL_BYTES).contains(&length) {
                    return Err(CoreError::LogicalLength);
                }
            }
            ExtentTagV1::Data => {
                if length == 0 || length > MAX_LOGICAL_BYTES {
                    return Err(CoreError::LogicalLength);
                }
                let count = cursor.read_u32_be()?;
                if count == 0 {
                    return Err(CoreError::CountCap);
                }
                validate_chunk_refs_per_file(u64::from(count))?;
                require_repetition(&cursor, u64::from(count), CHUNK_REFERENCE_BYTES)?;
                total_chunk_refs = total_chunk_refs
                    .checked_add(u64::from(count))
                    .ok_or(CoreError::IntegerOverflow)?;
                validate_chunk_refs_per_file(total_chunk_refs)?;
                let mut reconstructed = 0_u64;
                for _ in 0..count {
                    let chunk_len = cursor.read_u32_be()?;
                    validate_chunk_reference_len(u64::from(chunk_len))?;
                    reconstructed = reconstructed
                        .checked_add(u64::from(chunk_len))
                        .ok_or(CoreError::IntegerOverflow)?;
                    let id = PhysicalChunkIdV1::from_digest(*cursor.read_array::<32>()?);
                    visitor.visit_edge(StrongEdgeV1::Chunk(id))?;
                }
                if reconstructed != length {
                    return Err(CoreError::LogicalLength);
                }
            }
        }
        coverage = coverage
            .checked_add(length)
            .ok_or(CoreError::IntegerOverflow)?;
        validate_logical_length(coverage)?;
    }
    if coverage != logical_len {
        return Err(CoreError::LogicalLength);
    }
    cursor.finish()?;
    Ok(FileRecordV1 {
        mode,
        logical_len,
        extent_count,
        chunk_ref_count: total_chunk_refs,
    })
}

fn decode_symlink(payload: &[u8]) -> CoreResult<SymlinkRecordV1> {
    let mut cursor = SliceCursor::new(payload);
    let target_len = cursor.read_u32_be()?;
    let target =
        cursor.read_nonzero_bounded_bytes(u64::from(target_len), 4_096, CoreError::Target)?;
    ValidatedSymlinkTarget::new(target)?;
    cursor.finish()?;
    Ok(SymlinkRecordV1 { target_len })
}

fn read_component<'a>(cursor: &mut SliceCursor<'a>) -> CoreResult<ValidatedComponent<'a>> {
    let len = cursor.read_u16_be()?;
    if len == 0 || len > 255 {
        return Err(CoreError::Name);
    }
    ValidatedComponent::new(cursor.read_bytes(usize::from(len))?)
}

fn require_repetition(cursor: &SliceCursor<'_>, count: u64, width: u64) -> CoreResult<()> {
    let minimum = count.checked_mul(width).ok_or(CoreError::IntegerOverflow)?;
    let remaining = u64::try_from(cursor.remaining()).map_err(|_| CoreError::IntegerOverflow)?;
    if minimum <= remaining {
        Ok(())
    } else {
        Err(CoreError::LogicalLength)
    }
}
