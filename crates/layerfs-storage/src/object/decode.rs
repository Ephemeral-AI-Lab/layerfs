//! Canonical semantic decoder for the five frozen `ELSOBJ01` object kinds.
//!
//! The grammar is written once against a bounded cursor. Slice and neutral
//! read-port callers adapt that cursor; neither transport gets a second
//! envelope, field-width, or semantic-verification rule set.

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
    PhysicalTreeIdV1, ProfileId, VersionIdV1, COMPARISON_WINDOW_BYTES,
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

/// Bounded transport adapter used by the canonical semantic grammar.
pub(super) trait CanonicalObjectCursorV1 {
    fn remaining(&self) -> u64;
    fn read_into(&mut self, destination: &mut [u8]) -> CoreResult<()>;
    fn finish(&self) -> CoreResult<()>;

    fn read_array<const N: usize>(&mut self) -> CoreResult<[u8; N]> {
        let mut bytes = [0_u8; N];
        self.read_into(&mut bytes)?;
        Ok(bytes)
    }

    fn read_u8(&mut self) -> CoreResult<u8> {
        Ok(self.read_array::<1>()?[0])
    }

    fn read_u16_be(&mut self) -> CoreResult<u16> {
        Ok(u16::from_be_bytes(self.read_array::<2>()?))
    }

    fn read_u32_be(&mut self) -> CoreResult<u32> {
        Ok(u32::from_be_bytes(self.read_array::<4>()?))
    }

    fn read_u64_be(&mut self) -> CoreResult<u64> {
        Ok(u64::from_be_bytes(self.read_array::<8>()?))
    }

    fn consume_remaining(&mut self, scratch: &mut [u8; COMPARISON_WINDOW_BYTES]) -> CoreResult<()> {
        while self.remaining() != 0 {
            let take = usize::try_from(self.remaining().min(COMPARISON_WINDOW_BYTES as u64))
                .map_err(|_| CoreError::IntegerOverflow)?;
            self.read_into(&mut scratch[..take])?;
        }
        Ok(())
    }
}

impl CanonicalObjectCursorV1 for SliceCursor<'_> {
    fn remaining(&self) -> u64 {
        self.remaining() as u64
    }

    fn read_into(&mut self, destination: &mut [u8]) -> CoreResult<()> {
        let bytes = self.read_bytes(destination.len())?;
        destination.copy_from_slice(bytes);
        Ok(())
    }

    fn finish(&self) -> CoreResult<()> {
        if self.is_eof() {
            Ok(())
        } else {
            Err(CoreError::TrailingBytes)
        }
    }
}

