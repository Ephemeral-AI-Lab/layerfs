//! Bounded random-read canonical object decoding model.
//!
//! The port contract never exposes filesystem mechanics and requires every
//! requested range to be filled exactly. The validated result retains only
//! scalar metadata and the authenticated typed physical identity.

use super::{PhysicalObjectHeaderV1, PhysicalObjectPayloadV1, TypedPhysicalObjectIdV1};
use crate::CoreResult;

pub trait PhysicalObjectReadPortV1 {
    fn len(&mut self) -> CoreResult<u64>;

    fn is_empty(&mut self) -> CoreResult<bool> {
        self.len().map(|len| len == 0)
    }

    fn read_exact_at(&mut self, offset: u64, destination: &mut [u8]) -> CoreResult<()>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidatedPhysicalObjectFromPortV1 {
    pub(super) header: PhysicalObjectHeaderV1,
    pub(super) payload: PhysicalObjectPayloadV1,
    pub(super) physical_id: TypedPhysicalObjectIdV1,
}

impl ValidatedPhysicalObjectFromPortV1 {
    pub const fn header(self) -> PhysicalObjectHeaderV1 {
        self.header
    }

    pub const fn payload(self) -> PhysicalObjectPayloadV1 {
        self.payload
    }

    pub const fn physical_id(self) -> TypedPhysicalObjectIdV1 {
        self.physical_id
    }
}

use crate::format::{
    compare_unsigned, validate_chunk_object_count, validate_chunk_reference_len,
    validate_chunk_refs_per_file, validate_chunk_refs_per_version, validate_entry_count,
    validate_extents_per_file, validate_extents_per_version, validate_file_mode,
    validate_logical_length, validate_physical_object_len, validate_total_object_count,
    validate_tree_index_fanout, validate_tree_leaf_fanout, validate_tree_object_count, ExtentTagV1,
    PhysicalObjectKindV1, PhysicalTreeChildKindV1, PresenceV1, TreeSubtypeV1, ValidatedComponent,
    ValidatedSymlinkTarget, MAX_ENTRIES, MAX_LOGICAL_BYTES, MAX_TREE_PAGE_DEPTH,
    ROOT_DIRECTORY_MODE_SENTINEL_V1,
};
use crate::identity::{
    ChunkerSpecId, DigestSpecId, FramedHasherV1, PhysicalChunkIdV1, PhysicalFileIdV1,
    PhysicalSymlinkIdV1, PhysicalTreeIdV1, PhysicalVersionRecordIdV1, ProfileId, VersionIdV1,
    COMPARISON_WINDOW_BYTES, TAG_PHYSICAL_CHUNK, TAG_PHYSICAL_FILE, TAG_PHYSICAL_SYMLINK,
    TAG_PHYSICAL_TREE, TAG_PHYSICAL_VERSION_RECORD,
};
use crate::profile::ProfileSpecV1;
use crate::CoreError;

use super::decode::{
    validate_payload_bound, CHUNK_REFERENCE_BYTES, HOLE_MIN_BYTES, MIN_FILE_EXTENT_BYTES,
    MIN_TREE_INDEX_ENTRY_BYTES, MIN_TREE_LEAF_ENTRY_BYTES,
};
use super::encode::{CANONICAL_OBJECT_MAGIC_V1, OBJECT_HEADER_BYTES};
use super::model::*;

/// Decode, hash, and validate one complete physical object without borrowing
/// or allocating the complete object. The only payload-sized operation is a
/// sequence of reads into the caller-owned 65,536-byte comparison scratch.
pub fn decode_physical_object_from_port_v1<R, V>(
    reader: &mut R,
    visitor: &mut V,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
) -> CoreResult<ValidatedPhysicalObjectFromPortV1>
where
    R: PhysicalObjectReadPortV1 + ?Sized,
    V: StrongEdgeVisitorV1 + ?Sized,
{
    let complete_len = reader.len()?;
    validate_physical_object_len(complete_len)?;
    if complete_len < OBJECT_HEADER_BYTES {
        return Err(CoreError::Truncated);
    }

    let mut envelope = [0_u8; OBJECT_HEADER_BYTES as usize];
    reader.read_exact_at(0, &mut envelope)?;
    if &envelope[..8] != CANONICAL_OBJECT_MAGIC_V1 {
        return Err(CoreError::TypeDomain);
    }
    if u16::from_be_bytes([envelope[8], envelope[9]]) != 1 {
        return Err(CoreError::Schema);
    }
    let kind = PhysicalObjectKindV1::try_from(envelope[10])?;
    if envelope[11] != 0 {
        return Err(CoreError::Flags);
    }
    let profile_id = ProfileId::from_digest(
        envelope[12..44]
            .try_into()
            .map_err(|_| CoreError::Truncated)?,
    );
    if profile_id != ProfileSpecV1::frozen().id() {
        return Err(CoreError::TypeDomain);
    }
    let payload_len = u64::from_be_bytes(
        envelope[44..52]
            .try_into()
            .map_err(|_| CoreError::Truncated)?,
    );
    let declared_complete_len = OBJECT_HEADER_BYTES
        .checked_add(payload_len)
        .ok_or(CoreError::IntegerOverflow)?;
    validate_physical_object_len(declared_complete_len)?;
    validate_payload_bound(kind, payload_len)?;
    if complete_len < declared_complete_len {
        return Err(CoreError::Truncated);
    }
    if complete_len > declared_complete_len {
        return Err(CoreError::TrailingBytes);
    }

    let mut hasher = FramedHasherV1::new(physical_domain_tag(kind), complete_len);
    hasher.write(&envelope)?;
    let header = PhysicalObjectHeaderV1 {
        kind,
        profile_id,
        payload_len,
        complete_len,
    };

    visitor.begin_object();
    let decoded = {
        let mut cursor = PortCursorV1::new(reader, &mut hasher, OBJECT_HEADER_BYTES, complete_len);
        let result = decode_payload_from_port(kind, &mut cursor, visitor, scratch);
        result.and_then(|payload| {
            cursor.finish()?;
            Ok(payload)
        })
    };
    match decoded {
        Ok(payload) => {
            let physical_id = typed_physical_id_from_digest(kind, hasher.finish()?);
            visitor.commit_object();
            Ok(ValidatedPhysicalObjectFromPortV1 {
                header,
                payload,
                physical_id,
            })
        }
        Err(error) => {
            visitor.abort_object();
            Err(error)
        }
    }
}

struct PortCursorV1<'a, R: PhysicalObjectReadPortV1 + ?Sized> {
    reader: &'a mut R,
    hasher: &'a mut FramedHasherV1,
    offset: u64,
    end: u64,
}

