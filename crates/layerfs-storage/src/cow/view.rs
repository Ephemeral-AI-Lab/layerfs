//! Stable, bounded canonical-tree views used by structural COW mutation.
//!
//! The view port exposes declarations and one borrowed entry at a time. It
//! cannot publish, allocate a source-sized cache, or return an entry that
//! outlives the next read, so mutation mechanics remain independent of the
//! storage backend.

use super::tree::{
    encode_directory, encode_index_boundaries, encode_leaf, encode_leaf_from_reader,
    CanonicalDirectoryTreeV1, CanonicalTreeChildV1, CanonicalTreeEntryReaderV1,
    CanonicalTreeEntryV1, DirectoryBuildModeV1, DirectoryLogicalIdentityV1, PreparedTreeSinkV1,
    TreeObjectDispositionV1, TreePageBoundaryV1, TreePageSummaryV1, TreePlanV1, TreeSinkErrorV1,
    MAX_DIRECTORY_HASH_PROOF_NODES, MAX_TREE_OBJECT_BYTES, TREE_INDEX_FANOUT, TREE_LEAF_FANOUT,
};
use crate::format::compare_unsigned;
use crate::identity::{
    ExplicitDirectoryNodeV1, ImplicitRootDirectoryV1, PhysicalTreeIdV1, COMPARISON_WINDOW_BYTES,
    IDENTITY_HASHER_BYTES_V1,
};
use crate::limits::{CounterFieldV1, OperationCountersV1};
use crate::{CoreError, CoreResult};
use blake3::hazmat::{merge_subtrees_non_root, merge_subtrees_root, HasherExt, Mode};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TreeMutationSourceErrorV1 {
    Failure,
}

/// Immutable entry view for general add/remove COW. Every method observes one
/// authenticated snapshot; callers preflight the two declared shapes before
/// reading the first entry.
pub trait CanonicalTreeMutationSourceV1 {
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64>;

    /// Pure declarations: no allocation, caching, or I/O is permitted.
    fn declared_base_entry_count(&self) -> CoreResult<u32>;
    fn declared_result_entry_count(&self) -> CoreResult<u32>;

    fn read_base_entry(
        &mut self,
        ordinal: u32,
    ) -> Result<CanonicalTreeEntryV1<'_>, TreeMutationSourceErrorV1>;

    fn read_result_entry(
        &mut self,
        ordinal: u32,
    ) -> Result<CanonicalTreeEntryV1<'_>, TreeMutationSourceErrorV1>;
}

/// Mutation facts consumed by the authenticated-view verifier. Classification
/// and application stay in `mutate`; this type is only the narrow proof input.
#[derive(Clone, Copy)]
pub(super) enum TreeProofMutationV1<'a> {
    Add(CanonicalTreeEntryV1<'a>),
    Remove(CanonicalTreeEntryV1<'a>),
    Move {
        removal_index: usize,
        insertion_index: usize,
        expected_removed: CanonicalTreeEntryV1<'a>,
        moved: CanonicalTreeEntryV1<'a>,
    },
    ReplacePair {
        first_index: usize,
        first_expected: CanonicalTreeEntryV1<'a>,
        first_replacement: CanonicalTreeEntryV1<'a>,
        second_index: usize,
        second_expected: CanonicalTreeEntryV1<'a>,
        second_replacement: CanonicalTreeEntryV1<'a>,
    },
}

/// Sparse authentication for add/remove. Bytes before `stream_entry_offset`
/// are represented by one exact first chunk, canonical middle subtrees, and
/// at most one partial tail prefix. Entries from `stream_start_index` onward
/// are streamed and hashed; the same proof is reused after changing only the
/// frozen count field and the streamed suffix.
#[derive(Clone, Copy, Debug)]
pub struct DirectoryMutationHashProofV1<'a> {
    pub(super) old_preimage_len: u64,
    pub(super) stream_start_index: u32,
    pub(super) stream_entry_offset: u64,
    pub(super) old_head: &'a [u8],
    pub(super) middle: &'a [DirectoryHashSubtreeV1],
    pub(super) old_tail_prefix: &'a [u8],
}

impl<'a> DirectoryMutationHashProofV1<'a> {
    pub const fn new(
        old_preimage_len: u64,
        stream_start_index: u32,
        stream_entry_offset: u64,
        old_head: &'a [u8],
        middle: &'a [DirectoryHashSubtreeV1],
        old_tail_prefix: &'a [u8],
    ) -> Self {
        Self {
            old_preimage_len,
            stream_start_index,
            stream_entry_offset,
            old_head,
            middle,
            old_tail_prefix,
        }
    }
}

/// Bounded authenticated COW evidence. The view owns the proof-shaped
/// capability; mutation code can consume it but cannot construct physical or
/// logical evidence from an unauthenticated snapshot.
#[derive(Clone, Copy, Debug)]
pub struct AuthenticatedTreeMutationEvidenceV1<'a> {
    pub(super) base_logical: DirectoryLogicalIdentityV1,
    pub(super) base_physical: PhysicalTreeIdV1,
    pub(super) affected_leaf_index: Option<u32>,
    pub(super) affected_leaf: Option<TreePageSummaryV1>,
    pub(super) leaf_group: &'a [TreePageBoundaryV1<'a>],
    pub(super) level_one_group: &'a [TreePageBoundaryV1<'a>],
    pub(super) logical: DirectoryMutationHashProofV1<'a>,
}

impl<'a> AuthenticatedTreeMutationEvidenceV1<'a> {
    pub const fn new(
        base: CanonicalDirectoryTreeV1,
        affected_leaf_index: Option<u32>,
        affected_leaf: Option<TreePageSummaryV1>,
        leaf_group: &'a [TreePageBoundaryV1<'a>],
        level_one_group: &'a [TreePageBoundaryV1<'a>],
        logical: DirectoryMutationHashProofV1<'a>,
    ) -> Self {
        Self {
            base_logical: base.logical(),
            base_physical: base.physical(),
            affected_leaf_index,
            affected_leaf,
            leaf_group,
            level_one_group,
            logical,
        }
    }
}

/// One authenticated BLAKE3 subtree in a sparse proof of the frozen flat
/// logical-directory preimage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DirectoryHashSubtreeV1 {
    pub(super) offset: u64,
    pub(super) byte_len: u64,
    pub(super) chaining_value: [u8; 32],
}

impl DirectoryHashSubtreeV1 {
    pub const fn new(offset: u64, byte_len: u64, chaining_value: [u8; 32]) -> Self {
        Self {
            offset,
            byte_len,
            chaining_value,
        }
    }
}

/// Sparse proof covering every byte outside one bounded logical-directory
/// window.
#[derive(Clone, Copy, Debug)]
pub struct DirectoryHashProofV1<'a> {
    pub(super) preimage_len: u64,
    pub(super) window_offset: u64,
    pub(super) old_window: &'a [u8],
    pub(super) prefix: &'a [DirectoryHashSubtreeV1],
    pub(super) suffix: &'a [DirectoryHashSubtreeV1],
    pub(super) affected_leaf_offset: u64,
}

