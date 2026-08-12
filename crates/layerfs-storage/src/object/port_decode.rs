//! Bounded random-read canonical object decoding model.
//!
//! The port contract never exposes filesystem mechanics and requires every
//! requested range to be filled exactly. The validated result retains only
//! scalar metadata and the authenticated typed physical identity.

use super::decode::{
    decode_envelope_header_bytes_v1, decode_payload_from_cursor_v1, CanonicalObjectCursorV1,
};
use super::encode::{CanonicalPhysicalObjectVerifierV1, OBJECT_HEADER_BYTES};
use super::model::{StrongEdgeV1, StrongEdgeVisitorV1};
use super::{PhysicalObjectHeaderV1, PhysicalObjectPayloadV1, TypedPhysicalObjectIdV1};
use crate::format::{
    validate_physical_object_len, ExtentTagV1, PhysicalTreeChildKindV1, TreeSubtypeV1,
};
use crate::identity::COMPARISON_WINDOW_BYTES;
use crate::identity::{PhysicalChunkIdV1, PhysicalTreeIdV1};
use crate::{CoreError, CoreResult};

pub trait PhysicalObjectReadPortV1 {
    fn len(&mut self) -> CoreResult<u64>;

    fn is_empty(&mut self) -> CoreResult<bool> {
        self.len().map(|len| len == 0)
    }

    fn read_exact_at(&mut self, offset: u64, destination: &mut [u8]) -> CoreResult<()>;
}

/// Authenticated object-payload cursor for read-side typed events.
///
/// The canonical decoder authenticates and validates the complete object
/// first. This cursor only owns the bounded transport offsets needed to stream
/// already-validated tree/file records; it does not define a second semantic
/// grammar.
#[derive(Clone, Copy)]
pub(crate) struct VerifiedObjectStreamV1 {
    offset: u64,
    end: u64,
}

impl VerifiedObjectStreamV1 {
    pub(crate) const fn new(complete_len: u64) -> Self {
        Self {
            offset: OBJECT_HEADER_BYTES,
            end: complete_len,
        }
    }

    pub(crate) fn begin_tree_leaf<R: PhysicalObjectReadPortV1 + ?Sized>(
        &mut self,
        reader: &mut R,
        expected_depth: u8,
        depth: u8,
        count: u16,
    ) -> CoreResult<()> {
        if expected_depth != 0
            || TreeSubtypeV1::try_from(self.read_u8(reader)?)? != TreeSubtypeV1::Leaf
            || self.read_u8(reader)? != depth
            || self.read_u16(reader)? != count
        {
            return Err(CoreError::TypeDomain);
        }
        Ok(())
    }

    pub(crate) fn begin_tree_index<R: PhysicalObjectReadPortV1 + ?Sized>(
        &mut self,
        reader: &mut R,
        expected_depth: u8,
        depth: u8,
        count: u16,
    ) -> CoreResult<()> {
        if expected_depth == 0
            || depth != expected_depth
            || TreeSubtypeV1::try_from(self.read_u8(reader)?)? != TreeSubtypeV1::Index
            || self.read_u8(reader)? != depth
            || self.read_u16(reader)? != count
        {
            return Err(CoreError::TypeDomain);
        }
        Ok(())
    }

    pub(crate) fn next_leaf_entry<R: PhysicalObjectReadPortV1 + ?Sized>(
        &mut self,
        reader: &mut R,
        name: &mut [u8; 255],
    ) -> CoreResult<(usize, StrongEdgeV1)> {
        let name_len = self.read_component(reader, name)?;
        let kind = PhysicalTreeChildKindV1::try_from(self.read_u8(reader)?)?;
        let id = self.read_array::<R, 32>(reader)?;
        let edge = match kind {
            PhysicalTreeChildKindV1::Tree => StrongEdgeV1::Tree(PhysicalTreeIdV1::from_digest(id)),
            PhysicalTreeChildKindV1::File => {
                StrongEdgeV1::File(crate::identity::PhysicalFileIdV1::from_digest(id))
            }
            PhysicalTreeChildKindV1::Symlink => {
                StrongEdgeV1::Symlink(crate::identity::PhysicalSymlinkIdV1::from_digest(id))
            }
        };
        Ok((name_len, edge))
    }

