//! Frozen logical and physical LayerFS V1 identities.
//!
//! Every identity domain has a distinct Rust type with a private raw
//! constructor.  The only runtime digest path in this module is unkeyed
//! BLAKE3-256, and all multi-field preimages are fed incrementally.
//!
//! Raw digest bytes cannot construct an identity:
//!
//! ```compile_fail
//! use layerfs_storage::identity::LogicalChunkIdV1;
//! let _ = LogicalChunkIdV1::from_digest([0; 32]);
//! ```
//!
//! Logical and physical domains cannot be crossed:
//!
//! ```compile_fail
//! use layerfs_storage::identity::{LogicalChunkIdV1, PhysicalChunkIdV1};
//! fn needs_logical(_: LogicalChunkIdV1) {}
//! fn wrong(id: PhysicalChunkIdV1) { needs_logical(id); }
//! ```

use crate::format::{
    checked_u32, compare_unsigned, validate_chunk_reference_len, validate_chunk_refs_per_file,
    validate_directory_mode, validate_entry_count, validate_file_mode,
    validate_logical_chunk_payload_len, validate_logical_length, validate_physical_object_len,
    DirectoryModeContext, ValidatedComponent, ValidatedSymlinkTarget,
    ROOT_DIRECTORY_MODE_SENTINEL_V1,
};
use crate::{CoreError, CoreResult};

pub const DIGEST_BYTES: usize = 32;
pub const COMPARISON_WINDOW_BYTES: usize = 65_536;
/// Exact resident size of one pinned BLAKE3 1.8.5 streaming state on this
/// target. Operation memory plans charge one unit for each concurrently live
/// identity/checksum state.
pub const IDENTITY_HASHER_BYTES_V1: u64 = core::mem::size_of::<blake3::Hasher>() as u64;

const SCHEMA_V1_LE: [u8; 2] = 1_u16.to_le_bytes();
const ELSHASH1: [u8; 8] = *b"ELSHASH1";

#[allow(dead_code)]
pub(crate) const TAG_CHUNKER_SPEC: u8 = 0x01;
#[allow(dead_code)]
pub(crate) const TAG_DIGEST_SPEC: u8 = 0x02;
#[allow(dead_code)]
pub(crate) const TAG_PROFILE_SPEC: u8 = 0x03;
pub(crate) const TAG_PHYSICAL_CHUNK: u8 = 0x10;
pub(crate) const TAG_PHYSICAL_VERSION_RECORD: u8 = 0x11;
pub(crate) const TAG_PHYSICAL_TREE: u8 = 0x12;
pub(crate) const TAG_PHYSICAL_FILE: u8 = 0x13;
pub(crate) const TAG_PHYSICAL_SYMLINK: u8 = 0x14;
#[allow(dead_code)]
pub(crate) const TAG_PACK: u8 = 0x20;
#[allow(dead_code)]
pub(crate) const TAG_OBJECT_CHECKSUM: u8 = 0x21;

macro_rules! typed_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name([u8; DIGEST_BYTES]);

        impl $name {
            pub const fn as_bytes(&self) -> &[u8; DIGEST_BYTES] {
                &self.0
            }

            #[allow(dead_code)]
            pub(crate) const fn from_digest(bytes: [u8; DIGEST_BYTES]) -> Self {
                Self(bytes)
            }
        }
    };
}

typed_id!(LogicalChunkIdV1);
typed_id!(LogicalFileIdV1);
typed_id!(FileNodeIdV1);
typed_id!(SymlinkNodeIdV1);
typed_id!(DirectoryNodeIdV1);
typed_id!(VersionIdV1);

typed_id!(PhysicalChunkIdV1);
typed_id!(PhysicalFileIdV1);
typed_id!(PhysicalTreeIdV1);
typed_id!(PhysicalSymlinkIdV1);
typed_id!(PhysicalVersionRecordIdV1);
typed_id!(PackIdV1);
typed_id!(ObjectChecksumV1);

typed_id!(DigestSpecId);
typed_id!(ChunkerSpecId);
typed_id!(ProfileId);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LogicalChunkIdentityV1 {
    id: LogicalChunkIdV1,
    logical_len: u64,
}

