//! Checked fixed-width canonical codecs and caller-owned sinks.

use crate::error::{CoreError, CoreResult};

pub const PORTABLE_MODE_MAX: u16 = 0x0fff;
pub const ROOT_DIRECTORY_MODE_SENTINEL_V1: u16 = 0x1000;
pub const MAX_LOGICAL_BYTES: u64 = 8_589_934_592;
pub const MAX_CHUNK_BYTES: u64 = 32_768;
pub const MAX_ENTRIES: u64 = 1_000_000;
pub const MAX_TREE_OBJECTS: u64 = 4_000_001;
pub const MAX_CHUNK_OBJECTS: u64 = 2_310_720;
pub const MAX_TOTAL_OBJECTS: u64 = 7_310_722;
pub const MAX_EXTENTS_PER_FILE: u64 = 262_144;
pub const MAX_CHUNK_REFS_PER_FILE: u64 = 1_310_720;
pub const MAX_EXTENTS_PER_VERSION: u64 = 1_262_144;
pub const MAX_CHUNK_REFS_PER_VERSION: u64 = 2_310_720;
pub const MAX_TREE_LEAF_FANOUT: u64 = 192;
pub const MAX_TREE_INDEX_FANOUT: u64 = 96;
pub const MAX_TREE_PAGE_DEPTH: u64 = 2;
pub const MAX_PHYSICAL_OBJECT_BYTES: u64 = 50_593_858;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogicalChildKindV1 {
    File,
    Directory,
    Symlink,
}

impl TryFrom<u8> for LogicalChildKindV1 {
    type Error = CoreError;