impl<'a> DirectoryHashProofV1<'a> {
    pub const fn new(
        preimage_len: u64,
        window_offset: u64,
        old_window: &'a [u8],
        prefix: &'a [DirectoryHashSubtreeV1],
        suffix: &'a [DirectoryHashSubtreeV1],
        affected_leaf_offset: u64,
    ) -> Self {
        Self {
            preimage_len,
            window_offset,
            old_window,
            prefix,
            suffix,
            affected_leaf_offset,
        }
    }
}

/// Bounded evidence for a same-name replacement. It contains only the
/// affected leaf, the direct sibling summaries, and a sparse logical proof.
#[derive(Clone, Copy, Debug)]
pub struct AuthenticatedTreeReplacementEvidenceV1<'a> {
    pub(super) base_logical: DirectoryLogicalIdentityV1,
    pub(super) base_physical: PhysicalTreeIdV1,
    pub(super) affected_leaf_index: u32,
    pub(super) affected_entries: &'a [CanonicalTreeEntryV1<'a>],
    pub(super) affected_leaf: TreePageSummaryV1,
    pub(super) leaf_group: &'a [TreePageBoundaryV1<'a>],
    pub(super) level_one_group: &'a [TreePageBoundaryV1<'a>],
    pub(super) logical: DirectoryHashProofV1<'a>,
}

impl<'a> AuthenticatedTreeReplacementEvidenceV1<'a> {
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        base: CanonicalDirectoryTreeV1,
        affected_leaf_index: u32,
        affected_entries: &'a [CanonicalTreeEntryV1<'a>],
        affected_leaf: TreePageSummaryV1,
        leaf_group: &'a [TreePageBoundaryV1<'a>],
        level_one_group: &'a [TreePageBoundaryV1<'a>],
        logical: DirectoryHashProofV1<'a>,
    ) -> Self {
        Self {
            base_logical: base.logical(),
            base_physical: base.physical(),
            affected_leaf_index,
            affected_entries,
            affected_leaf,
            leaf_group,
            level_one_group,
            logical,
        }
    }

    /// Return the exact old entry covered by this bounded replacement proof.
    pub(crate) fn expected_entry_v1(
        self,
        replacement_index: usize,
    ) -> CoreResult<CanonicalTreeEntryV1<'a>> {
        let leaf_first = usize::try_from(self.affected_leaf.first_entry())
            .map_err(|_| CoreError::IntegerOverflow)?;
        let relative = replacement_index
            .checked_sub(leaf_first)
            .ok_or(CoreError::Path)?;
        self.affected_entries
            .get(relative)
            .copied()
            .ok_or(CoreError::Path)
    }
}
pub(super) const HASH_PROOF_ACCUMULATOR_NODES: usize =
    MAX_DIRECTORY_HASH_PROOF_NODES + COMPARISON_WINDOW_BYTES / blake3::CHUNK_LEN + 2;

#[derive(Clone, Copy)]
pub(super) struct HashProofNodeV1 {
    pub(super) offset: u64,
    pub(super) byte_len: u64,
    pub(super) chunks: u64,
    pub(super) chaining_value: [u8; 32],
}

pub(super) const EMPTY_HASH_PROOF_NODE: HashProofNodeV1 = HashProofNodeV1 {
    offset: 0,
    byte_len: 0,
    chunks: 0,
    chaining_value: [0; 32],
};

pub(super) struct HashProofAccumulatorV1 {
    nodes: [HashProofNodeV1; HASH_PROOF_ACCUMULATOR_NODES],
    len: usize,
    total_len: u64,
}

impl HashProofAccumulatorV1 {
    pub(super) fn new(total_len: u64) -> Self {
        Self {
            nodes: [EMPTY_HASH_PROOF_NODE; HASH_PROOF_ACCUMULATOR_NODES],
            len: 0,
            total_len,
        }
    }

    pub(super) fn push(&mut self, mut node: HashProofNodeV1) -> CoreResult<()> {
        while self.len > 0 && self.nodes[self.len - 1].chunks == node.chunks {
            let left = self.nodes[self.len - 1];
            let node_end = node
                .offset
                .checked_add(node.byte_len)
                .ok_or(CoreError::IntegerOverflow)?;
            if left.offset == 0 && node_end == self.total_len && self.len == 1 {
                break;
            }
            if left
                .offset
                .checked_add(left.byte_len)
                .ok_or(CoreError::IntegerOverflow)?
                != node.offset
            {
                return Err(CoreError::IdMismatch);
            }
            self.len -= 1;
            node = HashProofNodeV1 {
                offset: left.offset,
                byte_len: left
                    .byte_len
                    .checked_add(node.byte_len)
                    .ok_or(CoreError::IntegerOverflow)?,
                chunks: left
                    .chunks
                    .checked_add(node.chunks)
                    .ok_or(CoreError::IntegerOverflow)?,
                chaining_value: merge_subtrees_non_root(
                    &left.chaining_value,
                    &node.chaining_value,
                    Mode::Hash,
                ),
            };
        }
        if self.len >= self.nodes.len() {
            return Err(CoreError::ResourceRefused);
        }
        self.nodes[self.len] = node;
        self.len += 1;
        Ok(())
    }

    pub(super) fn finish(mut self) -> CoreResult<[u8; 32]> {
        if self.len < 2 {
            return Err(CoreError::IdMismatch);
        }
        let mut right = self.nodes[self.len - 1];
        self.len -= 1;
        while self.len > 1 {
            let left = self.nodes[self.len - 1];
            self.len -= 1;
            right = HashProofNodeV1 {
                offset: left.offset,
                byte_len: left
                    .byte_len
                    .checked_add(right.byte_len)
                    .ok_or(CoreError::IntegerOverflow)?,
                chunks: left
                    .chunks
                    .checked_add(right.chunks)
                    .ok_or(CoreError::IntegerOverflow)?,
                chaining_value: merge_subtrees_non_root(
                    &left.chaining_value,
                    &right.chaining_value,
                    Mode::Hash,
                ),
            };
        }
        let left = self.nodes[0];
        if left.offset != 0
            || left
                .byte_len
                .checked_add(right.byte_len)
                .ok_or(CoreError::IntegerOverflow)?
                != self.total_len
        {
            return Err(CoreError::IdMismatch);
        }
        Ok(
            *merge_subtrees_root(&left.chaining_value, &right.chaining_value, Mode::Hash)
                .as_bytes(),
        )
    }
}