impl LogicalChunkIdentityV1 {
    pub const fn id(self) -> LogicalChunkIdV1 {
        self.id
    }

    pub const fn logical_len(self) -> u64 {
        self.logical_len
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LogicalChunkRefV1 {
    id: LogicalChunkIdV1,
    chunk_len: u64,
}

impl LogicalChunkRefV1 {
    pub const fn from_identity(identity: LogicalChunkIdentityV1) -> Self {
        Self {
            id: identity.id,
            chunk_len: identity.logical_len,
        }
    }

    pub const fn id(self) -> LogicalChunkIdV1 {
        self.id
    }

    pub const fn chunk_len(self) -> u64 {
        self.chunk_len
    }

    pub(crate) const fn from_parts(id: LogicalChunkIdV1, chunk_len: u64) -> Self {
        Self { id, chunk_len }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LogicalFileIdentityV1 {
    id: LogicalFileIdV1,
    logical_len: u64,
}

impl LogicalFileIdentityV1 {
    pub const fn id(self) -> LogicalFileIdV1 {
        self.id
    }

    pub const fn logical_len(self) -> u64 {
        self.logical_len
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExplicitDirectoryNodeV1(DirectoryNodeIdV1);

impl ExplicitDirectoryNodeV1 {
    pub const fn id(self) -> DirectoryNodeIdV1 {
        self.0
    }

    pub(crate) const fn from_digest(bytes: [u8; DIGEST_BYTES]) -> Self {
        Self(DirectoryNodeIdV1::from_digest(bytes))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImplicitRootDirectoryV1(DirectoryNodeIdV1);

impl ImplicitRootDirectoryV1 {
    pub const fn id(self) -> DirectoryNodeIdV1 {
        self.0
    }

    pub(crate) const fn from_digest(bytes: [u8; DIGEST_BYTES]) -> Self {
        Self(DirectoryNodeIdV1::from_digest(bytes))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogicalChildIdV1 {
    File(FileNodeIdV1),
    Directory(ExplicitDirectoryNodeV1),
    Symlink(SymlinkNodeIdV1),
}

impl LogicalChildIdV1 {
    pub(crate) const fn kind_and_id(self) -> (u8, [u8; DIGEST_BYTES]) {
        match self {
            Self::File(id) => (0x01, id.0),
            Self::Directory(id) => (0x02, id.0 .0),
            Self::Symlink(id) => (0x03, id.0),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LogicalDirectoryEntryV1<'a> {
    name: ValidatedComponent<'a>,
    child: LogicalChildIdV1,
}

impl<'a> LogicalDirectoryEntryV1<'a> {
    pub const fn new(name: ValidatedComponent<'a>, child: LogicalChildIdV1) -> Self {
        Self { name, child }
    }
}

/// Derive `ESV2-LCHUNK\0 || schema-LE || len-LE || payload`.
pub fn derive_logical_chunk_v1(payload: &[u8]) -> CoreResult<LogicalChunkIdentityV1> {
    let payload_len = u64::try_from(payload.len()).map_err(|_| CoreError::IntegerOverflow)?;
    validate_logical_chunk_payload_len(payload_len)?;
    let mut hasher = StructuralHasher::new();
    hasher.write(b"ESV2-LCHUNK\0");
    hasher.write(&SCHEMA_V1_LE);
    hasher.write(&payload_len.to_le_bytes());
    hasher.write(payload);
    Ok(LogicalChunkIdentityV1 {
        id: LogicalChunkIdV1(hasher.finish()),
        logical_len: payload_len,
    })
}

/// Derive a logical chunk from at most two spans borrowed from the CDC ring.
#[allow(dead_code)]
pub(crate) fn derive_logical_chunk_spans_v1(
    first: &[u8],
    second: &[u8],
) -> CoreResult<LogicalChunkIdentityV1> {
    let payload_len = first
        .len()
        .checked_add(second.len())
        .ok_or(CoreError::IntegerOverflow)?;
    let payload_len = u64::try_from(payload_len).map_err(|_| CoreError::IntegerOverflow)?;
    validate_logical_chunk_payload_len(payload_len)?;
    let mut hasher = StructuralHasher::new();
    hasher.write(b"ESV2-LCHUNK\0");
    hasher.write(&SCHEMA_V1_LE);
    hasher.write(&payload_len.to_le_bytes());
    hasher.write(first);
    hasher.write(second);
    Ok(LogicalChunkIdentityV1 {
        id: LogicalChunkIdV1(hasher.finish()),
        logical_len: payload_len,
    })
}

pub fn derive_logical_file_v1(
    logical_len: u64,
    chunks: &[LogicalChunkRefV1],
) -> CoreResult<LogicalFileIdentityV1> {
    validate_logical_length(logical_len)?;
    let chunk_count = u64::try_from(chunks.len()).map_err(|_| CoreError::IntegerOverflow)?;
    validate_chunk_refs_per_file(chunk_count)?;
    let chunk_count = checked_u32(chunk_count)?;
    let mut reconstructed = 0_u64;
    for chunk in chunks {
        validate_chunk_reference_len(chunk.chunk_len)?;
        reconstructed = reconstructed
            .checked_add(chunk.chunk_len)
            .ok_or(CoreError::IntegerOverflow)?;
    }
    if reconstructed != logical_len {
        return Err(CoreError::LogicalLength);
    }

    let mut hasher = StructuralHasher::new();
    hasher.write(b"ESV2-LFILE\0");
    hasher.write(&SCHEMA_V1_LE);
    hasher.write(&logical_len.to_le_bytes());
    hasher.write(&chunk_count.to_le_bytes());
    for chunk in chunks {
        hasher.write(chunk.id.as_bytes());
        hasher.write(&chunk.chunk_len.to_le_bytes());
    }
    Ok(LogicalFileIdentityV1 {
        id: LogicalFileIdV1(hasher.finish()),
        logical_len,
    })
}

/// Incremental logical-file identity construction over an independently
/// bounded/replayable chunk-reference source. The count is framed before the
/// references, so callers must obtain it during the one-pass CDC phase.
pub(crate) struct LogicalFileHasherV1 {
    hasher: StructuralHasher,
    expected_len: u64,
    expected_count: u32,
    reconstructed_len: u64,
    count: u32,
}

impl LogicalFileHasherV1 {
    pub(crate) fn new(logical_len: u64, chunk_count: u64) -> CoreResult<Self> {
        validate_logical_length(logical_len)?;
        validate_chunk_refs_per_file(chunk_count)?;
        let expected_count = checked_u32(chunk_count)?;
        let mut hasher = StructuralHasher::new();
        hasher.write(b"ESV2-LFILE\0");
        hasher.write(&SCHEMA_V1_LE);
        hasher.write(&logical_len.to_le_bytes());
        hasher.write(&expected_count.to_le_bytes());
        Ok(Self {
            hasher,
            expected_len: logical_len,
            expected_count,
            reconstructed_len: 0,
            count: 0,
        })
    }

    pub(crate) fn push(&mut self, chunk: LogicalChunkRefV1) -> CoreResult<()> {
        validate_chunk_reference_len(chunk.chunk_len)?;
        self.count = self
            .count
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        if self.count > self.expected_count {
            return Err(CoreError::CountCap);
        }
        self.reconstructed_len = self
            .reconstructed_len
            .checked_add(chunk.chunk_len)
            .ok_or(CoreError::IntegerOverflow)?;
        if self.reconstructed_len > self.expected_len {
            return Err(CoreError::LogicalLength);
        }
        self.hasher.write(chunk.id.as_bytes());
        self.hasher.write(&chunk.chunk_len.to_le_bytes());
        Ok(())
    }

    pub(crate) fn finish(self) -> CoreResult<LogicalFileIdentityV1> {
        if self.count != self.expected_count || self.reconstructed_len != self.expected_len {
            return Err(CoreError::LogicalLength);
        }
        Ok(LogicalFileIdentityV1 {
            id: LogicalFileIdV1(self.hasher.finish()),
            logical_len: self.expected_len,
        })
    }
}

pub fn derive_file_node_v1(mode: u16, file: LogicalFileIdentityV1) -> CoreResult<FileNodeIdV1> {
    validate_file_mode(mode)?;
    let mut hasher = StructuralHasher::new();
    hasher.write(b"ESV2-FNODE\0");
    hasher.write(&SCHEMA_V1_LE);
    hasher.write(&mode.to_le_bytes());
    hasher.write(file.id.as_bytes());
    hasher.write(&file.logical_len.to_le_bytes());
    Ok(FileNodeIdV1(hasher.finish()))
}

pub fn derive_symlink_node_v1(target: ValidatedSymlinkTarget<'_>) -> CoreResult<SymlinkNodeIdV1> {
    let target_len =
        u32::try_from(target.as_bytes().len()).map_err(|_| CoreError::IntegerOverflow)?;
    let mut hasher = StructuralHasher::new();
    hasher.write(b"ESV2-SNODE\0");
    hasher.write(&SCHEMA_V1_LE);
    hasher.write(&target_len.to_le_bytes());
    hasher.write(target.as_bytes());
    Ok(SymlinkNodeIdV1(hasher.finish()))
}

pub fn derive_explicit_directory_v1(
    mode: u16,
    entries: &[LogicalDirectoryEntryV1<'_>],
) -> CoreResult<ExplicitDirectoryNodeV1> {
    derive_directory_v1(mode, DirectoryModeContext::Explicit, entries).map(ExplicitDirectoryNodeV1)
}

pub fn derive_implicit_root_directory_v1(
    entries: &[LogicalDirectoryEntryV1<'_>],
) -> CoreResult<ImplicitRootDirectoryV1> {
    derive_directory_v1(
        ROOT_DIRECTORY_MODE_SENTINEL_V1,
        DirectoryModeContext::ImplicitRoot,
        entries,
    )
    .map(ImplicitRootDirectoryV1)
}

fn derive_directory_v1(
    mode: u16,
    context: DirectoryModeContext,
    entries: &[LogicalDirectoryEntryV1<'_>],
) -> CoreResult<DirectoryNodeIdV1> {
    derive_directory_iter_v1(
        mode,
        context,
        u64::try_from(entries.len()).map_err(|_| CoreError::IntegerOverflow)?,
        entries.iter().copied(),
    )
}

pub(crate) fn derive_explicit_directory_iter_v1<'a, I>(
    mode: u16,
    count: u64,
    entries: I,
) -> CoreResult<ExplicitDirectoryNodeV1>
where
    I: IntoIterator<Item = LogicalDirectoryEntryV1<'a>>,
{
    derive_directory_iter_v1(mode, DirectoryModeContext::Explicit, count, entries)
        .map(ExplicitDirectoryNodeV1)
}

pub(crate) fn derive_implicit_root_directory_iter_v1<'a, I>(
    count: u64,
    entries: I,
) -> CoreResult<ImplicitRootDirectoryV1>
where
    I: IntoIterator<Item = LogicalDirectoryEntryV1<'a>>,
{
    derive_directory_iter_v1(
        ROOT_DIRECTORY_MODE_SENTINEL_V1,
        DirectoryModeContext::ImplicitRoot,
        count,
        entries,
    )
    .map(ImplicitRootDirectoryV1)
}

fn derive_directory_iter_v1<'a, I>(
    mode: u16,
    context: DirectoryModeContext,
    count: u64,
    entries: I,
) -> CoreResult<DirectoryNodeIdV1>
where
    I: IntoIterator<Item = LogicalDirectoryEntryV1<'a>>,
{
    validate_directory_mode(mode, context)?;
    validate_entry_count(count)?;
    let count = checked_u32(count)?;
    let mut previous: Option<&[u8]> = None;
    let mut actual = 0_u32;
    let mut hasher = StructuralHasher::new();
    hasher.write(b"ESV2-DNODE\0");
    hasher.write(&SCHEMA_V1_LE);
    hasher.write(&mode.to_le_bytes());
    hasher.write(&count.to_le_bytes());
    for entry in entries {
        actual = actual.checked_add(1).ok_or(CoreError::IntegerOverflow)?;
        if actual > count {
            return Err(CoreError::CountCap);
        }
        let name = entry.name.as_bytes();
        if previous.is_some_and(|left| compare_unsigned(left, name) != core::cmp::Ordering::Less) {
            return Err(CoreError::NonCanonicalOrder);
        }
        previous = Some(name);
        let name_len = u32::try_from(name.len()).map_err(|_| CoreError::IntegerOverflow)?;
        let (kind, child) = entry.child.kind_and_id();
        hasher.write(&name_len.to_le_bytes());
        hasher.write(name);
        hasher.write(&[kind]);
        hasher.write(&child);
    }
    if actual != count {
        return Err(CoreError::Truncated);
    }
    Ok(DirectoryNodeIdV1(hasher.finish()))
}

/// Incremental logical-directory identity construction. The caller supplies
/// the authenticated child count before streaming ordered entries, so no
/// entry vector is retained by closure admission.
pub(crate) struct LogicalDirectoryHasherV1 {
    hasher: StructuralHasher,
    context: DirectoryModeContext,
    expected_count: u32,
    actual_count: u32,
    previous_name: [u8; 255],
    previous_name_len: usize,
}

impl LogicalDirectoryHasherV1 {
    pub(crate) fn new(
        mode: u16,
        context: DirectoryModeContext,
        child_count: u64,
    ) -> CoreResult<Self> {
        validate_directory_mode(mode, context)?;
        validate_entry_count(child_count)?;
        let expected_count = checked_u32(child_count)?;
        let mut hasher = StructuralHasher::new();
        hasher.write(b"ESV2-DNODE\0");
        hasher.write(&SCHEMA_V1_LE);
        hasher.write(&mode.to_le_bytes());
        hasher.write(&expected_count.to_le_bytes());
        Ok(Self {
            hasher,
            context,
            expected_count,
            actual_count: 0,
            previous_name: [0; 255],
            previous_name_len: 0,
        })
    }

    pub(crate) fn push(&mut self, entry: LogicalDirectoryEntryV1<'_>) -> CoreResult<()> {
        self.actual_count = self
            .actual_count
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        if self.actual_count > self.expected_count {
            return Err(CoreError::CountCap);
        }
        let name = entry.name.as_bytes();
        if self.previous_name_len != 0
            && compare_unsigned(&self.previous_name[..self.previous_name_len], name)
                != core::cmp::Ordering::Less
        {
            return Err(CoreError::NonCanonicalOrder);
        }
        self.previous_name[..name.len()].copy_from_slice(name);
        self.previous_name_len = name.len();
        let name_len = u32::try_from(name.len()).map_err(|_| CoreError::IntegerOverflow)?;
        let (kind, child) = entry.child.kind_and_id();
        self.hasher.write(&name_len.to_le_bytes());
        self.hasher.write(name);
        self.hasher.write(&[kind]);
        self.hasher.write(&child);
        Ok(())
    }

    fn finish_id(self) -> CoreResult<DirectoryNodeIdV1> {
        if self.actual_count != self.expected_count {
            return Err(CoreError::Truncated);
        }
        Ok(DirectoryNodeIdV1(self.hasher.finish()))
    }

    pub(crate) fn finish_explicit(self) -> CoreResult<ExplicitDirectoryNodeV1> {
        if self.context != DirectoryModeContext::Explicit {
            return Err(CoreError::ChildMode);
        }
        self.finish_id().map(ExplicitDirectoryNodeV1)
    }

    pub(crate) fn finish_implicit_root(self) -> CoreResult<ImplicitRootDirectoryV1> {
        if self.context != DirectoryModeContext::ImplicitRoot {
            return Err(CoreError::RootSentinel);
        }
        self.finish_id().map(ImplicitRootDirectoryV1)
    }
}

pub fn derive_version_v1(root: ImplicitRootDirectoryV1) -> VersionIdV1 {
    let mut hasher = StructuralHasher::new();
    hasher.write(b"ESV2-VROOT\0");
    hasher.write(&SCHEMA_V1_LE);
    hasher.write(root.0.as_bytes());
    VersionIdV1(hasher.finish())
}

pub(crate) fn hash_frame(tag: u8, payload_len: u64) -> [u8; 20] {
    let mut frame = [0_u8; 20];
    frame[..8].copy_from_slice(&ELSHASH1);
    frame[8] = tag;
    frame[12..].copy_from_slice(&payload_len.to_be_bytes());
    frame
}

/// Exact-length streaming `ELSHASH1` adapter used by direct object writers.
pub(crate) struct FramedHasherV1 {
    hasher: StructuralHasher,
    expected_len: u64,
    written: u64,
}

impl FramedHasherV1 {
    pub(crate) fn new(tag: u8, payload_len: u64) -> Self {
        let mut hasher = StructuralHasher::new();
        hasher.write(&hash_frame(tag, payload_len));
        Self {
            hasher,
            expected_len: payload_len,
            written: 0,
        }
    }

    pub(crate) fn write(&mut self, bytes: &[u8]) -> CoreResult<()> {
        let len = u64::try_from(bytes.len()).map_err(|_| CoreError::IntegerOverflow)?;
        self.written = self
            .written
            .checked_add(len)
            .ok_or(CoreError::IntegerOverflow)?;
        if self.written > self.expected_len {
            return Err(CoreError::TrailingBytes);
        }
        self.hasher.write(bytes);
        Ok(())
    }

    pub(crate) fn finish(self) -> CoreResult<[u8; DIGEST_BYTES]> {
        if self.written != self.expected_len {
            return Err(CoreError::Truncated);
        }
        Ok(self.hasher.finish())
    }
}

pub(crate) fn derive_framed_bytes(tag: u8, payload: &[u8]) -> CoreResult<[u8; DIGEST_BYTES]> {
    let payload_len = u64::try_from(payload.len()).map_err(|_| CoreError::IntegerOverflow)?;
    let mut hasher = StructuralHasher::new();
    hasher.write(&hash_frame(tag, payload_len));
    for block in payload.chunks(COMPARISON_WINDOW_BYTES) {
        hasher.write(block);
    }
    Ok(hasher.finish())
}

pub fn derive_physical_chunk_id_v1(object: &[u8]) -> CoreResult<PhysicalChunkIdV1> {
    derive_physical_object(TAG_PHYSICAL_CHUNK, object).map(PhysicalChunkIdV1)
}

pub fn derive_physical_version_record_id_v1(
    object: &[u8],
) -> CoreResult<PhysicalVersionRecordIdV1> {
    derive_physical_object(TAG_PHYSICAL_VERSION_RECORD, object).map(PhysicalVersionRecordIdV1)
}

pub fn derive_physical_tree_id_v1(object: &[u8]) -> CoreResult<PhysicalTreeIdV1> {
    derive_physical_object(TAG_PHYSICAL_TREE, object).map(PhysicalTreeIdV1)
}

pub fn derive_physical_file_id_v1(object: &[u8]) -> CoreResult<PhysicalFileIdV1> {
    derive_physical_object(TAG_PHYSICAL_FILE, object).map(PhysicalFileIdV1)
}

pub fn derive_physical_symlink_id_v1(object: &[u8]) -> CoreResult<PhysicalSymlinkIdV1> {
    derive_physical_object(TAG_PHYSICAL_SYMLINK, object).map(PhysicalSymlinkIdV1)
}

#[allow(dead_code)]
pub(crate) fn derive_pack_id_v1(bytes_before_id: &[u8]) -> CoreResult<PackIdV1> {
    derive_framed_bytes(TAG_PACK, bytes_before_id).map(PackIdV1)
}

#[allow(dead_code)]
pub(crate) fn derive_object_checksum_v1(object: &[u8]) -> CoreResult<ObjectChecksumV1> {
    derive_framed_bytes(TAG_OBJECT_CHECKSUM, object).map(ObjectChecksumV1)
}

fn derive_physical_object(tag: u8, object: &[u8]) -> CoreResult<[u8; DIGEST_BYTES]> {
    let object_len = u64::try_from(object.len()).map_err(|_| CoreError::IntegerOverflow)?;
    validate_physical_object_len(object_len)?;
    derive_framed_bytes(tag, object)
}

struct StructuralHasher(blake3::Hasher);

impl StructuralHasher {
    fn new() -> Self {
        Self(blake3::Hasher::new())
    }

    fn write(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    fn finish(self) -> [u8; DIGEST_BYTES] {
        *self.0.finalize().as_bytes()
    }
}