    pub(crate) fn next_index_entry<R: PhysicalObjectReadPortV1 + ?Sized>(
        &mut self,
        reader: &mut R,
        first: &mut [u8; 255],
        last: &mut [u8; 255],
    ) -> CoreResult<(u32, usize, usize, PhysicalTreeIdV1)> {
        let subtree_entry_count = self.read_u32(reader)?;
        let first_len = self.read_component(reader, first)?;
        let last_len = self.read_component(reader, last)?;
        let child = PhysicalTreeIdV1::from_digest(self.read_array::<R, 32>(reader)?);
        Ok((subtree_entry_count, first_len, last_len, child))
    }

    pub(crate) fn begin_file<R: PhysicalObjectReadPortV1 + ?Sized>(
        &mut self,
        reader: &mut R,
        logical_len: u64,
        extent_count: u32,
    ) -> CoreResult<()> {
        let _mode = self.read_u16(reader)?;
        if self.read_u64(reader)? != logical_len {
            return Err(CoreError::LogicalLength);
        }
        if self.read_u32(reader)? != extent_count {
            return Err(CoreError::CountCap);
        }
        Ok(())
    }

    pub(crate) fn next_file_extent<R: PhysicalObjectReadPortV1 + ?Sized>(
        &mut self,
        reader: &mut R,
    ) -> CoreResult<VerifiedFileExtentV1> {
        let tag = ExtentTagV1::try_from(self.read_u8(reader)?)?;
        let length = self.read_u64(reader)?;
        match tag {
            ExtentTagV1::Hole => Ok(VerifiedFileExtentV1::Hole { length }),
            ExtentTagV1::Data => Ok(VerifiedFileExtentV1::Data {
                length,
                chunk_count: self.read_u32(reader)?,
            }),
        }
    }

    pub(crate) fn next_chunk_reference<R: PhysicalObjectReadPortV1 + ?Sized>(
        &mut self,
        reader: &mut R,
    ) -> CoreResult<(u32, PhysicalChunkIdV1)> {
        let length = self.read_u32(reader)?;
        let id = PhysicalChunkIdV1::from_digest(self.read_array::<R, 32>(reader)?);
        Ok((length, id))
    }

    pub(crate) fn finish(self) -> CoreResult<()> {
        if self.offset == self.end {
            Ok(())
        } else {
            Err(CoreError::TrailingBytes)
        }
    }

    fn read_array<R: PhysicalObjectReadPortV1 + ?Sized, const N: usize>(
        &mut self,
        reader: &mut R,
    ) -> CoreResult<[u8; N]> {
        let next = self
            .offset
            .checked_add(N as u64)
            .ok_or(CoreError::IntegerOverflow)?;
        if next > self.end {
            return Err(CoreError::Truncated);
        }
        let mut bytes = [0_u8; N];
        reader.read_exact_at(self.offset, &mut bytes)?;
        self.offset = next;
        Ok(bytes)
    }

    fn read_u8<R: PhysicalObjectReadPortV1 + ?Sized>(&mut self, reader: &mut R) -> CoreResult<u8> {
        Ok(self.read_array::<R, 1>(reader)?[0])
    }

    fn read_u16<R: PhysicalObjectReadPortV1 + ?Sized>(
        &mut self,
        reader: &mut R,
    ) -> CoreResult<u16> {
        Ok(u16::from_be_bytes(self.read_array::<R, 2>(reader)?))
    }

    fn read_u32<R: PhysicalObjectReadPortV1 + ?Sized>(
        &mut self,
        reader: &mut R,
    ) -> CoreResult<u32> {
        Ok(u32::from_be_bytes(self.read_array::<R, 4>(reader)?))
    }