pub(super) fn hash_directory_sparse_proof(
    proof: DirectoryHashProofV1<'_>,
    window: &[u8],
) -> CoreResult<[u8; 32]> {
    if proof.preimage_len <= blake3::CHUNK_LEN as u64 {
        if proof.window_offset != 0
            || window.len() as u64 != proof.preimage_len
            || !proof.prefix.is_empty()
            || !proof.suffix.is_empty()
        {
            return Err(CoreError::IdMismatch);
        }
        return Ok(*blake3::hash(window).as_bytes());
    }
    let mut accumulator = HashProofAccumulatorV1::new(proof.preimage_len);
    let mut expected_offset = 0_u64;
    for subtree in proof.prefix {
        let node = validate_hash_subtree(*subtree, expected_offset, proof.preimage_len)?;
        expected_offset = expected_offset
            .checked_add(node.byte_len)
            .ok_or(CoreError::IntegerOverflow)?;
        accumulator.push(node)?;
    }
    if expected_offset != proof.window_offset {
        return Err(CoreError::IdMismatch);
    }
    for chunk in window.chunks(blake3::CHUNK_LEN) {
        let mut hasher = blake3::Hasher::new();
        hasher.set_input_offset(expected_offset);
        hasher.update(chunk);
        let node = HashProofNodeV1 {
            offset: expected_offset,
            byte_len: chunk.len() as u64,
            chunks: 1,
            chaining_value: hasher.finalize_non_root(),
        };
        expected_offset = expected_offset
            .checked_add(chunk.len() as u64)
            .ok_or(CoreError::IntegerOverflow)?;
        accumulator.push(node)?;
    }
    for subtree in proof.suffix {
        let node = validate_hash_subtree(*subtree, expected_offset, proof.preimage_len)?;
        expected_offset = expected_offset
            .checked_add(node.byte_len)
            .ok_or(CoreError::IntegerOverflow)?;
        accumulator.push(node)?;
    }
    if expected_offset != proof.preimage_len {
        return Err(CoreError::IdMismatch);
    }
    accumulator.finish()
}

pub(super) fn validate_hash_subtree(
    subtree: DirectoryHashSubtreeV1,
    expected_offset: u64,
    total_len: u64,
) -> CoreResult<HashProofNodeV1> {
    if subtree.offset != expected_offset || subtree.byte_len == 0 {
        return Err(CoreError::IdMismatch);
    }
    let chunk_len = blake3::CHUNK_LEN as u64;
    let chunks = subtree
        .byte_len
        .checked_add(chunk_len - 1)
        .ok_or(CoreError::IntegerOverflow)?
        / chunk_len;
    let span = chunks
        .checked_mul(chunk_len)
        .ok_or(CoreError::IntegerOverflow)?;
    let end = subtree
        .offset
        .checked_add(subtree.byte_len)
        .ok_or(CoreError::IntegerOverflow)?;
    if !chunks.is_power_of_two()
        || subtree.offset % span != 0
        || (subtree.byte_len != span && end != total_len)
        || subtree.byte_len <= span.saturating_sub(chunk_len)
        || end > total_len
    {
        return Err(CoreError::IdMismatch);
    }
    Ok(HashProofNodeV1 {
        offset: subtree.offset,
        byte_len: subtree.byte_len,
        chunks,
        chaining_value: subtree.chaining_value,
    })
}
#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) struct TreeEntrySnapshotV1 {
    pub(super) name: [u8; 255],
    pub(super) name_len: u8,
    pub(super) child: CanonicalTreeChildV1,
}

impl TreeEntrySnapshotV1 {
    pub(super) fn from_entry(entry: CanonicalTreeEntryV1<'_>) -> CoreResult<Self> {
        let name = entry.name.as_bytes();
        let name_len = u8::try_from(name.len()).map_err(|_| CoreError::Name)?;
        let mut owned = [0_u8; 255];
        owned[..name.len()].copy_from_slice(name);
        Ok(Self {
            name: owned,
            name_len,
            child: entry.child,
        })
    }

    pub(super) fn name(&self) -> &[u8] {
        &self.name[..usize::from(self.name_len)]
    }

    pub(super) fn matches(self, entry: CanonicalTreeEntryV1<'_>) -> bool {
        self.name() == entry.name.as_bytes() && self.child == entry.child
    }
}

pub(super) fn read_base_snapshot<T: CanonicalTreeMutationSourceV1 + ?Sized>(
    source: &mut T,
    ordinal: usize,
) -> CoreResult<TreeEntrySnapshotV1> {
    let ordinal = u32::try_from(ordinal).map_err(|_| CoreError::IntegerOverflow)?;
    let entry = source
        .read_base_entry(ordinal)
        .map_err(map_tree_mutation_source)?;
    TreeEntrySnapshotV1::from_entry(entry)
}

pub(super) fn read_result_snapshot<T: CanonicalTreeMutationSourceV1 + ?Sized>(
    source: &mut T,
    ordinal: usize,
) -> CoreResult<TreeEntrySnapshotV1> {
    let ordinal = u32::try_from(ordinal).map_err(|_| CoreError::IntegerOverflow)?;
    let entry = source
        .read_result_entry(ordinal)
        .map_err(map_tree_mutation_source)?;
    TreeEntrySnapshotV1::from_entry(entry)
}