pub fn decode_physical_object_v1<'a, V: StrongEdgeVisitorV1 + ?Sized>(
    bytes: &'a [u8],
    visitor: &mut V,
) -> CoreResult<ValidatedPhysicalObjectV1<'a>> {
    let mut cursor = SliceCursor::new(bytes);
    let header = decode_envelope_header_from_cursor_v1(&mut cursor)?;
    let mut scratch = [0_u8; COMPARISON_WINDOW_BYTES];
    visitor.begin_object();
    let decoded = decode_payload_from_cursor_v1(header.kind, &mut cursor, visitor, &mut scratch);
    match decoded {
        Ok(payload) => {
            cursor.finish()?;
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

/// Parse and validate the fixed envelope header. The port decoder reads the
/// same 52 bytes into its transport buffer and calls this function directly.
pub(super) fn decode_envelope_header_bytes_v1(
    envelope: &[u8; OBJECT_HEADER_BYTES as usize],
) -> CoreResult<PhysicalObjectHeaderV1> {
    let mut cursor = SliceCursor::new(envelope);
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
    cursor.finish()?;
    Ok(PhysicalObjectHeaderV1 {
        kind,
        profile_id,
        payload_len,
        complete_len,
    })
}

pub(super) fn decode_envelope_header_from_cursor_v1<C: CanonicalObjectCursorV1 + ?Sized>(
    cursor: &mut C,
) -> CoreResult<PhysicalObjectHeaderV1> {
    let envelope = cursor.read_array::<{ OBJECT_HEADER_BYTES as usize }>()?;
    let header = decode_envelope_header_bytes_v1(&envelope)?;
    if cursor.remaining() < header.payload_len {
        return Err(CoreError::Truncated);
    }
    if cursor.remaining() > header.payload_len {
        return Err(CoreError::TrailingBytes);
    }
    Ok(header)
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

pub(super) fn decode_payload_from_cursor_v1<C, V>(
    kind: PhysicalObjectKindV1,
    cursor: &mut C,
    visitor: &mut V,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
) -> CoreResult<PhysicalObjectPayloadV1>
where
    C: CanonicalObjectCursorV1 + ?Sized,
    V: StrongEdgeVisitorV1 + ?Sized,
{
    match kind {
        PhysicalObjectKindV1::VersionRecord => {
            decode_version(cursor, visitor).map(PhysicalObjectPayloadV1::VersionRecord)
        }
        PhysicalObjectKindV1::Tree => {
            decode_tree(cursor, visitor).map(PhysicalObjectPayloadV1::Tree)
        }
        PhysicalObjectKindV1::File => {
            decode_file(cursor, visitor).map(PhysicalObjectPayloadV1::File)
        }
        PhysicalObjectKindV1::Symlink => {
            decode_symlink(cursor, scratch).map(PhysicalObjectPayloadV1::Symlink)
        }
        PhysicalObjectKindV1::Chunk => {
            let payload_len =
                u32::try_from(cursor.remaining()).map_err(|_| CoreError::IntegerOverflow)?;
            cursor.consume_remaining(scratch)?;
            Ok(PhysicalObjectPayloadV1::Chunk(ChunkRecordV1 {
                payload_len,
            }))
        }
    }
}

fn decode_version<C: CanonicalObjectCursorV1 + ?Sized, V: StrongEdgeVisitorV1 + ?Sized>(
    cursor: &mut C,
    visitor: &mut V,
) -> CoreResult<VersionRecordV1> {
    let version_id = VersionIdV1::from_digest(cursor.read_array::<32>()?);
    let chunker_spec_id = ChunkerSpecId::from_digest(cursor.read_array::<32>()?);
    let digest_spec_id = DigestSpecId::from_digest(cursor.read_array::<32>()?);
    let root_tree_id = PhysicalTreeIdV1::from_digest(cursor.read_array::<32>()?);
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

fn decode_tree<C: CanonicalObjectCursorV1 + ?Sized, V: StrongEdgeVisitorV1 + ?Sized>(
    cursor: &mut C,
    visitor: &mut V,
) -> CoreResult<TreeRecordV1> {
    let subtype = TreeSubtypeV1::try_from(cursor.read_u8()?)?;
    let value = match subtype {
        TreeSubtypeV1::Directory => TreeRecordV1::Directory(decode_directory(cursor, visitor)?),
        TreeSubtypeV1::Leaf => TreeRecordV1::Leaf(decode_leaf(cursor, visitor)?),
        TreeSubtypeV1::Index => TreeRecordV1::Index(decode_index(cursor, visitor)?),
    };
    cursor.finish()?;
    Ok(value)
}

fn decode_directory<C: CanonicalObjectCursorV1 + ?Sized, V: StrongEdgeVisitorV1 + ?Sized>(
    cursor: &mut C,
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
        let id = PhysicalTreeIdV1::from_digest(cursor.read_array::<32>()?);
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

fn decode_leaf<C: CanonicalObjectCursorV1 + ?Sized, V: StrongEdgeVisitorV1 + ?Sized>(
    cursor: &mut C,
    visitor: &mut V,
) -> CoreResult<LeafTreeRecordV1> {
    let depth = cursor.read_u8()?;
    if depth != 0 {
        return Err(CoreError::CountCap);
    }
    let count = cursor.read_u16_be()?;
    validate_tree_leaf_fanout(u64::from(count))?;
    require_repetition(cursor, u64::from(count), MIN_TREE_LEAF_ENTRY_BYTES)?;
    let mut previous = [0_u8; 255];
    let mut previous_len = 0_usize;
    for _ in 0..count {
        let mut current = [0_u8; 255];
        let current_len = read_component(cursor, &mut current)?;
        if previous_len != 0
            && compare_unsigned(&previous[..previous_len], &current[..current_len])
                != core::cmp::Ordering::Less
        {
            return Err(CoreError::NonCanonicalOrder);
        }
        let kind = PhysicalTreeChildKindV1::try_from(cursor.read_u8()?)?;
        let raw_id = cursor.read_array::<32>()?;
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
        previous[..current_len].copy_from_slice(&current[..current_len]);
        previous_len = current_len;
    }
    Ok(LeafTreeRecordV1 { depth, count })
}

fn decode_index<C: CanonicalObjectCursorV1 + ?Sized, V: StrongEdgeVisitorV1 + ?Sized>(
    cursor: &mut C,
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
    let mut previous_last = [0_u8; 255];
    let mut previous_last_len = 0_usize;
    let mut total = 0_u64;
    for index in 0..count {
        let subtree_entry_count = cursor.read_u32_be()?;
        if subtree_entry_count == 0 || u64::from(subtree_entry_count) > MAX_ENTRIES {
            return Err(CoreError::CountCap);
        }
        let mut first = [0_u8; 255];
        let first_len = read_component(cursor, &mut first)?;
        if previous_last_len != 0
            && compare_unsigned(&previous_last[..previous_last_len], &first[..first_len])
                != core::cmp::Ordering::Less
        {
            return Err(CoreError::NonCanonicalOrder);
        }
        let mut last = [0_u8; 255];
        let last_len = read_component(cursor, &mut last)?;
        if compare_unsigned(&first[..first_len], &last[..last_len]).is_gt() {
            return Err(CoreError::NonCanonicalOrder);
        }
        let child = PhysicalTreeIdV1::from_digest(cursor.read_array::<32>()?);
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
        previous_last[..last_len].copy_from_slice(&last[..last_len]);
        previous_last_len = last_len;
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

fn decode_file<C: CanonicalObjectCursorV1 + ?Sized, V: StrongEdgeVisitorV1 + ?Sized>(
    cursor: &mut C,
    visitor: &mut V,
) -> CoreResult<FileRecordV1> {
    let mode = cursor.read_u16_be()?;
    validate_file_mode(mode)?;
    let logical_len = cursor.read_u64_be()?;
    validate_logical_length(logical_len)?;
    let extent_count = cursor.read_u32_be()?;
    validate_extents_per_file(u64::from(extent_count))?;
    require_repetition(cursor, u64::from(extent_count), MIN_FILE_EXTENT_BYTES)?;
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
                require_repetition(cursor, u64::from(count), CHUNK_REFERENCE_BYTES)?;
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
                    let id = PhysicalChunkIdV1::from_digest(cursor.read_array::<32>()?);
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

fn decode_symlink<C: CanonicalObjectCursorV1 + ?Sized>(
    cursor: &mut C,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
) -> CoreResult<SymlinkRecordV1> {
    let target_len = cursor.read_u32_be()?;
    if target_len == 0 || target_len > 4_096 {
        return Err(CoreError::Target);
    }
    let len = usize::try_from(target_len).map_err(|_| CoreError::IntegerOverflow)?;
    cursor.read_into(&mut scratch[..len])?;
    ValidatedSymlinkTarget::new(&scratch[..len])?;
    cursor.finish()?;
    Ok(SymlinkRecordV1 { target_len })
}

fn read_component<C: CanonicalObjectCursorV1 + ?Sized>(
    cursor: &mut C,
    destination: &mut [u8; 255],
) -> CoreResult<usize> {
    let len = usize::from(cursor.read_u16_be()?);
    if len == 0 || len > destination.len() {
        return Err(CoreError::Name);
    }
    cursor.read_into(&mut destination[..len])?;
    ValidatedComponent::new(&destination[..len])?;
    Ok(len)
}

fn require_repetition<C: CanonicalObjectCursorV1 + ?Sized>(
    cursor: &C,
    count: u64,
    width: u64,
) -> CoreResult<()> {
    let minimum = count.checked_mul(width).ok_or(CoreError::IntegerOverflow)?;
    if minimum <= cursor.remaining() {
        Ok(())
    } else {
        Err(CoreError::LogicalLength)
    }
}
