//! Canonical logical identities for chunks, files, nodes, directories, and versions.

use super::framing::StructuralHasher;
use super::*;
use crate::format::{
    checked_u32, compare_unsigned, validate_chunk_reference_len, validate_chunk_refs_per_file,
    validate_directory_mode, validate_entry_count, validate_file_mode,
    validate_logical_chunk_payload_len, validate_logical_length, DirectoryModeContext,
    ValidatedComponent, ValidatedSymlinkTarget, ROOT_DIRECTORY_MODE_SENTINEL_V1,
};
use crate::{CoreError, CoreResult};

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

/// Incremental logical-file identity construction when the canonical chunk
/// count is learned only after the single CDC pass. The first BLAKE3 chunk is
/// retained so its count field can be filled at `finish`; later bytes are
/// reduced into a fixed number of aligned BLAKE3 subtrees.
pub(crate) struct DeferredCountLogicalFileHasherV1 {
    first_chunk: [u8; blake3::CHUNK_LEN],
    first_len: usize,
    count_offset: usize,
    suffix_hasher: Option<blake3::Hasher>,
    suffix_offset: u64,
    suffix_len: u64,
    suffix_limit: u64,
    suffix_subtrees: [[u8; DIGEST_BYTES]; u64::BITS as usize],
    suffix_subtree_count: usize,
    expected_len: u64,
    reconstructed_len: u64,
    count: u32,
}

pub const DEFERRED_COUNT_LOGICAL_FILE_HASHER_BYTES_V1: u64 =
    core::mem::size_of::<DeferredCountLogicalFileHasherV1>() as u64;

impl DeferredCountLogicalFileHasherV1 {
    pub(crate) fn new(logical_len: u64) -> CoreResult<Self> {
        validate_logical_length(logical_len)?;
        let mut hasher = Self {
            first_chunk: [0; blake3::CHUNK_LEN],
            first_len: 0,
            count_offset: 0,
            suffix_hasher: None,
            suffix_offset: blake3::CHUNK_LEN as u64,
            suffix_len: 0,
            suffix_limit: blake3::CHUNK_LEN as u64,
            suffix_subtrees: [[0; DIGEST_BYTES]; u64::BITS as usize],
            suffix_subtree_count: 0,
            expected_len: logical_len,
            reconstructed_len: 0,
            count: 0,
        };
        hasher.write(b"ESV2-LFILE\0")?;
        hasher.write(&SCHEMA_V1_LE)?;
        hasher.write(&logical_len.to_le_bytes())?;
        hasher.count_offset = hasher.first_len;
        hasher.write(&0_u32.to_le_bytes())?;
        Ok(hasher)
    }

    pub(crate) fn push(&mut self, chunk: LogicalChunkRefV1) -> CoreResult<()> {
        validate_chunk_reference_len(chunk.chunk_len)?;
        let next_count = self
            .count
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        validate_chunk_refs_per_file(u64::from(next_count))?;
        let next_len = self
            .reconstructed_len
            .checked_add(chunk.chunk_len)
            .ok_or(CoreError::IntegerOverflow)?;
        if next_len > self.expected_len {
            return Err(CoreError::LogicalLength);
        }
        self.write(chunk.id.as_bytes())?;
        self.write(&chunk.chunk_len.to_le_bytes())?;
        self.count = next_count;
        self.reconstructed_len = next_len;
        Ok(())
    }

    pub(crate) fn finish(mut self) -> CoreResult<LogicalFileIdentityV1> {
        if self.reconstructed_len != self.expected_len {
            return Err(CoreError::LogicalLength);
        }
        let count_end = self
            .count_offset
            .checked_add(core::mem::size_of::<u32>())
            .ok_or(CoreError::IntegerOverflow)?;
        self.first_chunk
            .get_mut(self.count_offset..count_end)
            .ok_or(CoreError::IntegerOverflow)?
            .copy_from_slice(&self.count.to_le_bytes());

        if self.suffix_hasher.is_none() && self.suffix_subtree_count == 0 {
            let bytes = self
                .first_chunk
                .get(..self.first_len)
                .ok_or(CoreError::IntegerOverflow)?;
            return Ok(LogicalFileIdentityV1 {
                id: LogicalFileIdV1(*blake3::hash(bytes).as_bytes()),
                logical_len: self.expected_len,
            });
        }
        self.finish_suffix_subtree()?;

        use blake3::hazmat::{merge_subtrees_non_root, merge_subtrees_root, HasherExt, Mode};
        let first = blake3::Hasher::new()
            .update(&self.first_chunk)
            .finalize_non_root();
        let mut left = first;
        let final_index = self
            .suffix_subtree_count
            .checked_sub(1)
            .ok_or(CoreError::Truncated)?;
        for index in 0..final_index {
            let right = self
                .suffix_subtrees
                .get(index)
                .ok_or(CoreError::IntegerOverflow)?;
            left = merge_subtrees_non_root(&left, right, Mode::Hash);
        }
        let right = self
            .suffix_subtrees
            .get(final_index)
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(LogicalFileIdentityV1 {
            id: LogicalFileIdV1(*merge_subtrees_root(&left, right, Mode::Hash).as_bytes()),
            logical_len: self.expected_len,
        })
    }