pub(super) fn map_tree_mutation_source(
    TreeMutationSourceErrorV1::Failure: TreeMutationSourceErrorV1,
) -> CoreError {
    CoreError::SourceFailure
}
pub(super) fn validate_page_boundaries(
    boundaries: &[TreePageBoundaryV1<'_>],
    depth: u8,
    first_index: usize,
    end_index: usize,
    plan: TreePlanV1,
) -> CoreResult<()> {
    if boundaries.len() != end_index - first_index {
        return Err(CoreError::IdMismatch);
    }
    let mut previous_last: Option<&[u8]> = None;
    for (relative, boundary) in boundaries.iter().enumerate() {
        let index = first_index
            .checked_add(relative)
            .ok_or(CoreError::IntegerOverflow)?;
        let (first_entry, end_entry) = if depth == 0 {
            let first = index
                .checked_mul(TREE_LEAF_FANOUT)
                .ok_or(CoreError::IntegerOverflow)?;
            (first, (first + TREE_LEAF_FANOUT).min(plan.entry_count))
        } else if depth == 1 {
            let first_leaf = index
                .checked_mul(TREE_INDEX_FANOUT)
                .ok_or(CoreError::IntegerOverflow)?;
            let end_leaf = (first_leaf + TREE_INDEX_FANOUT).min(plan.leaf_count);
            (
                first_leaf
                    .checked_mul(TREE_LEAF_FANOUT)
                    .ok_or(CoreError::IntegerOverflow)?,
                end_leaf
                    .checked_mul(TREE_LEAF_FANOUT)
                    .ok_or(CoreError::IntegerOverflow)?
                    .min(plan.entry_count),
            )
        } else {
            return Err(CoreError::CountCap);
        };
        if boundary.summary.depth != depth
            || boundary.summary.first_entry as usize != first_entry
            || boundary.summary.last_entry as usize != end_entry - 1
            || boundary.summary.subtree_entry_count as usize != end_entry - first_entry
            || compare_unsigned(
                boundary.first_name.as_bytes(),
                boundary.last_name.as_bytes(),
            ) == core::cmp::Ordering::Greater
            || previous_last.is_some_and(|left| {
                compare_unsigned(left, boundary.first_name.as_bytes()) != core::cmp::Ordering::Less
            })
        {
            return Err(CoreError::IdMismatch);
        }
        previous_last = Some(boundary.last_name.as_bytes());
    }
    Ok(())
}

pub(super) fn derive_replacement_logical(
    base: CanonicalDirectoryTreeV1,
    evidence: AuthenticatedTreeReplacementEvidenceV1<'_>,
    replacement_relative: usize,
    replacement: CanonicalTreeEntryV1<'_>,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    counters: &mut OperationCountersV1,
) -> CoreResult<DirectoryLogicalIdentityV1> {
    let proof = evidence.logical;
    let window_len = proof.old_window.len();
    if window_len == 0
        || window_len > scratch.len()
        || proof.window_offset % blake3::CHUNK_LEN as u64 != 0
        || proof.preimage_len < 20
    {
        return Err(CoreError::IdMismatch);
    }
    let window_end = proof
        .window_offset
        .checked_add(window_len as u64)
        .ok_or(CoreError::IntegerOverflow)?;
    if window_end > proof.preimage_len
        || (window_end != proof.preimage_len && window_len % blake3::CHUNK_LEN != 0)
        || proof.affected_leaf_offset < proof.window_offset
    {
        return Err(CoreError::IdMismatch);
    }
    scratch[..window_len].copy_from_slice(proof.old_window);
    counters.add(CounterFieldV1::BytesRead, window_len as u64)?;
    counters.add(CounterFieldV1::BytesCopied, window_len as u64)?;

    let mut cursor = usize::try_from(proof.affected_leaf_offset - proof.window_offset)
        .map_err(|_| CoreError::IntegerOverflow)?;
    let mut replacement_child_offset = None;
    for (relative, entry) in evidence.affected_entries.iter().enumerate() {
        let name = entry.name.as_bytes();
        verify_logical_bytes(scratch, &mut cursor, &(name.len() as u32).to_le_bytes())?;
        verify_logical_bytes(scratch, &mut cursor, name)?;
        let child_offset = cursor;
        let (kind, id) = entry.child.logical().kind_and_id();
        verify_logical_bytes(scratch, &mut cursor, &[kind])?;
        verify_logical_bytes(scratch, &mut cursor, &id)?;
        if relative == replacement_relative {
            replacement_child_offset = Some(child_offset);
        }
    }

    let old_digest = hash_directory_sparse_proof(proof, &scratch[..window_len])?;
    if old_digest != directory_logical_bytes(base.logical) {
        return Err(CoreError::IdMismatch);
    }
    let child_offset = replacement_child_offset.ok_or(CoreError::Path)?;
    let child_end = child_offset
        .checked_add(33)
        .ok_or(CoreError::IntegerOverflow)?;
    let destination = scratch
        .get_mut(child_offset..child_end)
        .ok_or(CoreError::IdMismatch)?;
    let (kind, id) = replacement.child.logical().kind_and_id();
    destination[0] = kind;
    destination[1..].copy_from_slice(&id);
    let new_digest = hash_directory_sparse_proof(proof, &scratch[..window_len])?;
    Ok(directory_logical_from_digest(base.mode, new_digest))
}

fn verify_logical_bytes(bytes: &[u8], cursor: &mut usize, expected: &[u8]) -> CoreResult<()> {
    let end = cursor
        .checked_add(expected.len())
        .ok_or(CoreError::IntegerOverflow)?;
    if bytes.get(*cursor..end) != Some(expected) {
        return Err(CoreError::IdMismatch);
    }
    *cursor = end;
    Ok(())
}

pub(super) fn directory_logical_bytes(logical: DirectoryLogicalIdentityV1) -> [u8; 32] {
    match logical {
        DirectoryLogicalIdentityV1::ImplicitRoot(id) => *id.id().as_bytes(),
        DirectoryLogicalIdentityV1::Explicit(id) => *id.id().as_bytes(),
    }
}

pub(super) fn directory_logical_from_digest(
    mode: DirectoryBuildModeV1,
    digest: [u8; 32],
) -> DirectoryLogicalIdentityV1 {
    match mode {
        DirectoryBuildModeV1::ImplicitRoot => {
            DirectoryLogicalIdentityV1::ImplicitRoot(ImplicitRootDirectoryV1::from_digest(digest))
        }
        DirectoryBuildModeV1::Explicit(_) => {
            DirectoryLogicalIdentityV1::Explicit(ExplicitDirectoryNodeV1::from_digest(digest))
        }
    }
}

fn mutation_entry_encoded_len(entry: CanonicalTreeEntryV1<'_>) -> CoreResult<u64> {
    u64::try_from(entry.name().as_bytes().len())
        .map_err(|_| CoreError::IntegerOverflow)?
        .checked_add(37)
        .ok_or(CoreError::IntegerOverflow)
}

pub(crate) fn mutation_evidence_resident_bytes_v1(
    evidence: AuthenticatedTreeMutationEvidenceV1<'_>,
) -> CoreResult<usize> {
    if evidence.leaf_group.len() > TREE_INDEX_FANOUT
        || evidence.level_one_group.len() > 55
        || evidence.logical.middle.len() > MAX_DIRECTORY_HASH_PROOF_NODES
        || evidence.logical.old_head.len() > blake3::CHUNK_LEN
        || evidence.logical.old_tail_prefix.len() > blake3::CHUNK_LEN
    {
        return Err(CoreError::ResourceRefused);
    }
    let mut bytes = core::mem::size_of_val(evidence.leaf_group)
        .checked_add(core::mem::size_of_val(evidence.level_one_group))
        .and_then(|value| value.checked_add(core::mem::size_of_val(evidence.logical.middle)))
        .and_then(|value| value.checked_add(evidence.logical.old_head.len()))
        .and_then(|value| value.checked_add(evidence.logical.old_tail_prefix.len()))
        .ok_or(CoreError::IntegerOverflow)?;
    for boundary in evidence
        .leaf_group
        .iter()
        .chain(evidence.level_one_group.iter())
    {
        bytes = bytes
            .checked_add(boundary.first_name.as_bytes().len())
            .and_then(|value| value.checked_add(boundary.last_name.as_bytes().len()))
            .ok_or(CoreError::IntegerOverflow)?;
    }
    Ok(bytes)
}

pub(crate) fn mutation_hash_state_bytes_v1() -> CoreResult<u64> {
    u64::try_from(
        core::mem::size_of::<SparsePreimageHasherV1>()
            + 2 * core::mem::size_of::<TreeEntrySnapshotV1>(),
    )
    .map_err(|_| CoreError::IntegerOverflow)?
    .checked_add(IDENTITY_HASHER_BYTES_V1)
    .ok_or(CoreError::IntegerOverflow)
}

pub(super) fn validate_mutation_relation<T: CanonicalTreeMutationSourceV1 + ?Sized>(
    source: &mut T,
    mutation_index: usize,
    mutation: TreeProofMutationV1<'_>,
    base_count: usize,
    result_count: usize,
) -> CoreResult<()> {
    let stream_start = mutation_index.saturating_sub(1);
    let mut previous_base: Option<TreeEntrySnapshotV1> = None;
    for ordinal in stream_start..base_count {
        let entry = read_base_snapshot(source, ordinal)?;
        if previous_base.is_some_and(|previous| {
            compare_unsigned(previous.name(), entry.name()) != core::cmp::Ordering::Less
        }) {
            return Err(CoreError::NonCanonicalOrder);
        }
        previous_base = Some(entry);
    }

    let mut previous_result: Option<TreeEntrySnapshotV1> = None;
    for result_ordinal in stream_start.min(result_count)..result_count {
        let result = read_result_snapshot(source, result_ordinal)?;
        if previous_result.is_some_and(|previous| {
            compare_unsigned(previous.name(), result.name()) != core::cmp::Ordering::Less
        }) {
            return Err(CoreError::NonCanonicalOrder);
        }
        let expected = match mutation {
            TreeProofMutationV1::Add(added) if result_ordinal == mutation_index => {
                if !result.matches(added) {
                    return Err(CoreError::Path);
                }
                None
            }
            TreeProofMutationV1::Add(_) => Some(if result_ordinal < mutation_index {
                result_ordinal
            } else {
                result_ordinal
                    .checked_sub(1)
                    .ok_or(CoreError::IntegerOverflow)?
            }),
            TreeProofMutationV1::Remove(_) => Some(if result_ordinal < mutation_index {
                result_ordinal
            } else {
                result_ordinal
                    .checked_add(1)
                    .ok_or(CoreError::IntegerOverflow)?
            }),
            TreeProofMutationV1::Move {
                insertion_index,
                moved,
                ..
            } if result_ordinal == insertion_index => {
                if !result.matches(moved) {
                    return Err(CoreError::Path);
                }
                None
            }
            TreeProofMutationV1::Move {
                removal_index,
                insertion_index,
                ..
            } => {
                let intermediate = if result_ordinal < insertion_index {
                    result_ordinal
                } else {
                    result_ordinal
                        .checked_sub(1)
                        .ok_or(CoreError::IntegerOverflow)?
                };
                Some(if intermediate < removal_index {
                    intermediate
                } else {
                    intermediate
                        .checked_add(1)
                        .ok_or(CoreError::IntegerOverflow)?
                })
            }
            TreeProofMutationV1::ReplacePair {
                first_index,
                first_replacement,
                ..
            } if result_ordinal == first_index => {
                if !result.matches(first_replacement) {
                    return Err(CoreError::Path);
                }
                None
            }
            TreeProofMutationV1::ReplacePair {
                second_index,
                second_replacement,
                ..
            } if result_ordinal == second_index => {
                if !result.matches(second_replacement) {
                    return Err(CoreError::Path);
                }
                None
            }
            TreeProofMutationV1::ReplacePair { .. } => Some(result_ordinal),
        };
        if let Some(base_ordinal) = expected {
            if read_base_snapshot(source, base_ordinal)? != result {
                return Err(CoreError::Path);
            }
        }
        previous_result = Some(result);
    }
    match mutation {
        TreeProofMutationV1::Remove(expected) => {
            if !read_base_snapshot(source, mutation_index)?.matches(expected) {
                return Err(CoreError::Path);
            }
        }
        TreeProofMutationV1::Move {
            removal_index,
            expected_removed,
            ..
        } => {
            if !read_base_snapshot(source, removal_index)?.matches(expected_removed) {
                return Err(CoreError::Path);
            }
        }
        TreeProofMutationV1::ReplacePair {
            first_index,
            first_expected,
            second_index,
            second_expected,
            ..
        } => {
            if !read_base_snapshot(source, first_index)?.matches(first_expected)
                || !read_base_snapshot(source, second_index)?.matches(second_expected)
            {
                return Err(CoreError::Path);
            }
        }
        TreeProofMutationV1::Add(_) => {}
    }
    Ok(())
}

fn logical_directory_header(mode: DirectoryBuildModeV1, count: usize) -> CoreResult<[u8; 19]> {
    let mut header = [0_u8; 19];
    header[..11].copy_from_slice(b"ESV2-DNODE\0");
    header[11..13].copy_from_slice(&1_u16.to_le_bytes());
    header[13..15].copy_from_slice(&mode.wire_mode()?.to_le_bytes());
    header[15..19].copy_from_slice(
        &u32::try_from(count)
            .map_err(|_| CoreError::IntegerOverflow)?
            .to_le_bytes(),
    );
    Ok(header)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn authenticate_and_derive_mutation_logical<
    T: CanonicalTreeMutationSourceV1 + ?Sized,
>(
    base: CanonicalDirectoryTreeV1,
    evidence: AuthenticatedTreeMutationEvidenceV1<'_>,
    mutation_index: usize,
    mutation: TreeProofMutationV1<'_>,
    source: &mut T,
    base_count: usize,
    result_count: usize,
    scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    counters: &mut OperationCountersV1,
) -> CoreResult<DirectoryLogicalIdentityV1> {
    let proof = evidence.logical;
    let expected_stream_start = mutation_index.saturating_sub(1);
    if proof.stream_start_index as usize != expected_stream_start
        || proof.old_preimage_len < 19
        || proof.stream_entry_offset > proof.old_preimage_len
    {
        return Err(CoreError::IdMismatch);
    }
    let tail_prefix_len =
        u64::try_from(proof.old_tail_prefix.len()).map_err(|_| CoreError::IntegerOverflow)?;
    let tail_offset = proof
        .stream_entry_offset
        .checked_sub(tail_prefix_len)
        .ok_or(CoreError::IdMismatch)?;
    if tail_offset % blake3::CHUNK_LEN as u64 != 0
        || proof.old_tail_prefix.len() >= blake3::CHUNK_LEN
    {
        return Err(CoreError::IdMismatch);
    }
    if tail_offset == 0 {
        if !proof.old_head.is_empty() || !proof.middle.is_empty() {
            return Err(CoreError::IdMismatch);
        }
        if proof.old_tail_prefix.get(..19)
            != Some(&logical_directory_header(base.mode, base_count)?)
        {
            return Err(CoreError::IdMismatch);
        }
    } else if proof.old_head.len() != blake3::CHUNK_LEN
        || proof.old_head.get(..19) != Some(&logical_directory_header(base.mode, base_count)?)
    {
        return Err(CoreError::IdMismatch);
    }

    let changed_len = match mutation {
        TreeProofMutationV1::Add(entry) | TreeProofMutationV1::Remove(entry) => {
            mutation_entry_encoded_len(entry)?
        }
        TreeProofMutationV1::Move { .. } | TreeProofMutationV1::ReplacePair { .. } => 0,
    };
    let result_preimage_len = match mutation {
        TreeProofMutationV1::Add(_) => proof
            .old_preimage_len
            .checked_add(changed_len)
            .ok_or(CoreError::IntegerOverflow)?,
        TreeProofMutationV1::Remove(_) => proof
            .old_preimage_len
            .checked_sub(changed_len)
            .ok_or(CoreError::IdMismatch)?,
        TreeProofMutationV1::Move {
            expected_removed,
            moved,
            ..
        } => {
            let without_old = proof
                .old_preimage_len
                .checked_sub(mutation_entry_encoded_len(expected_removed)?)
                .ok_or(CoreError::IdMismatch)?;
            without_old
                .checked_add(mutation_entry_encoded_len(moved)?)
                .ok_or(CoreError::IntegerOverflow)?
        }
        TreeProofMutationV1::ReplacePair { .. } => proof.old_preimage_len,
    };

    let old_digest = hash_streamed_directory_side(
        source,
        true,
        base_count,
        proof,
        proof.old_head,
        proof.old_tail_prefix,
        proof.old_preimage_len,
        counters,
    )?;
    if old_digest != directory_logical_bytes(base.logical) {
        return Err(CoreError::IdMismatch);
    }

    let (new_head, new_tail_prefix) = if tail_offset == 0 {
        let length = proof.old_tail_prefix.len();
        scratch[..length].copy_from_slice(proof.old_tail_prefix);
        scratch[15..19].copy_from_slice(
            &u32::try_from(result_count)
                .map_err(|_| CoreError::IntegerOverflow)?
                .to_le_bytes(),
        );
        (&[][..], &scratch[..length])
    } else {
        scratch[..blake3::CHUNK_LEN].copy_from_slice(proof.old_head);
        scratch[15..19].copy_from_slice(
            &u32::try_from(result_count)
                .map_err(|_| CoreError::IntegerOverflow)?
                .to_le_bytes(),
        );
        (&scratch[..blake3::CHUNK_LEN], proof.old_tail_prefix)
    };
    let new_digest = hash_streamed_directory_side(
        source,
        false,
        result_count,
        proof,
        new_head,
        new_tail_prefix,
        result_preimage_len,
        counters,
    )?;
    Ok(directory_logical_from_digest(base.mode, new_digest))
}

#[allow(clippy::too_many_arguments)]
fn hash_streamed_directory_side<T: CanonicalTreeMutationSourceV1 + ?Sized>(
    source: &mut T,
    base_side: bool,
    count: usize,
    proof: DirectoryMutationHashProofV1<'_>,
    head: &[u8],
    tail_prefix: &[u8],
    total_len: u64,
    counters: &mut OperationCountersV1,
) -> CoreResult<[u8; 32]> {
    let mut hasher = SparsePreimageHasherV1::new(total_len);
    hasher.write(head)?;
    for subtree in proof.middle {
        hasher.push_subtree(*subtree)?;
    }
    hasher.write(tail_prefix)?;
    let stream_start = proof.stream_start_index as usize;
    let mut previous: Option<TreeEntrySnapshotV1> = None;
    for ordinal in stream_start..count {
        let entry = if base_side {
            read_base_snapshot(source, ordinal)?
        } else {
            read_result_snapshot(source, ordinal)?
        };
        if previous.is_some_and(|left| {
            compare_unsigned(left.name(), entry.name()) != core::cmp::Ordering::Less
        }) {
            return Err(CoreError::NonCanonicalOrder);
        }
        let name_len = u32::from(entry.name_len);
        hasher.write(&name_len.to_le_bytes())?;
        hasher.write(entry.name())?;
        let (kind, id) = entry.child.logical().kind_and_id();
        hasher.write(&[kind])?;
        hasher.write(&id)?;
        counters.add(
            CounterFieldV1::BytesRead,
            u64::from(entry.name_len)
                .checked_add(37)
                .ok_or(CoreError::IntegerOverflow)?,
        )?;
        previous = Some(entry);
    }
    hasher.finish()
}

struct SparsePreimageHasherV1 {
    accumulator: HashProofAccumulatorV1,
    chunk: [u8; blake3::CHUNK_LEN],
    chunk_len: usize,
    committed: u64,
    total_len: u64,
}

impl SparsePreimageHasherV1 {
    fn new(total_len: u64) -> Self {
        Self {
            accumulator: HashProofAccumulatorV1::new(total_len),
            chunk: [0; blake3::CHUNK_LEN],
            chunk_len: 0,
            committed: 0,
            total_len,
        }
    }

    fn write(&mut self, mut bytes: &[u8]) -> CoreResult<()> {
        let written = self
            .committed
            .checked_add(self.chunk_len as u64)
            .and_then(|value| value.checked_add(bytes.len() as u64))
            .ok_or(CoreError::IntegerOverflow)?;
        if written > self.total_len {
            return Err(CoreError::TrailingBytes);
        }
        while !bytes.is_empty() {
            let take = (blake3::CHUNK_LEN - self.chunk_len).min(bytes.len());
            self.chunk[self.chunk_len..self.chunk_len + take].copy_from_slice(&bytes[..take]);
            self.chunk_len += take;
            bytes = &bytes[take..];
            if self.chunk_len == blake3::CHUNK_LEN && self.total_len > blake3::CHUNK_LEN as u64 {
                self.flush_chunk()?;
            }
        }
        Ok(())
    }

    fn push_subtree(&mut self, subtree: DirectoryHashSubtreeV1) -> CoreResult<()> {
        if self.chunk_len != 0 || self.total_len <= blake3::CHUNK_LEN as u64 {
            return Err(CoreError::IdMismatch);
        }
        let node = validate_hash_subtree(subtree, self.committed, self.total_len)?;
        self.committed = self
            .committed
            .checked_add(node.byte_len)
            .ok_or(CoreError::IntegerOverflow)?;
        self.accumulator.push(node)
    }

    fn flush_chunk(&mut self) -> CoreResult<()> {
        if self.chunk_len == 0 {
            return Ok(());
        }
        let mut hasher = blake3::Hasher::new();
        hasher.set_input_offset(self.committed);
        hasher.update(&self.chunk[..self.chunk_len]);
        let node = HashProofNodeV1 {
            offset: self.committed,
            byte_len: self.chunk_len as u64,
            chunks: 1,
            chaining_value: hasher.finalize_non_root(),
        };
        self.committed = self
            .committed
            .checked_add(self.chunk_len as u64)
            .ok_or(CoreError::IntegerOverflow)?;
        self.chunk_len = 0;
        self.accumulator.push(node)
    }

    fn finish(mut self) -> CoreResult<[u8; 32]> {
        if self.total_len <= blake3::CHUNK_LEN as u64 {
            if self.committed != 0 || self.chunk_len as u64 != self.total_len {
                return Err(CoreError::Truncated);
            }
            return Ok(*blake3::hash(&self.chunk[..self.chunk_len]).as_bytes());
        }
        self.flush_chunk()?;
        if self.committed != self.total_len {
            return Err(CoreError::Truncated);
        }
        self.accumulator.finish()
    }
}

struct BaseTreeEntryReader<'a, T: CanonicalTreeMutationSourceV1 + ?Sized> {
    source: &'a mut T,
}

impl<T: CanonicalTreeMutationSourceV1 + ?Sized> CanonicalTreeEntryReaderV1
    for BaseTreeEntryReader<'_, T>
{
    fn read_entry(&mut self, ordinal: usize) -> CoreResult<CanonicalTreeEntryV1<'_>> {
        let ordinal = u32::try_from(ordinal).map_err(|_| CoreError::IntegerOverflow)?;
        self.source
            .read_base_entry(ordinal)
            .map_err(map_tree_mutation_source)
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_mutation_physical_evidence<T: CanonicalTreeMutationSourceV1 + ?Sized>(
    base: CanonicalDirectoryTreeV1,
    plan: TreePlanV1,
    evidence: AuthenticatedTreeMutationEvidenceV1<'_>,
    mutation_index: usize,
    source: &mut T,
    scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
) -> CoreResult<Option<TreePageSummaryV1>> {
    if evidence.base_logical != base.logical
        || evidence.base_physical != base.physical
        || base.entry_count() as usize != plan.entry_count
        || base.page_depth() != plan.page_depth
        || base.leaf_count() as usize != plan.leaf_count
        || base.level_one_count() as usize != plan.level_one_count
    {
        return Err(CoreError::IdMismatch);
    }
    let mut verifier = VerificationTreeSinkV1;
    let mut ignored = OperationCountersV1::default();
    if plan.entry_count == 0 {
        if evidence.affected_leaf_index.is_some()
            || evidence.affected_leaf.is_some()
            || !evidence.leaf_group.is_empty()
            || !evidence.level_one_group.is_empty()
        {
            return Err(CoreError::IdMismatch);
        }
        let physical = encode_directory(
            base.mode().wire_mode()?,
            0,
            0,
            None,
            &mut verifier,
            &mut ignored,
            scratch,
        )?;
        if physical != base.physical() {
            return Err(CoreError::IdMismatch);
        }
        return Ok(None);
    }

    let base_index = mutation_index.min(plan.entry_count - 1);
    let leaf_index = base_index / TREE_LEAF_FANOUT;
    if evidence.affected_leaf_index != Some(leaf_index as u32) {
        return Err(CoreError::IdMismatch);
    }
    let first = leaf_index * TREE_LEAF_FANOUT;
    let end = (first + TREE_LEAF_FANOUT).min(plan.entry_count);
    let mut reader = BaseTreeEntryReader { source };
    let verified_leaf = encode_leaf_from_reader(
        &mut reader,
        first,
        end,
        &mut verifier,
        &mut ignored,
        scratch,
    )?;
    if evidence.affected_leaf != Some(verified_leaf) {
        return Err(CoreError::IdMismatch);
    }

    let verified_level_one = match plan.page_depth {
        0 => {
            if !evidence.leaf_group.is_empty() || !evidence.level_one_group.is_empty() {
                return Err(CoreError::IdMismatch);
            }
            None
        }
        1 | 2 => {
            let group = leaf_index / TREE_INDEX_FANOUT;
            let first_leaf = group * TREE_INDEX_FANOUT;
            let end_leaf = (first_leaf + TREE_INDEX_FANOUT).min(plan.leaf_count);
            validate_page_boundaries(evidence.leaf_group, 0, first_leaf, end_leaf, plan)?;
            let affected = evidence
                .leaf_group
                .get(leaf_index - first_leaf)
                .ok_or(CoreError::MissingClosureEdge)?;
            let first_entry = read_base_snapshot(reader.source, first)?;
            let last_entry = read_base_snapshot(reader.source, end - 1)?;
            if affected.summary != verified_leaf
                || affected.first_name.as_bytes() != first_entry.name()
                || affected.last_name.as_bytes() != last_entry.name()
            {
                return Err(CoreError::IdMismatch);
            }
            Some((
                group,
                encode_index_boundaries(
                    1,
                    evidence.leaf_group.iter().copied().map(Ok),
                    &mut verifier,
                    &mut ignored,
                    scratch,
                )?,
            ))
        }
        _ => return Err(CoreError::CountCap),
    };

    let root_page = match plan.page_depth {
        0 => verified_leaf,
        1 => verified_level_one
            .map(|(_, summary)| summary)
            .ok_or(CoreError::MissingClosureEdge)?,
        2 => {
            validate_page_boundaries(evidence.level_one_group, 1, 0, plan.level_one_count, plan)?;
            let (group, verified) = verified_level_one.ok_or(CoreError::MissingClosureEdge)?;
            if evidence
                .level_one_group
                .get(group)
                .map(|boundary| boundary.summary)
                != Some(verified)
            {
                return Err(CoreError::IdMismatch);
            }
            encode_index_boundaries(
                2,
                evidence.level_one_group.iter().copied().map(Ok),
                &mut verifier,
                &mut ignored,
                scratch,
            )?
        }
        _ => return Err(CoreError::CountCap),
    };
    let physical = encode_directory(
        base.mode().wire_mode()?,
        base.entry_count(),
        base.page_depth(),
        Some(root_page.id()),
        &mut verifier,
        &mut ignored,
        scratch,
    )?;
    if physical != base.physical() {
        return Err(CoreError::IdMismatch);
    }
    Ok(Some(root_page))
}

pub(crate) fn replacement_evidence_resident_bytes_v1(
    evidence: AuthenticatedTreeReplacementEvidenceV1<'_>,
) -> CoreResult<usize> {
    if evidence.logical.prefix.len() + evidence.logical.suffix.len()
        > MAX_DIRECTORY_HASH_PROOF_NODES
        || evidence.logical.old_window.len() > COMPARISON_WINDOW_BYTES
        || evidence.affected_entries.len() > TREE_LEAF_FANOUT
        || evidence.leaf_group.len() > TREE_INDEX_FANOUT
        || evidence.level_one_group.len() > TREE_INDEX_FANOUT
    {
        return Err(CoreError::ResourceRefused);
    }
    let mut bytes = evidence
        .logical
        .old_window
        .len()
        .checked_add(core::mem::size_of_val(evidence.logical.prefix))
        .and_then(|value| value.checked_add(core::mem::size_of_val(evidence.logical.suffix)))
        .and_then(|value| value.checked_add(core::mem::size_of_val(evidence.affected_entries)))
        .and_then(|value| value.checked_add(core::mem::size_of_val(evidence.leaf_group)))
        .and_then(|value| value.checked_add(core::mem::size_of_val(evidence.level_one_group)))
        .and_then(|value| {
            value
                .checked_add(core::mem::size_of::<HashProofNodeV1>() * HASH_PROOF_ACCUMULATOR_NODES)
        })
        .ok_or(CoreError::IntegerOverflow)?;
    for entry in evidence.affected_entries {
        bytes = bytes
            .checked_add(entry.name().as_bytes().len())
            .ok_or(CoreError::IntegerOverflow)?;
    }
    for boundary in evidence
        .leaf_group
        .iter()
        .chain(evidence.level_one_group.iter())
    {
        bytes = bytes
            .checked_add(boundary.first_name.as_bytes().len())
            .and_then(|value| value.checked_add(boundary.last_name.as_bytes().len()))
            .ok_or(CoreError::IntegerOverflow)?;
    }
    Ok(bytes)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn validate_replacement_evidence(
    base: CanonicalDirectoryTreeV1,
    plan: TreePlanV1,
    evidence: AuthenticatedTreeReplacementEvidenceV1<'_>,
    replacement_index: usize,
    replacement: CanonicalTreeEntryV1<'_>,
    object_scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
    logical_scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    counters: &mut OperationCountersV1,
) -> CoreResult<DirectoryLogicalIdentityV1> {
    let leaf_index = replacement_index / TREE_LEAF_FANOUT;
    let leaf_first = leaf_index
        .checked_mul(TREE_LEAF_FANOUT)
        .ok_or(CoreError::IntegerOverflow)?;
    let leaf_end = leaf_first
        .checked_add(TREE_LEAF_FANOUT)
        .ok_or(CoreError::IntegerOverflow)?
        .min(plan.entry_count);
    if evidence.base_logical != base.logical()
        || evidence.base_physical != base.physical()
        || base.entry_count() as usize != plan.entry_count
        || base.page_depth() != plan.page_depth
        || base.leaf_count() as usize != plan.leaf_count
        || base.level_one_count() as usize != plan.level_one_count
        || evidence.affected_leaf_index as usize != leaf_index
        || evidence.affected_entries.len() != leaf_end - leaf_first
        || evidence.affected_leaf.depth() != 0
        || evidence.affected_leaf.first_entry() as usize != leaf_first
        || evidence.affected_leaf.last_entry() as usize != leaf_end - 1
        || evidence.affected_leaf.subtree_entry_count() as usize != leaf_end - leaf_first
    {
        return Err(CoreError::IdMismatch);
    }

    let mut verifier = VerificationTreeSinkV1;
    let mut ignored = OperationCountersV1::default();
    let verified_leaf = encode_leaf(
        evidence.affected_entries.iter().copied(),
        leaf_first,
        leaf_end,
        &mut verifier,
        &mut ignored,
        object_scratch,
    )?;
    if verified_leaf != evidence.affected_leaf {
        return Err(CoreError::IdMismatch);
    }

    let verified_level_one = match plan.page_depth {
        0 => {
            if !evidence.leaf_group.is_empty() || !evidence.level_one_group.is_empty() {
                return Err(CoreError::IdMismatch);
            }
            None
        }
        1 | 2 => {
            let level_one_index = leaf_index / TREE_INDEX_FANOUT;
            let first_leaf = level_one_index
                .checked_mul(TREE_INDEX_FANOUT)
                .ok_or(CoreError::IntegerOverflow)?;
            let end_leaf = first_leaf
                .checked_add(TREE_INDEX_FANOUT)
                .ok_or(CoreError::IntegerOverflow)?
                .min(plan.leaf_count);
            validate_page_boundaries(evidence.leaf_group, 0, first_leaf, end_leaf, plan)?;
            let affected_boundary = evidence
                .leaf_group
                .get(leaf_index - first_leaf)
                .ok_or(CoreError::MissingClosureEdge)?;
            if affected_boundary.summary != verified_leaf
                || affected_boundary.first_name.as_bytes()
                    != evidence.affected_entries[0].name().as_bytes()
                || affected_boundary.last_name.as_bytes()
                    != evidence.affected_entries[evidence.affected_entries.len() - 1]
                        .name()
                        .as_bytes()
            {
                return Err(CoreError::IdMismatch);
            }
            Some((
                level_one_index,
                encode_index_boundaries(
                    1,
                    evidence.leaf_group.iter().copied().map(Ok),
                    &mut verifier,
                    &mut ignored,
                    object_scratch,
                )?,
            ))
        }
        _ => return Err(CoreError::CountCap),
    };

    let root_page = match plan.page_depth {
        0 => verified_leaf,
        1 => verified_level_one
            .map(|(_, summary)| summary)
            .ok_or(CoreError::MissingClosureEdge)?,
        2 => {
            validate_page_boundaries(evidence.level_one_group, 1, 0, plan.level_one_count, plan)?;
            let (level_one_index, verified) =
                verified_level_one.ok_or(CoreError::MissingClosureEdge)?;
            if evidence
                .level_one_group
                .get(level_one_index)
                .map(|boundary| boundary.summary)
                != Some(verified)
            {
                return Err(CoreError::IdMismatch);
            }
            encode_index_boundaries(
                2,
                evidence.level_one_group.iter().copied().map(Ok),
                &mut verifier,
                &mut ignored,
                object_scratch,
            )?
        }
        _ => return Err(CoreError::CountCap),
    };
    let physical = encode_directory(
        base.mode().wire_mode()?,
        base.entry_count(),
        base.page_depth(),
        Some(root_page.id()),
        &mut verifier,
        &mut ignored,
        object_scratch,
    )?;
    if physical != base.physical() {
        return Err(CoreError::IdMismatch);
    }

    derive_replacement_logical(
        base,
        evidence,
        replacement_index - leaf_first,
        replacement,
        logical_scratch,
        counters,
    )
}

struct VerificationTreeSinkV1;

impl PreparedTreeSinkV1 for VerificationTreeSinkV1 {
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64> {
        Ok(0)
    }

    fn begin_private_tree_set(&mut self, _maximum_objects: u32) -> Result<(), TreeSinkErrorV1> {
        Ok(())
    }

    fn admit_private_tree(
        &mut self,
        _id: PhysicalTreeIdV1,
        _canonical_bytes: &[u8],
    ) -> Result<TreeObjectDispositionV1, TreeSinkErrorV1> {
        Ok(TreeObjectDispositionV1::Created)
    }

    fn finish_private_tree_set(&mut self, _root: PhysicalTreeIdV1) -> Result<(), TreeSinkErrorV1> {
        Ok(())
    }

    fn abort_private_tree_set(&mut self) {}
}