impl<'a, R: PhysicalObjectReadPortV1 + ?Sized> PortCursorV1<'a, R> {
    const fn new(reader: &'a mut R, hasher: &'a mut FramedHasherV1, offset: u64, end: u64) -> Self {
        Self {
            reader,
            hasher,
            offset,
            end,
        }
    }

    fn remaining(&self) -> u64 {
        self.end - self.offset
    }

    fn require(&self, len: u64) -> CoreResult<()> {
        if len <= self.remaining() {
            Ok(())
        } else {
            Err(CoreError::Truncated)
        }
    }

    fn read_into(&mut self, destination: &mut [u8]) -> CoreResult<()> {
        let len = u64::try_from(destination.len()).map_err(|_| CoreError::IntegerOverflow)?;
        self.require(len)?;
        self.reader.read_exact_at(self.offset, destination)?;
        self.hasher.write(destination)?;
        self.offset = self
            .offset
            .checked_add(len)
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(())
    }

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

    fn finish(&self) -> CoreResult<()> {
        if self.offset == self.end {
            Ok(())
        } else {
            Err(CoreError::TrailingBytes)
        }
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

fn require_port_repetition<R: PhysicalObjectReadPortV1 + ?Sized>(
    cursor: &PortCursorV1<'_, R>,
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

fn decode_payload_from_port<R, V>(
    kind: PhysicalObjectKindV1,
    cursor: &mut PortCursorV1<'_, R>,
    visitor: &mut V,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
) -> CoreResult<PhysicalObjectPayloadV1>
where
    R: PhysicalObjectReadPortV1 + ?Sized,
    V: StrongEdgeVisitorV1 + ?Sized,
{
    match kind {
        PhysicalObjectKindV1::VersionRecord => {
            decode_version_from_port(cursor, visitor).map(PhysicalObjectPayloadV1::VersionRecord)
        }
        PhysicalObjectKindV1::Tree => {
            decode_tree_from_port(cursor, visitor).map(PhysicalObjectPayloadV1::Tree)
        }
        PhysicalObjectKindV1::File => {
            decode_file_from_port(cursor, visitor).map(PhysicalObjectPayloadV1::File)
        }
        PhysicalObjectKindV1::Symlink => {
            decode_symlink_from_port(cursor, scratch).map(PhysicalObjectPayloadV1::Symlink)
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

fn decode_version_from_port<R, V>(
    cursor: &mut PortCursorV1<'_, R>,
    visitor: &mut V,
) -> CoreResult<VersionRecordV1>
where
    R: PhysicalObjectReadPortV1 + ?Sized,
    V: StrongEdgeVisitorV1 + ?Sized,
{
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

fn decode_tree_from_port<R, V>(
    cursor: &mut PortCursorV1<'_, R>,
    visitor: &mut V,
) -> CoreResult<TreeRecordV1>
where
    R: PhysicalObjectReadPortV1 + ?Sized,
    V: StrongEdgeVisitorV1 + ?Sized,
{
    match TreeSubtypeV1::try_from(cursor.read_u8()?)? {
        TreeSubtypeV1::Directory => {
            decode_directory_from_port(cursor, visitor).map(TreeRecordV1::Directory)
        }
        TreeSubtypeV1::Leaf => decode_leaf_from_port(cursor, visitor).map(TreeRecordV1::Leaf),
        TreeSubtypeV1::Index => decode_index_from_port(cursor, visitor).map(TreeRecordV1::Index),
    }
}

fn decode_directory_from_port<R, V>(
    cursor: &mut PortCursorV1<'_, R>,
    visitor: &mut V,
) -> CoreResult<DirectoryTreeRecordV1>
where
    R: PhysicalObjectReadPortV1 + ?Sized,
    V: StrongEdgeVisitorV1 + ?Sized,
{
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

fn read_component_from_port<R: PhysicalObjectReadPortV1 + ?Sized>(
    cursor: &mut PortCursorV1<'_, R>,
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

fn decode_leaf_from_port<R, V>(
    cursor: &mut PortCursorV1<'_, R>,
    visitor: &mut V,
) -> CoreResult<LeafTreeRecordV1>
where
    R: PhysicalObjectReadPortV1 + ?Sized,
    V: StrongEdgeVisitorV1 + ?Sized,
{
    let depth = cursor.read_u8()?;
    if depth != 0 {
        return Err(CoreError::CountCap);
    }
    let count = cursor.read_u16_be()?;
    validate_tree_leaf_fanout(u64::from(count))?;
    require_port_repetition(cursor, u64::from(count), MIN_TREE_LEAF_ENTRY_BYTES)?;
    let mut previous = [0_u8; 255];
    let mut previous_len = 0_usize;
    for _ in 0..count {
        let mut current = [0_u8; 255];
        let current_len = read_component_from_port(cursor, &mut current)?;
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

fn decode_index_from_port<R, V>(
    cursor: &mut PortCursorV1<'_, R>,
    visitor: &mut V,
) -> CoreResult<IndexTreeRecordV1>
where
    R: PhysicalObjectReadPortV1 + ?Sized,
    V: StrongEdgeVisitorV1 + ?Sized,
{
    let depth = cursor.read_u8()?;
    if depth == 0 || u64::from(depth) > MAX_TREE_PAGE_DEPTH {
        return Err(CoreError::CountCap);
    }
    let count = cursor.read_u16_be()?;
    validate_tree_index_fanout(u64::from(count))?;
    require_port_repetition(cursor, u64::from(count), MIN_TREE_INDEX_ENTRY_BYTES)?;
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
        let first_len = read_component_from_port(cursor, &mut first)?;
        if previous_last_len != 0
            && compare_unsigned(&previous_last[..previous_last_len], &first[..first_len])
                != core::cmp::Ordering::Less
        {
            return Err(CoreError::NonCanonicalOrder);
        }
        let mut last = [0_u8; 255];
        let last_len = read_component_from_port(cursor, &mut last)?;
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

fn decode_file_from_port<R, V>(
    cursor: &mut PortCursorV1<'_, R>,
    visitor: &mut V,
) -> CoreResult<FileRecordV1>
where
    R: PhysicalObjectReadPortV1 + ?Sized,
    V: StrongEdgeVisitorV1 + ?Sized,
{
    let mode = cursor.read_u16_be()?;
    validate_file_mode(mode)?;
    let logical_len = cursor.read_u64_be()?;
    validate_logical_length(logical_len)?;
    let extent_count = cursor.read_u32_be()?;
    validate_extents_per_file(u64::from(extent_count))?;
    require_port_repetition(cursor, u64::from(extent_count), MIN_FILE_EXTENT_BYTES)?;
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
                require_port_repetition(cursor, u64::from(count), CHUNK_REFERENCE_BYTES)?;
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
    Ok(FileRecordV1 {
        mode,
        logical_len,
        extent_count,
        chunk_ref_count: total_chunk_refs,
    })
}

fn decode_symlink_from_port<R: PhysicalObjectReadPortV1 + ?Sized>(
    cursor: &mut PortCursorV1<'_, R>,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
) -> CoreResult<SymlinkRecordV1> {
    let target_len = cursor.read_u32_be()?;
    if target_len == 0 || target_len > 4_096 {
        return Err(CoreError::Target);
    }
    let len = usize::try_from(target_len).map_err(|_| CoreError::IntegerOverflow)?;
    cursor.read_into(&mut scratch[..len])?;
    ValidatedSymlinkTarget::new(&scratch[..len])?;
    Ok(SymlinkRecordV1 { target_len })
}

const fn physical_domain_tag(kind: PhysicalObjectKindV1) -> u8 {
    match kind {
        PhysicalObjectKindV1::VersionRecord => TAG_PHYSICAL_VERSION_RECORD,
        PhysicalObjectKindV1::Tree => TAG_PHYSICAL_TREE,
        PhysicalObjectKindV1::File => TAG_PHYSICAL_FILE,
        PhysicalObjectKindV1::Symlink => TAG_PHYSICAL_SYMLINK,
        PhysicalObjectKindV1::Chunk => TAG_PHYSICAL_CHUNK,
    }
}

fn typed_physical_id_from_digest(
    kind: PhysicalObjectKindV1,
    digest: [u8; 32],
) -> TypedPhysicalObjectIdV1 {
    match kind {
        PhysicalObjectKindV1::VersionRecord => {
            TypedPhysicalObjectIdV1::VersionRecord(PhysicalVersionRecordIdV1::from_digest(digest))
        }
        PhysicalObjectKindV1::Tree => {
            TypedPhysicalObjectIdV1::Tree(PhysicalTreeIdV1::from_digest(digest))
        }
        PhysicalObjectKindV1::File => {
            TypedPhysicalObjectIdV1::File(PhysicalFileIdV1::from_digest(digest))
        }
        PhysicalObjectKindV1::Symlink => {
            TypedPhysicalObjectIdV1::Symlink(PhysicalSymlinkIdV1::from_digest(digest))
        }
        PhysicalObjectKindV1::Chunk => {
            TypedPhysicalObjectIdV1::Chunk(PhysicalChunkIdV1::from_digest(digest))
        }
    }
}