    fn read_u64<R: PhysicalObjectReadPortV1 + ?Sized>(
        &mut self,
        reader: &mut R,
    ) -> CoreResult<u64> {
        Ok(u64::from_be_bytes(self.read_array::<R, 8>(reader)?))
    }

    fn read_component<R: PhysicalObjectReadPortV1 + ?Sized>(
        &mut self,
        reader: &mut R,
        destination: &mut [u8; 255],
    ) -> CoreResult<usize> {
        let len = usize::from(self.read_u16(reader)?);
        if len == 0 || len > destination.len() {
            return Err(CoreError::Name);
        }
        let next = self
            .offset
            .checked_add(len as u64)
            .ok_or(CoreError::IntegerOverflow)?;
        if next > self.end {
            return Err(CoreError::Truncated);
        }
        reader.read_exact_at(self.offset, &mut destination[..len])?;
        self.offset = next;
        Ok(len)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VerifiedFileExtentV1 {
    Hole { length: u64 },
    Data { length: u64, chunk_count: u32 },
}

pub(crate) fn read_object_payload_exact_v1<R: PhysicalObjectReadPortV1 + ?Sized>(
    reader: &mut R,
    payload_offset: u64,
    destination: &mut [u8],
) -> CoreResult<()> {
    let object_len = reader.len()?;
    let offset = OBJECT_HEADER_BYTES
        .checked_add(payload_offset)
        .ok_or(CoreError::IntegerOverflow)?;
    let end = offset
        .checked_add(destination.len() as u64)
        .ok_or(CoreError::IntegerOverflow)?;
    if end > object_len {
        return Err(CoreError::Truncated);
    }
    reader.read_exact_at(offset, destination)
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

/// Decode, hash, and validate one complete physical object from a bounded
/// random-read port. The semantic grammar is shared with the slice decoder;
/// this module supplies only exact port reads and the physical hash stream.
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
    let header = decode_envelope_header_bytes_v1(&envelope)?;
    if complete_len < header.complete_len {
        return Err(CoreError::Truncated);
    }
    if complete_len > header.complete_len {
        return Err(CoreError::TrailingBytes);
    }

    let mut verifier = CanonicalPhysicalObjectVerifierV1::new(header.kind, complete_len)?;
    verifier.write(&envelope)?;
    visitor.begin_object();
    let decoded = {
        let mut cursor =
            PortCursorV1::new(reader, &mut verifier, OBJECT_HEADER_BYTES, complete_len);
        let result = decode_payload_from_cursor_v1(header.kind, &mut cursor, visitor, scratch);
        result.and_then(|payload| {
            cursor.finish()?;
            Ok(payload)
        })
    };
    match decoded {
        Ok(payload) => {
            let physical_id = verifier.finish()?;
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
    verifier: &'a mut CanonicalPhysicalObjectVerifierV1,
    offset: u64,
    end: u64,
}

impl<'a, R: PhysicalObjectReadPortV1 + ?Sized> PortCursorV1<'a, R> {
    const fn new(
        reader: &'a mut R,
        verifier: &'a mut CanonicalPhysicalObjectVerifierV1,
        offset: u64,
        end: u64,
    ) -> Self {
        Self {
            reader,
            verifier,
            offset,
            end,
        }
    }
}

impl<R: PhysicalObjectReadPortV1 + ?Sized> CanonicalObjectCursorV1 for PortCursorV1<'_, R> {
    fn remaining(&self) -> u64 {
        self.end - self.offset
    }

    fn read_into(&mut self, destination: &mut [u8]) -> CoreResult<()> {
        let len = u64::try_from(destination.len()).map_err(|_| CoreError::IntegerOverflow)?;
        if len > self.remaining() {
            return Err(CoreError::Truncated);
        }
        self.reader.read_exact_at(self.offset, destination)?;
        self.verifier.write(destination)?;
        self.offset = self
            .offset
            .checked_add(len)
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(())
    }

    fn finish(&self) -> CoreResult<()> {
        if self.offset == self.end {
            Ok(())
        } else {
            Err(CoreError::TrailingBytes)
        }
    }
}