    fn write(&mut self, mut bytes: &[u8]) -> CoreResult<()> {
        if self.first_len < self.first_chunk.len() {
            let take = bytes.len().min(self.first_chunk.len() - self.first_len);
            let end = self
                .first_len
                .checked_add(take)
                .ok_or(CoreError::IntegerOverflow)?;
            self.first_chunk
                .get_mut(self.first_len..end)
                .ok_or(CoreError::IntegerOverflow)?
                .copy_from_slice(&bytes[..take]);
            self.first_len = end;
            bytes = &bytes[take..];
        }
        while !bytes.is_empty() {
            if self.suffix_hasher.is_none() {
                if self.suffix_offset % blake3::CHUNK_LEN as u64 != 0 {
                    return Err(CoreError::IntegerOverflow);
                }
                use blake3::hazmat::HasherExt;
                let mut hasher = blake3::Hasher::new();
                hasher.set_input_offset(self.suffix_offset);
                self.suffix_hasher = Some(hasher);
            }
            let remaining = self
                .suffix_limit
                .checked_sub(self.suffix_len)
                .ok_or(CoreError::IntegerOverflow)?;
            let take = bytes
                .len()
                .min(usize::try_from(remaining).map_err(|_| CoreError::IntegerOverflow)?);
            self.suffix_hasher
                .as_mut()
                .ok_or(CoreError::Truncated)?
                .update(&bytes[..take]);
            self.suffix_len = self
                .suffix_len
                .checked_add(u64::try_from(take).map_err(|_| CoreError::IntegerOverflow)?)
                .ok_or(CoreError::IntegerOverflow)?;
            bytes = &bytes[take..];
            if self.suffix_len == self.suffix_limit {
                self.finish_suffix_subtree()?;
            }
        }
        Ok(())
    }

    fn finish_suffix_subtree(&mut self) -> CoreResult<()> {
        let Some(hasher) = self.suffix_hasher.take() else {
            return Ok(());
        };
        if self.suffix_len == 0 {
            return Err(CoreError::Truncated);
        }
        use blake3::hazmat::HasherExt;
        let completed_full_subtree = self.suffix_len == self.suffix_limit;
        let target = self
            .suffix_subtrees
            .get_mut(self.suffix_subtree_count)
            .ok_or(CoreError::CountCap)?;
        *target = hasher.finalize_non_root();
        self.suffix_subtree_count = self
            .suffix_subtree_count
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        self.suffix_offset = self
            .suffix_offset
            .checked_add(self.suffix_len)
            .ok_or(CoreError::IntegerOverflow)?;
        self.suffix_len = 0;
        if completed_full_subtree {
            self.suffix_limit = blake3::hazmat::max_subtree_len(self.suffix_offset)
                .ok_or(CoreError::IntegerOverflow)?;
        }
        Ok(())
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deferred_count_file_hasher_matches_frozen_streaming_identity() {
        for count in [0_u32, 1, 24, 25, 26, 49, 50, 51, 100, 1_000, 100_000] {
            let logical_len = u64::from(count);
            let mut frozen = LogicalFileHasherV1::new(logical_len, logical_len).unwrap();
            let mut deferred = DeferredCountLogicalFileHasherV1::new(logical_len).unwrap();
            for index in 0..count {
                let mut digest = [0_u8; DIGEST_BYTES];
                digest[..4].copy_from_slice(&index.to_le_bytes());
                let reference =
                    LogicalChunkRefV1::from_parts(LogicalChunkIdV1::from_digest(digest), 1);
                frozen.push(reference).unwrap();
                deferred.push(reference).unwrap();
            }
            assert_eq!(deferred.finish().unwrap(), frozen.finish().unwrap());
        }
    }
}