    fn try_from(value: u8) -> CoreResult<Self> {
        match value {
            0x01 => Ok(Self::File),
            0x02 => Ok(Self::Directory),
            0x03 => Ok(Self::Symlink),
            _ => Err(CoreError::UnknownKind),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhysicalObjectKindV1 {
    VersionRecord,
    Tree,
    File,
    Symlink,
    Chunk,
}

impl TryFrom<u8> for PhysicalObjectKindV1 {
    type Error = CoreError;

    fn try_from(value: u8) -> CoreResult<Self> {
        match value {
            0x01 => Ok(Self::VersionRecord),
            0x02 => Ok(Self::Tree),
            0x03 => Ok(Self::File),
            0x04 => Ok(Self::Symlink),
            0x05 => Ok(Self::Chunk),
            _ => Err(CoreError::UnknownKind),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TreeSubtypeV1 {
    Directory,
    Leaf,
    Index,
}

impl TryFrom<u8> for TreeSubtypeV1 {
    type Error = CoreError;

    fn try_from(value: u8) -> CoreResult<Self> {
        match value {
            0x01 => Ok(Self::Directory),
            0x02 => Ok(Self::Leaf),
            0x03 => Ok(Self::Index),
            _ => Err(CoreError::UnknownKind),
        }
    }
}

/// Physical Tree entry kinds are a distinct wire domain from logical child
/// kinds even though the currently frozen byte values happen to overlap.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhysicalTreeChildKindV1 {
    Tree,
    File,
    Symlink,
}

impl TryFrom<u8> for PhysicalTreeChildKindV1 {
    type Error = CoreError;

    fn try_from(value: u8) -> CoreResult<Self> {
        match value {
            0x01 => Ok(Self::Tree),
            0x02 => Ok(Self::File),
            0x03 => Ok(Self::Symlink),
            _ => Err(CoreError::UnknownKind),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtentTagV1 {
    Hole,
    Data,
}

impl TryFrom<u8> for ExtentTagV1 {
    type Error = CoreError;

    fn try_from(value: u8) -> CoreResult<Self> {
        match value {
            0x01 => Ok(Self::Hole),
            0x02 => Ok(Self::Data),
            _ => Err(CoreError::UnknownKind),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresenceV1 {
    Absent,
    Present,
}

impl TryFrom<u8> for PresenceV1 {
    type Error = CoreError;

    fn try_from(value: u8) -> CoreResult<Self> {
        match value {
            0x00 => Ok(Self::Absent),
            0x01 => Ok(Self::Present),
            _ => Err(CoreError::UnknownKind),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirectoryModeContext {
    Explicit,
    ImplicitRoot,
}

pub fn validate_file_mode(mode: u16) -> CoreResult<()> {
    if mode <= PORTABLE_MODE_MAX {
        Ok(())
    } else {
        Err(CoreError::FileMode)
    }
}

pub fn validate_domain(actual: &[u8], expected: &[u8]) -> CoreResult<()> {
    if actual == expected {
        Ok(())
    } else {
        Err(CoreError::TypeDomain)
    }
}

pub fn validate_schema_v1(schema: u16) -> CoreResult<()> {
    if schema == 1 {
        Ok(())
    } else {
        Err(CoreError::Schema)
    }
}

pub fn validate_flags_zero(flags: u8) -> CoreResult<()> {
    if flags == 0 {
        Ok(())
    } else {
        Err(CoreError::Flags)
    }
}

pub fn validate_reserved_zero(reserved: &[u8]) -> CoreResult<()> {
    if reserved.iter().all(|&byte| byte == 0) {
        Ok(())
    } else {
        Err(CoreError::Reserved)
    }
}

pub fn validate_directory_mode(mode: u16, context: DirectoryModeContext) -> CoreResult<()> {
    match context {
        DirectoryModeContext::Explicit if mode <= PORTABLE_MODE_MAX => Ok(()),
        DirectoryModeContext::Explicit => Err(CoreError::ChildMode),
        DirectoryModeContext::ImplicitRoot if mode == ROOT_DIRECTORY_MODE_SENTINEL_V1 => Ok(()),
        DirectoryModeContext::ImplicitRoot => Err(CoreError::RootSentinel),
    }
}

/// A strict cursor over one already-borrowed canonical value.
#[derive(Clone, Copy, Debug)]
pub struct SliceCursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> SliceCursor<'a> {
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub const fn position(&self) -> usize {
        self.offset
    }

    pub const fn remaining(&self) -> usize {
        self.bytes.len() - self.offset
    }

    pub const fn is_eof(&self) -> bool {
        self.offset == self.bytes.len()
    }

    pub fn read_array<const N: usize>(&mut self) -> CoreResult<&'a [u8; N]> {
        let end = self
            .offset
            .checked_add(N)
            .ok_or(CoreError::IntegerOverflow)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(CoreError::Truncated)?;
        self.offset = end;
        value.try_into().map_err(|_| CoreError::Truncated)
    }

    pub fn read_bytes(&mut self, len: usize) -> CoreResult<&'a [u8]> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or(CoreError::IntegerOverflow)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(CoreError::Truncated)?;
        self.offset = end;
        Ok(value)
    }

    /// Validate a contextual bound and integer conversion before borrowing any
    /// variable bytes. A failed preflight leaves the cursor unchanged.
    pub fn read_bounded_bytes(
        &mut self,
        len: u64,
        maximum: u64,
        bound_error: CoreError,
    ) -> CoreResult<&'a [u8]> {
        require_at_most(len, maximum, bound_error)?;
        self.read_bytes(checked_usize(len)?)
    }

    /// As [`Self::read_bounded_bytes`], for grammars with a frozen nonzero
    /// lower bound.
    pub fn read_nonzero_bounded_bytes(
        &mut self,
        len: u64,
        maximum: u64,
        bound_error: CoreError,
    ) -> CoreResult<&'a [u8]> {
        require_nonzero_at_most(len, maximum, bound_error)?;
        self.read_bytes(checked_usize(len)?)
    }

    pub fn read_u8(&mut self) -> CoreResult<u8> {
        Ok(self.read_array::<1>()?[0])
    }

    pub fn read_u16_le(&mut self) -> CoreResult<u16> {
        Ok(u16::from_le_bytes(*self.read_array()?))
    }

    pub fn read_u16_be(&mut self) -> CoreResult<u16> {
        Ok(u16::from_be_bytes(*self.read_array()?))
    }

    pub fn read_u32_le(&mut self) -> CoreResult<u32> {
        Ok(u32::from_le_bytes(*self.read_array()?))
    }

    pub fn read_u32_be(&mut self) -> CoreResult<u32> {
        Ok(u32::from_be_bytes(*self.read_array()?))
    }

    pub fn read_u64_le(&mut self) -> CoreResult<u64> {
        Ok(u64::from_le_bytes(*self.read_array()?))
    }

    pub fn read_u64_be(&mut self) -> CoreResult<u64> {
        Ok(u64::from_be_bytes(*self.read_array()?))
    }

    pub fn expect(&mut self, expected: &[u8], error: CoreError) -> CoreResult<()> {
        if self.read_bytes(expected.len())? == expected {
            Ok(())
        } else {
            Err(error)
        }
    }

    pub fn finish(self) -> CoreResult<()> {
        if self.is_eof() {
            Ok(())
        } else {
            Err(CoreError::TrailingBytes)
        }
    }
}

/// Effect-free destination supplied and owned by the caller.
pub trait ByteSink {
    fn remaining_capacity(&self) -> usize;
    fn write(&mut self, bytes: &[u8]) -> CoreResult<()>;
}

/// A strict encoder over a caller-owned byte slice.
#[derive(Debug)]
pub struct SliceSink<'a> {
    bytes: &'a mut [u8],
    offset: usize,
}

impl<'a> SliceSink<'a> {
    pub fn new(bytes: &'a mut [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub const fn len(&self) -> usize {
        self.offset
    }

    pub const fn is_empty(&self) -> bool {
        self.offset == 0
    }

    pub fn written(&self) -> &[u8] {
        &self.bytes[..self.offset]
    }

    pub fn write_u8(&mut self, value: u8) -> CoreResult<()> {
        self.write(&[value])
    }

    pub fn write_u16_le(&mut self, value: u16) -> CoreResult<()> {
        self.write(&value.to_le_bytes())
    }

    pub fn write_u16_be(&mut self, value: u16) -> CoreResult<()> {
        self.write(&value.to_be_bytes())
    }

    pub fn write_u32_le(&mut self, value: u32) -> CoreResult<()> {
        self.write(&value.to_le_bytes())
    }

    pub fn write_u32_be(&mut self, value: u32) -> CoreResult<()> {
        self.write(&value.to_be_bytes())
    }

    pub fn write_u64_le(&mut self, value: u64) -> CoreResult<()> {
        self.write(&value.to_le_bytes())
    }

    pub fn write_u64_be(&mut self, value: u64) -> CoreResult<()> {
        self.write(&value.to_be_bytes())
    }
}

impl ByteSink for SliceSink<'_> {
    fn remaining_capacity(&self) -> usize {
        self.bytes.len() - self.offset
    }

    fn write(&mut self, bytes: &[u8]) -> CoreResult<()> {
        if bytes.len() > self.remaining_capacity() {
            return Err(CoreError::SinkRefused);
        }
        let end = self
            .offset
            .checked_add(bytes.len())
            .ok_or(CoreError::IntegerOverflow)?;
        self.bytes[self.offset..end].copy_from_slice(bytes);
        self.offset = end;
        Ok(())
    }
}

pub fn checked_usize(value: u64) -> CoreResult<usize> {
    value.try_into().map_err(|_| CoreError::IntegerOverflow)
}

pub fn checked_u32(value: u64) -> CoreResult<u32> {
    value.try_into().map_err(|_| CoreError::IntegerOverflow)
}

pub fn checked_encoded_len(base: usize, count: u64, width: usize) -> CoreResult<usize> {
    let count = checked_usize(count)?;
    let variable = count.checked_mul(width).ok_or(CoreError::IntegerOverflow)?;
    base.checked_add(variable).ok_or(CoreError::IntegerOverflow)
}

pub fn require_at_most(value: u64, maximum: u64, bound_error: CoreError) -> CoreResult<()> {
    if value <= maximum {
        Ok(())
    } else {
        Err(bound_error)
    }
}

pub fn require_nonzero_at_most(value: u64, maximum: u64, bound_error: CoreError) -> CoreResult<()> {
    if value != 0 && value <= maximum {
        Ok(())
    } else {
        Err(bound_error)
    }
}

pub fn usize_from_u32(value: u32) -> CoreResult<usize> {
    usize::try_from(value).map_err(|_| CoreError::IntegerOverflow)
}

pub fn validate_logical_length(len: u64) -> CoreResult<()> {
    require_at_most(len, MAX_LOGICAL_BYTES, CoreError::LogicalLength)
}

pub fn validate_logical_chunk_payload_len(len: u64) -> CoreResult<()> {
    require_at_most(len, MAX_CHUNK_BYTES, CoreError::ChunkCap)
}

pub fn validate_physical_chunk_payload_len(len: u64) -> CoreResult<()> {
    require_nonzero_at_most(len, MAX_CHUNK_BYTES, CoreError::ChunkCap)
}

pub fn validate_chunk_reference_len(len: u64) -> CoreResult<()> {
    require_nonzero_at_most(len, MAX_CHUNK_BYTES, CoreError::ChunkLength)
}

pub fn validate_count_at_most(count: u64, maximum: u64) -> CoreResult<()> {
    require_at_most(count, maximum, CoreError::CountCap)
}

pub fn validate_nonzero_count_at_most(count: u64, maximum: u64) -> CoreResult<()> {
    require_nonzero_at_most(count, maximum, CoreError::CountCap)
}

pub fn validate_entry_count(count: u64) -> CoreResult<()> {
    validate_count_at_most(count, MAX_ENTRIES)
}

pub fn validate_tree_object_count(count: u64) -> CoreResult<()> {
    validate_count_at_most(count, MAX_TREE_OBJECTS)
}

pub fn validate_chunk_object_count(count: u64) -> CoreResult<()> {
    validate_count_at_most(count, MAX_CHUNK_OBJECTS)
}

pub fn validate_total_object_count(count: u64) -> CoreResult<()> {
    validate_count_at_most(count, MAX_TOTAL_OBJECTS)
}

pub fn validate_extents_per_file(count: u64) -> CoreResult<()> {
    validate_count_at_most(count, MAX_EXTENTS_PER_FILE)
}

pub fn validate_chunk_refs_per_file(count: u64) -> CoreResult<()> {
    validate_count_at_most(count, MAX_CHUNK_REFS_PER_FILE)
}

pub fn validate_extents_per_version(count: u64) -> CoreResult<()> {
    validate_count_at_most(count, MAX_EXTENTS_PER_VERSION)
}

pub fn validate_chunk_refs_per_version(count: u64) -> CoreResult<()> {
    validate_count_at_most(count, MAX_CHUNK_REFS_PER_VERSION)
}

pub fn validate_tree_leaf_fanout(count: u64) -> CoreResult<()> {
    validate_nonzero_count_at_most(count, MAX_TREE_LEAF_FANOUT)
}

pub fn validate_tree_index_fanout(count: u64) -> CoreResult<()> {
    validate_nonzero_count_at_most(count, MAX_TREE_INDEX_FANOUT)
}

pub fn validate_directory_page_depth(depth: u64) -> CoreResult<()> {
    validate_count_at_most(depth, MAX_TREE_PAGE_DEPTH)
}

pub fn validate_leaf_page_depth(depth: u64) -> CoreResult<()> {
    if depth == 0 {
        Ok(())
    } else {
        Err(CoreError::CountCap)
    }
}

pub fn validate_index_page_depth(depth: u64) -> CoreResult<()> {
    validate_nonzero_count_at_most(depth, MAX_TREE_PAGE_DEPTH)
}

pub fn validate_physical_object_len(len: u64) -> CoreResult<()> {
    require_at_most(len, MAX_PHYSICAL_OBJECT_BYTES, CoreError::PhysicalObjectCap)
}
