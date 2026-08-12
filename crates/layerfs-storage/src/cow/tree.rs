//! Canonical physical Tree construction and affected-spine replacement.
//!
//! Tree objects fit one frozen 55,736-byte caller-owned buffer. Page metadata
//! is retained only in a caller-owned, count-bounded summary area; payloads
//! are admitted immediately to a transaction-private sink. A same-name COW
//! replacement rebuilds one Leaf, its one-or-two Index ancestors, and the
//! Directory wrapper while reusing every unaffected immutable page identity.

use crate::format::{
    compare_unsigned, validate_directory_mode, validate_entry_count, DirectoryModeContext,
    PhysicalObjectKindV1, ValidatedComponent, MAX_ENTRIES, ROOT_DIRECTORY_MODE_SENTINEL_V1,
};
use crate::identity::{
    derive_explicit_directory_iter_v1, derive_implicit_root_directory_iter_v1,
    derive_physical_tree_id_v1, ExplicitDirectoryNodeV1, FileNodeIdV1, ImplicitRootDirectoryV1,
    LogicalChildIdV1, LogicalDirectoryEntryV1, PhysicalFileIdV1, PhysicalSymlinkIdV1,
    PhysicalTreeIdV1, SymlinkNodeIdV1, COMPARISON_WINDOW_BYTES, IDENTITY_HASHER_BYTES_V1,
};
use crate::limits::OperationReservationV1;
#[cfg(test)]
use crate::limits::ResourceLedgerV1;
use crate::limits::{
    CounterFieldV1, MemoryComponentV1, OperationCountersV1, OperationMemoryPlanV1,
};
use crate::object::{
    decode_physical_object_v1, encode_physical_object_header_v1, DiscardStrongEdgesV1,
    TypedPhysicalObjectIdV1,
};
use crate::{CoreError, CoreResult};
use blake3::hazmat::{merge_subtrees_non_root, merge_subtrees_root, HasherExt, Mode};

use super::view::{CanonicalTreeMutationSourceV1, TreeMutationSourceErrorV1};

pub const TREE_LEAF_FANOUT: usize = 192;
pub const TREE_INDEX_FANOUT: usize = 96;
pub const MAX_TREE_OBJECT_BYTES: usize = 55_736;
pub const MAX_TREE_PAGE_SUMMARIES: usize = 5_265;
pub const MAX_DIRECTORY_HASH_PROOF_NODES: usize = 64;
/// One mutable leaf group plus every possible depth-two root child. General
/// COW mutation never needs the complete directory page population resident.
pub const MAX_COW_TREE_PAGE_SUMMARIES: usize = TREE_INDEX_FANOUT + 55;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirectoryBuildModeV1 {
    ImplicitRoot,
    Explicit(u16),
}

impl DirectoryBuildModeV1 {
    fn wire_mode(self) -> CoreResult<u16> {
        match self {
            Self::ImplicitRoot => Ok(ROOT_DIRECTORY_MODE_SENTINEL_V1),
            Self::Explicit(mode) => {
                validate_directory_mode(mode, DirectoryModeContext::Explicit)?;
                Ok(mode)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CanonicalTreeChildV1 {
    Directory {
        logical: ExplicitDirectoryNodeV1,
        physical: PhysicalTreeIdV1,
    },
    File {
        logical: FileNodeIdV1,
        physical: PhysicalFileIdV1,
    },
    Symlink {
        logical: SymlinkNodeIdV1,
        physical: PhysicalSymlinkIdV1,
    },
}

impl CanonicalTreeChildV1 {
    const fn logical(self) -> LogicalChildIdV1 {
        match self {
            Self::Directory { logical, .. } => LogicalChildIdV1::Directory(logical),
            Self::File { logical, .. } => LogicalChildIdV1::File(logical),
            Self::Symlink { logical, .. } => LogicalChildIdV1::Symlink(logical),
        }
    }

    const fn physical_kind(self) -> u8 {
        match self {
            Self::Directory { .. } => 0x01,
            Self::File { .. } => 0x02,
            Self::Symlink { .. } => 0x03,
        }
    }

    const fn physical_id_ref(&self) -> &[u8; 32] {
        match self {
            Self::Directory { physical, .. } => physical.as_bytes(),
            Self::File { physical, .. } => physical.as_bytes(),
            Self::Symlink { physical, .. } => physical.as_bytes(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CanonicalTreeShapeV1 {
    entry_count: u32,
    page_depth: u8,
    leaf_count: u32,
    level_one_count: u32,
    page_summary_count: u32,
    tree_object_count: u32,
}

impl CanonicalTreeShapeV1 {
    pub const fn entry_count(self) -> u32 {
        self.entry_count
    }

    pub const fn page_depth(self) -> u8 {
        self.page_depth
    }

    pub const fn leaf_count(self) -> u32 {
        self.leaf_count
    }

    pub const fn level_one_count(self) -> u32 {
        self.level_one_count
    }

    pub const fn page_summary_count(self) -> u32 {
        self.page_summary_count
    }

    pub const fn tree_object_count(self) -> u32 {
        self.tree_object_count
    }
}

/// Validate the frozen entry/depth/fanout limits without allocating entries.
pub fn preflight_canonical_tree_v1(entry_count: u64) -> CoreResult<CanonicalTreeShapeV1> {
    validate_entry_count(entry_count)?;
    let entry_count = usize::try_from(entry_count).map_err(|_| CoreError::IntegerOverflow)?;
    TreePlanV1::for_count(entry_count)?.shape()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CanonicalTreeEntryV1<'a> {
    name: ValidatedComponent<'a>,
    child: CanonicalTreeChildV1,
}

impl<'a> CanonicalTreeEntryV1<'a> {
    pub const fn new(name: ValidatedComponent<'a>, child: CanonicalTreeChildV1) -> Self {
        Self { name, child }
    }

    pub const fn name(self) -> ValidatedComponent<'a> {
        self.name
    }

    pub const fn child(self) -> CanonicalTreeChildV1 {
        self.child
    }

    const fn logical_entry(self) -> LogicalDirectoryEntryV1<'a> {
        LogicalDirectoryEntryV1::new(self.name, self.child.logical())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirectoryLogicalIdentityV1 {
    ImplicitRoot(ImplicitRootDirectoryV1),
    Explicit(ExplicitDirectoryNodeV1),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TreeObjectDispositionV1 {
    Created,
    Reused,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TreeSinkErrorV1 {
    Failure,
}

/// Transaction-private immutable Tree sink. No object becomes visible through
/// this interface; closure admission owns the later visibility boundary.
pub trait PreparedTreeSinkV1 {
    /// Maximum transient userspace memory retained by this adapter. Durable
    /// immutable tree bytes are carrier storage. This declaration must not
    /// allocate or begin private preparation.
    fn resident_memory_bound_bytes(&self) -> CoreResult<u64>;
    fn begin_private_tree_set(&mut self, maximum_objects: u32) -> Result<(), TreeSinkErrorV1>;
    fn admit_private_tree(
        &mut self,
        id: PhysicalTreeIdV1,
        canonical_bytes: &[u8],
    ) -> Result<TreeObjectDispositionV1, TreeSinkErrorV1>;
    fn finish_private_tree_set(&mut self, root: PhysicalTreeIdV1) -> Result<(), TreeSinkErrorV1>;
    fn abort_private_tree_set(&mut self);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TreePageSummaryV1 {
    id: PhysicalTreeIdV1,
    depth: u8,
    first_entry: u32,
    last_entry: u32,
    subtree_entry_count: u32,
    object_len: u32,
}

impl TreePageSummaryV1 {
    pub const fn id(self) -> PhysicalTreeIdV1 {
        self.id
    }

    pub const fn depth(self) -> u8 {
        self.depth
    }

    pub const fn first_entry(self) -> u32 {
        self.first_entry
    }

    pub const fn last_entry(self) -> u32 {
        self.last_entry
    }

    pub const fn subtree_entry_count(self) -> u32 {
        self.subtree_entry_count
    }

    pub const fn object_len(self) -> u32 {
        self.object_len
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CanonicalDirectoryTreeV1 {
    logical: DirectoryLogicalIdentityV1,
    physical: PhysicalTreeIdV1,
    mode: DirectoryBuildModeV1,
    entry_count: u32,
    page_depth: u8,
    tree_object_count: u32,
    leaf_count: u32,
    level_one_count: u32,
}

impl CanonicalDirectoryTreeV1 {
    pub const fn logical(self) -> DirectoryLogicalIdentityV1 {
        self.logical
    }

    pub const fn physical(self) -> PhysicalTreeIdV1 {
        self.physical
    }

    pub const fn mode(self) -> DirectoryBuildModeV1 {
        self.mode
    }

    pub const fn entry_count(self) -> u32 {
        self.entry_count
    }

    pub const fn page_depth(self) -> u8 {
        self.page_depth
    }

    pub const fn tree_object_count(self) -> u32 {
        self.tree_object_count
    }

    pub const fn leaf_count(self) -> u32 {
        self.leaf_count
    }

    pub const fn level_one_count(self) -> u32 {
        self.level_one_count
    }
}

/// Sparse authentication for add/remove. Bytes before `stream_entry_offset`
/// are represented by one exact first chunk, canonical middle subtrees, and
/// at most one partial tail prefix. Entries from `stream_start_index` onward
/// are streamed and hashed; the same proof is reused after changing only the
/// frozen count field and the streamed suffix.
#[derive(Clone, Copy, Debug)]
pub struct DirectoryMutationHashProofV1<'a> {
    old_preimage_len: u64,
    stream_start_index: u32,
    stream_entry_offset: u64,
    old_head: &'a [u8],
    middle: &'a [DirectoryHashSubtreeV1],
    old_tail_prefix: &'a [u8],
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

/// Bounded physical-spine and sparse-logical evidence for add/remove. The
/// leaf group is at most 96 summaries and the root group at most 55; no full
/// base-tree summary or entry array is accepted by this interface.
#[derive(Clone, Copy, Debug)]
pub struct AuthenticatedTreeMutationEvidenceV1<'a> {
    base_logical: DirectoryLogicalIdentityV1,
    base_physical: PhysicalTreeIdV1,
    affected_leaf_index: Option<u32>,
    affected_leaf: Option<TreePageSummaryV1>,
    leaf_group: &'a [TreePageBoundaryV1<'a>],
    level_one_group: &'a [TreePageBoundaryV1<'a>],
    logical: DirectoryMutationHashProofV1<'a>,
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
            base_logical: base.logical,
            base_physical: base.physical,
            affected_leaf_index,
            affected_leaf,
            leaf_group,
            level_one_group,
            logical,
        }
    }
}

/// One authenticated BLAKE3 subtree in a sparse proof of the frozen flat
/// logical-directory preimage. The chaining value is not an identity and is
/// accepted only when the complete proof recomputes the admitted base root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DirectoryHashSubtreeV1 {
    offset: u64,
    byte_len: u64,
    chaining_value: [u8; 32],
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

/// Sparse proof covering every byte outside one bounded, chunk-aligned
/// logical-directory window. The old window is authenticated by recomputing
/// the complete base digest before any private tree output begins.
#[derive(Clone, Copy, Debug)]
pub struct DirectoryHashProofV1<'a> {
    preimage_len: u64,
    window_offset: u64,
    old_window: &'a [u8],
    prefix: &'a [DirectoryHashSubtreeV1],
    suffix: &'a [DirectoryHashSubtreeV1],
    affected_leaf_offset: u64,
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

/// A page summary plus the exact boundary names committed by its parent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TreePageBoundaryV1<'a> {
    summary: TreePageSummaryV1,
    first_name: ValidatedComponent<'a>,
    last_name: ValidatedComponent<'a>,
}

impl<'a> TreePageBoundaryV1<'a> {
    pub const fn new(
        summary: TreePageSummaryV1,
        first_name: ValidatedComponent<'a>,
        last_name: ValidatedComponent<'a>,
    ) -> Self {
        Self {
            summary,
            first_name,
            last_name,
        }
    }

    pub const fn summary(self) -> TreePageSummaryV1 {
        self.summary
    }
}

/// Bounded evidence for a same-name replacement. It contains only the
/// affected leaf, the direct sibling summaries needed to authenticate its
/// physical spine, and a sparse proof of the flat logical identity.
#[derive(Clone, Copy, Debug)]
pub struct AuthenticatedTreeReplacementEvidenceV1<'a> {
    base_logical: DirectoryLogicalIdentityV1,
    base_physical: PhysicalTreeIdV1,
    affected_leaf_index: u32,
    affected_entries: &'a [CanonicalTreeEntryV1<'a>],
    affected_leaf: TreePageSummaryV1,
    leaf_group: &'a [TreePageBoundaryV1<'a>],
    level_one_group: &'a [TreePageBoundaryV1<'a>],
    logical: DirectoryHashProofV1<'a>,
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
            base_logical: base.logical,
            base_physical: base.physical,
            affected_leaf_index,
            affected_entries,
            affected_leaf,
            leaf_group,
            level_one_group,
            logical,
        }
    }

    /// Return the exact old entry covered by this bounded replacement proof.
    /// Complete mutation orchestration uses it to bind a separately
    /// authenticated base file to the accepted tree before preparing output.
    pub(crate) fn expected_entry_v1(
        self,
        replacement_index: usize,
    ) -> CoreResult<CanonicalTreeEntryV1<'a>> {
        let leaf_first = usize::try_from(self.affected_leaf.first_entry)
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CowTreeReplacementV1 {
    directory: CanonicalDirectoryTreeV1,
    changed_leaf_index: u32,
    changed_leaf: TreePageSummaryV1,
    changed_level_one: Option<(u32, TreePageSummaryV1)>,
}

impl CowTreeReplacementV1 {
    pub const fn directory(self) -> CanonicalDirectoryTreeV1 {
        self.directory
    }

    pub const fn changed_leaf_index(self) -> u32 {
        self.changed_leaf_index
    }

    pub const fn changed_leaf(self) -> TreePageSummaryV1 {
        self.changed_leaf
    }

    pub const fn changed_level_one(self) -> Option<(u32, TreePageSummaryV1)> {
        self.changed_level_one
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CowTreeMutationV1 {
    directory: CanonicalDirectoryTreeV1,
    first_changed_leaf: u32,
    structurally_reused_leaves: u32,
    structurally_reused_level_one: u32,
    emitted_objects: u32,
}

impl CowTreeMutationV1 {
    pub const fn directory(self) -> CanonicalDirectoryTreeV1 {
        self.directory
    }

    pub const fn first_changed_leaf(self) -> u32 {
        self.first_changed_leaf
    }

    pub const fn structurally_reused_leaves(self) -> u32 {
        self.structurally_reused_leaves
    }

    pub const fn structurally_reused_level_one(self) -> u32 {
        self.structurally_reused_level_one
    }

    pub const fn emitted_objects(self) -> u32 {
        self.emitted_objects
    }
}

#[cfg(test)]
pub fn build_canonical_directory_v1<S: PreparedTreeSinkV1 + ?Sized>(
    mode: DirectoryBuildModeV1,
    entries: &[CanonicalTreeEntryV1<'_>],
    sink: &mut S,
    ledger: &ResourceLedgerV1,
    counters: &mut OperationCountersV1,
    object_scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
    page_scratch: &mut [Option<TreePageSummaryV1>],
) -> CoreResult<CanonicalDirectoryTreeV1> {
    let plan = TreePlanV1::for_count(entries.len())?;
    validate_entries(entries)?;
    mode.wire_mode()?;
    if page_scratch.len() < plan.summary_count {
        return Err(CoreError::ResourceRefused);
    }
    let memory = OperationMemoryPlanV1::empty()
        .charge(
            MemoryComponentV1::ObjectScratch,
            object_scratch.len() as u64,
        )?
        .charge(
            MemoryComponentV1::PageSummaries,
            core::mem::size_of_val(page_scratch) as u64,
        )?
        .charge(MemoryComponentV1::HashState, IDENTITY_HASHER_BYTES_V1)?
        .charge(
            MemoryComponentV1::MetadataWindow,
            sink.resident_memory_bound_bytes()?,
        )?;
    let _reservation = ledger.reserve_operation_with_plan(memory)?;
    counters.memory_high_water = counters.memory_high_water.max(ledger.high_water_bytes());
    let logical = derive_logical(mode, entries.len(), entries.iter().copied())?;
    sink.begin_private_tree_set(plan.tree_object_count)
        .map_err(map_sink)?;
    let result = build_directory_inner(
        mode,
        entries,
        logical,
        plan,
        sink,
        counters,
        object_scratch,
        page_scratch,
    );
    if result.is_err() {
        sink.abort_private_tree_set();
    }
    result
}

#[cfg(feature = "operation-polymorphism")]
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_canonical_directory_borrowed_v1<S: PreparedTreeSinkV1 + ?Sized>(
    mode: DirectoryBuildModeV1,
    entries: &[CanonicalTreeEntryV1<'_>],
    sink: &mut S,
    reservation: &OperationReservationV1<'_>,
    counters: &mut OperationCountersV1,
    object_scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
    page_scratch: &mut [Option<TreePageSummaryV1>],
) -> CoreResult<CanonicalDirectoryTreeV1> {
    let plan = TreePlanV1::for_count(entries.len())?;
    validate_entries(entries)?;
    mode.wire_mode()?;
    if page_scratch.len() < plan.summary_count {
        return Err(CoreError::ResourceRefused);
    }
    let memory = OperationMemoryPlanV1::empty()
        .charge(
            MemoryComponentV1::ObjectScratch,
            object_scratch.len() as u64,
        )?
        .charge(
            MemoryComponentV1::PageSummaries,
            core::mem::size_of_val(page_scratch) as u64,
        )?
        .charge(MemoryComponentV1::HashState, IDENTITY_HASHER_BYTES_V1)?
        .charge(
            MemoryComponentV1::MetadataWindow,
            sink.resident_memory_bound_bytes()?,
        )?;
    reservation.require(memory)?;
    let logical = derive_logical(mode, entries.len(), entries.iter().copied())?;
    sink.begin_private_tree_set(plan.tree_object_count)
        .map_err(map_sink)?;
    let result = build_directory_inner(
        mode,
        entries,
        logical,
        plan,
        sink,
        counters,
        object_scratch,
        page_scratch,
    );
    if result.is_err() {
        sink.abort_private_tree_set();
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn build_directory_inner<S: PreparedTreeSinkV1 + ?Sized>(
    mode: DirectoryBuildModeV1,
    entries: &[CanonicalTreeEntryV1<'_>],
    logical: DirectoryLogicalIdentityV1,
    plan: TreePlanV1,
    sink: &mut S,
    counters: &mut OperationCountersV1,
    object_scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
    page_scratch: &mut [Option<TreePageSummaryV1>],
) -> CoreResult<CanonicalDirectoryTreeV1> {
    for (leaf, target) in page_scratch.iter_mut().enumerate().take(plan.leaf_count) {
        let first = leaf
            .checked_mul(TREE_LEAF_FANOUT)
            .ok_or(CoreError::IntegerOverflow)?;
        let end = first
            .checked_add(TREE_LEAF_FANOUT)
            .ok_or(CoreError::IntegerOverflow)?
            .min(entries.len());
        let summary = encode_leaf(
            entries[first..end].iter().copied(),
            first,
            end,
            sink,
            counters,
            object_scratch,
        )?;
        *target = Some(summary);
    }

    let level_one_start = plan.leaf_count;
    for group in 0..plan.level_one_count {
        let first_leaf = group
            .checked_mul(TREE_INDEX_FANOUT)
            .ok_or(CoreError::IntegerOverflow)?;
        let end_leaf = first_leaf
            .checked_add(TREE_INDEX_FANOUT)
            .ok_or(CoreError::IntegerOverflow)?
            .min(plan.leaf_count);
        let children = page_scratch[first_leaf..end_leaf]
            .iter()
            .map(|entry| entry.ok_or(CoreError::MissingClosureEdge));
        let summary = encode_index(1, children, entries, sink, counters, object_scratch)?;
        page_scratch[level_one_start + group] = Some(summary);
    }

    let root_page = match plan.page_depth {
        0 if entries.is_empty() => None,
        0 => page_scratch[0],
        1 => page_scratch[level_one_start],
        2 => {
            let children = page_scratch[level_one_start..level_one_start + plan.level_one_count]
                .iter()
                .map(|entry| entry.ok_or(CoreError::MissingClosureEdge));
            let root = encode_index(2, children, entries, sink, counters, object_scratch)?;
            page_scratch[level_one_start + plan.level_one_count] = Some(root);
            Some(root)
        }
        _ => return Err(CoreError::CountCap),
    };
    let physical = encode_directory(
        mode.wire_mode()?,
        u32::try_from(entries.len()).map_err(|_| CoreError::IntegerOverflow)?,
        plan.page_depth,
        root_page.map(TreePageSummaryV1::id),
        sink,
        counters,
        object_scratch,
    )?;
    sink.finish_private_tree_set(physical).map_err(map_sink)?;
    Ok(CanonicalDirectoryTreeV1 {
        logical,
        physical,
        mode,
        entry_count: u32::try_from(entries.len()).map_err(|_| CoreError::IntegerOverflow)?,
        page_depth: plan.page_depth,
        tree_object_count: plan.tree_object_count,
        leaf_count: u32::try_from(plan.leaf_count).map_err(|_| CoreError::IntegerOverflow)?,
        level_one_count: u32::try_from(plan.level_one_count)
            .map_err(|_| CoreError::IntegerOverflow)?,
    })
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(super) fn replace_directory_entry_cow_impl_v1<S: PreparedTreeSinkV1 + ?Sized>(
    base: CanonicalDirectoryTreeV1,
    evidence: AuthenticatedTreeReplacementEvidenceV1<'_>,
    replacement_index: usize,
    replacement: CanonicalTreeEntryV1<'_>,
    sink: &mut S,
    ledger: &ResourceLedgerV1,
    counters: &mut OperationCountersV1,
    object_scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
    logical_scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
) -> CoreResult<CowTreeReplacementV1> {
    replace_directory_entry_cow_with_admission_v1(
        base,
        evidence,
        replacement_index,
        replacement,
        sink,
        TreeMemoryAdmissionV1::Independent(ledger),
        counters,
        object_scratch,
        logical_scratch,
    )
}

#[cfg(feature = "operation-polymorphism")]
#[allow(clippy::too_many_arguments)]
pub(super) fn replace_directory_entry_cow_borrowed_impl_v1<S: PreparedTreeSinkV1 + ?Sized>(
    base: CanonicalDirectoryTreeV1,
    evidence: AuthenticatedTreeReplacementEvidenceV1<'_>,
    replacement_index: usize,
    replacement: CanonicalTreeEntryV1<'_>,
    sink: &mut S,
    reservation: &OperationReservationV1<'_>,
    counters: &mut OperationCountersV1,
    object_scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
    logical_scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
) -> CoreResult<CowTreeReplacementV1> {
    replace_directory_entry_cow_with_admission_v1(
        base,
        evidence,
        replacement_index,
        replacement,
        sink,
        TreeMemoryAdmissionV1::Borrowed(reservation),
        counters,
        object_scratch,
        logical_scratch,
    )
}

#[derive(Clone, Copy)]
enum TreeMemoryAdmissionV1<'a> {
    #[cfg(test)]
    Independent(&'a ResourceLedgerV1),
    Borrowed(&'a OperationReservationV1<'a>),
}

#[allow(clippy::too_many_arguments)]
fn replace_directory_entry_cow_with_admission_v1<S: PreparedTreeSinkV1 + ?Sized>(
    base: CanonicalDirectoryTreeV1,
    evidence: AuthenticatedTreeReplacementEvidenceV1<'_>,
    replacement_index: usize,
    replacement: CanonicalTreeEntryV1<'_>,
    sink: &mut S,
    admission: TreeMemoryAdmissionV1<'_>,
    counters: &mut OperationCountersV1,
    object_scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
    logical_scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
) -> CoreResult<CowTreeReplacementV1> {
    let plan = TreePlanV1::for_count(base.entry_count as usize)?;
    let leaf_index = replacement_index / TREE_LEAF_FANOUT;
    if replacement_index >= base.entry_count as usize
        || evidence.affected_leaf_index as usize != leaf_index
    {
        return Err(CoreError::Path);
    }
    let leaf_first = leaf_index
        .checked_mul(TREE_LEAF_FANOUT)
        .ok_or(CoreError::IntegerOverflow)?;
    let leaf_end = leaf_first
        .checked_add(TREE_LEAF_FANOUT)
        .ok_or(CoreError::IntegerOverflow)?
        .min(base.entry_count as usize);
    if evidence.affected_entries.len()
        != leaf_end
            .checked_sub(leaf_first)
            .ok_or(CoreError::IntegerOverflow)?
    {
        return Err(CoreError::IdMismatch);
    }
    validate_entries(evidence.affected_entries)?;
    let relative = replacement_index
        .checked_sub(leaf_first)
        .ok_or(CoreError::IntegerOverflow)?;
    let old = evidence
        .affected_entries
        .get(relative)
        .ok_or(CoreError::Path)?;
    if old.name.as_bytes() != replacement.name.as_bytes() {
        return Err(CoreError::Path);
    }
    let evidence_bytes = replacement_evidence_resident_bytes_v1(evidence)?;
    let memory = OperationMemoryPlanV1::empty()
        .charge(
            MemoryComponentV1::ObjectScratch,
            object_scratch.len() as u64,
        )?
        .charge(
            MemoryComponentV1::ComparisonWindow,
            logical_scratch.len() as u64,
        )?
        .charge(MemoryComponentV1::EvidenceWindow, evidence_bytes as u64)?
        .charge(MemoryComponentV1::HashState, IDENTITY_HASHER_BYTES_V1)?
        .charge(
            MemoryComponentV1::MetadataWindow,
            sink.resident_memory_bound_bytes()?,
        )?;
    match admission {
        #[cfg(test)]
        TreeMemoryAdmissionV1::Independent(ledger) => {
            let _reservation = ledger.reserve_operation_with_plan(memory)?;
            counters.memory_high_water = counters.memory_high_water.max(ledger.high_water_bytes());
            replace_directory_entry_after_admission_v1(
                base,
                plan,
                evidence,
                replacement_index,
                replacement,
                sink,
                counters,
                object_scratch,
                logical_scratch,
            )
        }
        TreeMemoryAdmissionV1::Borrowed(reservation) => {
            reservation.require(memory)?;
            replace_directory_entry_after_admission_v1(
                base,
                plan,
                evidence,
                replacement_index,
                replacement,
                sink,
                counters,
                object_scratch,
                logical_scratch,
            )
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn replace_directory_entry_after_admission_v1<S: PreparedTreeSinkV1 + ?Sized>(
    base: CanonicalDirectoryTreeV1,
    plan: TreePlanV1,
    evidence: AuthenticatedTreeReplacementEvidenceV1<'_>,
    replacement_index: usize,
    replacement: CanonicalTreeEntryV1<'_>,
    sink: &mut S,
    counters: &mut OperationCountersV1,
    object_scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
    logical_scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
) -> CoreResult<CowTreeReplacementV1> {
    let logical = validate_replacement_evidence(
        base,
        plan,
        evidence,
        replacement_index,
        replacement,
        object_scratch,
        logical_scratch,
        counters,
    )?;
    let changed_objects = u32::from(plan.page_depth)
        .checked_add(2)
        .ok_or(CoreError::IntegerOverflow)?;
    sink.begin_private_tree_set(changed_objects)
        .map_err(map_sink)?;
    let result = replace_inner(
        base,
        evidence,
        replacement_index,
        replacement,
        logical,
        plan,
        sink,
        counters,
        object_scratch,
    );
    if result.is_err() {
        sink.abort_private_tree_set();
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn replace_inner<S: PreparedTreeSinkV1 + ?Sized>(
    base: CanonicalDirectoryTreeV1,
    evidence: AuthenticatedTreeReplacementEvidenceV1<'_>,
    replacement_index: usize,
    replacement: CanonicalTreeEntryV1<'_>,
    logical: DirectoryLogicalIdentityV1,
    plan: TreePlanV1,
    sink: &mut S,
    counters: &mut OperationCountersV1,
    object_scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
) -> CoreResult<CowTreeReplacementV1> {
    let leaf_index = replacement_index / TREE_LEAF_FANOUT;
    let leaf_first = leaf_index
        .checked_mul(TREE_LEAF_FANOUT)
        .ok_or(CoreError::IntegerOverflow)?;
    let leaf_end = leaf_first
        .checked_add(TREE_LEAF_FANOUT)
        .ok_or(CoreError::IntegerOverflow)?
        .min(base.entry_count as usize);
    let relative_replacement = replacement_index
        .checked_sub(leaf_first)
        .ok_or(CoreError::IntegerOverflow)?;
    let changed_leaf = encode_leaf(
        evidence
            .affected_entries
            .iter()
            .enumerate()
            .map(|(relative, entry)| {
                if relative == relative_replacement {
                    replacement
                } else {
                    *entry
                }
            }),
        leaf_first,
        leaf_end,
        sink,
        counters,
        object_scratch,
    )?;

    let mut changed_level_one = None;
    let root_page = match plan.page_depth {
        0 => changed_leaf,
        1 | 2 => {
            let level_one_index = leaf_index / TREE_INDEX_FANOUT;
            let first_leaf = level_one_index
                .checked_mul(TREE_INDEX_FANOUT)
                .ok_or(CoreError::IntegerOverflow)?;
            let level_one = encode_index_boundaries(
                1,
                evidence
                    .leaf_group
                    .iter()
                    .enumerate()
                    .map(|(relative, boundary)| {
                        Ok(if first_leaf + relative == leaf_index {
                            TreePageBoundaryV1 {
                                summary: changed_leaf,
                                ..*boundary
                            }
                        } else {
                            *boundary
                        })
                    }),
                sink,
                counters,
                object_scratch,
            )?;
            changed_level_one = Some((
                u32::try_from(level_one_index).map_err(|_| CoreError::IntegerOverflow)?,
                level_one,
            ));
            if plan.page_depth == 1 {
                level_one
            } else {
                encode_index_boundaries(
                    2,
                    evidence
                        .level_one_group
                        .iter()
                        .enumerate()
                        .map(|(index, boundary)| {
                            Ok(if index == level_one_index {
                                TreePageBoundaryV1 {
                                    summary: level_one,
                                    ..*boundary
                                }
                            } else {
                                *boundary
                            })
                        }),
                    sink,
                    counters,
                    object_scratch,
                )?
            }
        }
        _ => return Err(CoreError::CountCap),
    };
    let physical = encode_directory(
        base.mode.wire_mode()?,
        base.entry_count,
        base.page_depth,
        Some(root_page.id),
        sink,
        counters,
        object_scratch,
    )?;
    sink.finish_private_tree_set(physical).map_err(map_sink)?;

    account_replacement_reuse(evidence, leaf_index, plan.page_depth, counters)?;
    Ok(CowTreeReplacementV1 {
        directory: CanonicalDirectoryTreeV1 {
            logical,
            physical,
            ..base
        },
        changed_leaf_index: u32::try_from(leaf_index).map_err(|_| CoreError::IntegerOverflow)?,
        changed_leaf,
        changed_level_one,
    })
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(super) fn add_directory_entry_cow_impl_v1<T, S>(
    base: CanonicalDirectoryTreeV1,
    evidence: AuthenticatedTreeMutationEvidenceV1<'_>,
    insertion_index: usize,
    added: CanonicalTreeEntryV1<'_>,
    source: &mut T,
    sink: &mut S,
    ledger: &ResourceLedgerV1,
    counters: &mut OperationCountersV1,
    object_scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
    logical_scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    page_scratch: &mut [Option<TreePageSummaryV1>],
) -> CoreResult<CowTreeMutationV1>
where
    T: CanonicalTreeMutationSourceV1 + ?Sized,
    S: PreparedTreeSinkV1 + ?Sized,
{
    mutate_directory_entries_cow_v1(
        base,
        evidence,
        insertion_index,
        TreeMutationKindV1::Add(added),
        source,
        sink,
        TreeMemoryAdmissionV1::Independent(ledger),
        counters,
        object_scratch,
        logical_scratch,
        page_scratch,
    )
}

#[cfg(feature = "operation-polymorphism")]
#[allow(clippy::too_many_arguments)]
pub(super) fn add_directory_entry_cow_borrowed_impl_v1<T, S>(
    base: CanonicalDirectoryTreeV1,
    evidence: AuthenticatedTreeMutationEvidenceV1<'_>,
    insertion_index: usize,
    added: CanonicalTreeEntryV1<'_>,
    source: &mut T,
    sink: &mut S,
    reservation: &OperationReservationV1<'_>,
    counters: &mut OperationCountersV1,
    object_scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
    logical_scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    page_scratch: &mut [Option<TreePageSummaryV1>],
) -> CoreResult<CowTreeMutationV1>
where
    T: CanonicalTreeMutationSourceV1 + ?Sized,
    S: PreparedTreeSinkV1 + ?Sized,
{
    mutate_directory_entries_cow_v1(
        base,
        evidence,
        insertion_index,
        TreeMutationKindV1::Add(added),
        source,
        sink,
        TreeMemoryAdmissionV1::Borrowed(reservation),
        counters,
        object_scratch,
        logical_scratch,
        page_scratch,
    )
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(super) fn remove_directory_entry_cow_impl_v1<T, S>(
    base: CanonicalDirectoryTreeV1,
    evidence: AuthenticatedTreeMutationEvidenceV1<'_>,
    removal_index: usize,
    expected_removed: CanonicalTreeEntryV1<'_>,
    source: &mut T,
    sink: &mut S,
    ledger: &ResourceLedgerV1,
    counters: &mut OperationCountersV1,
    object_scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
    logical_scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    page_scratch: &mut [Option<TreePageSummaryV1>],
) -> CoreResult<CowTreeMutationV1>
where
    T: CanonicalTreeMutationSourceV1 + ?Sized,
    S: PreparedTreeSinkV1 + ?Sized,
{
    mutate_directory_entries_cow_v1(
        base,
        evidence,
        removal_index,
        TreeMutationKindV1::Remove(expected_removed),
        source,
        sink,
        TreeMemoryAdmissionV1::Independent(ledger),
        counters,
        object_scratch,
        logical_scratch,
        page_scratch,
    )
}

#[cfg(feature = "operation-polymorphism")]
#[allow(clippy::too_many_arguments)]
pub(super) fn remove_directory_entry_cow_borrowed_impl_v1<T, S>(
    base: CanonicalDirectoryTreeV1,
    evidence: AuthenticatedTreeMutationEvidenceV1<'_>,
    removal_index: usize,
    expected_removed: CanonicalTreeEntryV1<'_>,
    source: &mut T,
    sink: &mut S,
    reservation: &OperationReservationV1<'_>,
    counters: &mut OperationCountersV1,
    object_scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
    logical_scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    page_scratch: &mut [Option<TreePageSummaryV1>],
) -> CoreResult<CowTreeMutationV1>
where
    T: CanonicalTreeMutationSourceV1 + ?Sized,
    S: PreparedTreeSinkV1 + ?Sized,
{
    mutate_directory_entries_cow_v1(
        base,
        evidence,
        removal_index,
        TreeMutationKindV1::Remove(expected_removed),
        source,
        sink,
        TreeMemoryAdmissionV1::Borrowed(reservation),
        counters,
        object_scratch,
        logical_scratch,
        page_scratch,
    )
}

#[cfg(feature = "operation-polymorphism")]
#[allow(clippy::too_many_arguments)]
pub(super) fn move_directory_entry_cow_borrowed_impl_v1<T, S>(
    base: CanonicalDirectoryTreeV1,
    evidence: AuthenticatedTreeMutationEvidenceV1<'_>,
    removal_index: usize,
    insertion_index: usize,
    expected_removed: CanonicalTreeEntryV1<'_>,
    moved: CanonicalTreeEntryV1<'_>,
    source: &mut T,
    sink: &mut S,
    reservation: &OperationReservationV1<'_>,
    counters: &mut OperationCountersV1,
    object_scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
    logical_scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    page_scratch: &mut [Option<TreePageSummaryV1>],
) -> CoreResult<CowTreeMutationV1>
where
    T: CanonicalTreeMutationSourceV1 + ?Sized,
    S: PreparedTreeSinkV1 + ?Sized,
{
    mutate_directory_entries_cow_v1(
        base,
        evidence,
        removal_index.min(insertion_index),
        TreeMutationKindV1::Move {
            removal_index,
            insertion_index,
            expected_removed,
            moved,
        },
        source,
        sink,
        TreeMemoryAdmissionV1::Borrowed(reservation),
        counters,
        object_scratch,
        logical_scratch,
        page_scratch,
    )
}

/// Replace two distinct entries in one authenticated directory snapshot.
/// This is the root-spine primitive used by a cross-directory Move after the
/// source detach and destination attach candidates have both been prepared.
/// The caller supplies the one final result view; no intermediate root is
/// emitted or made visible.
#[cfg(feature = "operation-polymorphism")]
#[allow(clippy::too_many_arguments)]
pub(super) fn replace_two_directory_entries_cow_borrowed_impl_v1<T, S>(
    base: CanonicalDirectoryTreeV1,
    evidence: AuthenticatedTreeMutationEvidenceV1<'_>,
    first_index: usize,
    first_expected: CanonicalTreeEntryV1<'_>,
    first_replacement: CanonicalTreeEntryV1<'_>,
    second_index: usize,
    second_expected: CanonicalTreeEntryV1<'_>,
    second_replacement: CanonicalTreeEntryV1<'_>,
    source: &mut T,
    sink: &mut S,
    reservation: &OperationReservationV1<'_>,
    counters: &mut OperationCountersV1,
    object_scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
    logical_scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    page_scratch: &mut [Option<TreePageSummaryV1>],
) -> CoreResult<CowTreeMutationV1>
where
    T: CanonicalTreeMutationSourceV1 + ?Sized,
    S: PreparedTreeSinkV1 + ?Sized,
{
    mutate_directory_entries_cow_v1(
        base,
        evidence,
        first_index.min(second_index),
        TreeMutationKindV1::ReplacePair {
            first_index,
            first_expected,
            first_replacement,
            second_index,
            second_expected,
            second_replacement,
        },
        source,
        sink,
        TreeMemoryAdmissionV1::Borrowed(reservation),
        counters,
        object_scratch,
        logical_scratch,
        page_scratch,
    )
}

#[derive(Clone, Copy)]
enum TreeMutationKindV1<'a> {
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

#[allow(clippy::too_many_arguments)]
fn mutate_directory_entries_cow_v1<T, S>(
    base: CanonicalDirectoryTreeV1,
    evidence: AuthenticatedTreeMutationEvidenceV1<'_>,
    mutation_index: usize,
    mutation: TreeMutationKindV1<'_>,
    source: &mut T,
    sink: &mut S,
    admission: TreeMemoryAdmissionV1<'_>,
    counters: &mut OperationCountersV1,
    object_scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
    logical_scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    page_scratch: &mut [Option<TreePageSummaryV1>],
) -> CoreResult<CowTreeMutationV1>
where
    T: CanonicalTreeMutationSourceV1 + ?Sized,
    S: PreparedTreeSinkV1 + ?Sized,
{
    let base_count = usize::try_from(source.declared_base_entry_count()?)
        .map_err(|_| CoreError::IntegerOverflow)?;
    let result_count = usize::try_from(source.declared_result_entry_count()?)
        .map_err(|_| CoreError::IntegerOverflow)?;
    if base_count != base.entry_count as usize {
        return Err(CoreError::IdMismatch);
    }
    match mutation {
        TreeMutationKindV1::Add(_) => {
            if mutation_index > base_count
                || result_count
                    != base_count
                        .checked_add(1)
                        .ok_or(CoreError::IntegerOverflow)?
            {
                return Err(CoreError::Path);
            }
        }
        TreeMutationKindV1::Remove(_) => {
            if mutation_index >= base_count || result_count.checked_add(1) != Some(base_count) {
                return Err(CoreError::Path);
            }
        }
        TreeMutationKindV1::Move {
            removal_index,
            insertion_index,
            ..
        } => {
            if base_count == 0
                || result_count != base_count
                || removal_index >= base_count
                || insertion_index >= result_count
                || mutation_index != removal_index.min(insertion_index)
            {
                return Err(CoreError::Path);
            }
        }
        TreeMutationKindV1::ReplacePair {
            first_index,
            second_index,
            first_expected,
            first_replacement,
            second_expected,
            second_replacement,
        } => {
            if result_count != base_count
                || first_index == second_index
                || first_index >= base_count
                || second_index >= base_count
                || mutation_index != first_index.min(second_index)
                || first_expected.name != first_replacement.name
                || second_expected.name != second_replacement.name
            {
                return Err(CoreError::Path);
            }
        }
    }
    let base_plan = TreePlanV1::for_count(base_count)?;
    let result_plan = TreePlanV1::for_count(result_count)?;
    let required_page_summaries = TREE_INDEX_FANOUT
        .checked_add(result_plan.level_one_count)
        .ok_or(CoreError::IntegerOverflow)?;
    if page_scratch.len() < required_page_summaries {
        return Err(CoreError::ResourceRefused);
    }
    let evidence_bytes = mutation_evidence_resident_bytes_v1(evidence)?;
    let page_bytes = core::mem::size_of_val(page_scratch);
    let port_bytes = source
        .resident_memory_bound_bytes()?
        .checked_add(sink.resident_memory_bound_bytes()?)
        .ok_or(CoreError::IntegerOverflow)?;
    let memory = OperationMemoryPlanV1::empty()
        .charge(
            MemoryComponentV1::ObjectScratch,
            object_scratch.len() as u64,
        )?
        .charge(
            MemoryComponentV1::ComparisonWindow,
            logical_scratch.len() as u64,
        )?
        .charge(
            MemoryComponentV1::PageSummaries,
            u64::try_from(page_bytes).map_err(|_| CoreError::IntegerOverflow)?,
        )?
        .charge(
            MemoryComponentV1::EvidenceWindow,
            u64::try_from(evidence_bytes).map_err(|_| CoreError::IntegerOverflow)?,
        )?
        .charge(
            MemoryComponentV1::HashState,
            mutation_hash_state_bytes_v1()?,
        )?
        .charge(MemoryComponentV1::MetadataWindow, port_bytes)?;
    match admission {
        #[cfg(test)]
        TreeMemoryAdmissionV1::Independent(ledger) => {
            let _reservation = ledger.reserve_operation_with_plan(memory)?;
            counters.memory_high_water = counters.memory_high_water.max(ledger.high_water_bytes());
            mutate_directory_entries_after_admission_v1(
                base,
                evidence,
                mutation_index,
                mutation,
                source,
                sink,
                counters,
                object_scratch,
                logical_scratch,
                page_scratch,
                base_count,
                result_count,
                base_plan,
                result_plan,
            )
        }
        TreeMemoryAdmissionV1::Borrowed(reservation) => {
            reservation.require(memory)?;
            mutate_directory_entries_after_admission_v1(
                base,
                evidence,
                mutation_index,
                mutation,
                source,
                sink,
                counters,
                object_scratch,
                logical_scratch,
                page_scratch,
                base_count,
                result_count,
                base_plan,
                result_plan,
            )
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn mutate_directory_entries_after_admission_v1<T, S>(
    base: CanonicalDirectoryTreeV1,
    evidence: AuthenticatedTreeMutationEvidenceV1<'_>,
    mutation_index: usize,
    mutation: TreeMutationKindV1<'_>,
    source: &mut T,
    sink: &mut S,
    counters: &mut OperationCountersV1,
    object_scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
    logical_scratch: &mut [u8; COMPARISON_WINDOW_BYTES],
    page_scratch: &mut [Option<TreePageSummaryV1>],
    base_count: usize,
    result_count: usize,
    base_plan: TreePlanV1,
    result_plan: TreePlanV1,
) -> CoreResult<CowTreeMutationV1>
where
    T: CanonicalTreeMutationSourceV1 + ?Sized,
    S: PreparedTreeSinkV1 + ?Sized,
{
    validate_mutation_relation(source, mutation_index, mutation, base_count, result_count)?;
    let logical = authenticate_and_derive_mutation_logical(
        base,
        evidence,
        mutation_index,
        mutation,
        source,
        base_count,
        result_count,
        logical_scratch,
        counters,
    )?;
    let verified_base_root = validate_mutation_physical_evidence(
        base,
        base_plan,
        evidence,
        mutation_index,
        source,
        object_scratch,
    )?;
    let first_changed_leaf = mutation_index / TREE_LEAF_FANOUT;
    let reused_leaves = first_changed_leaf
        .min(result_plan.leaf_count)
        .min(base_plan.leaf_count);
    let first_changed_level_one = first_changed_leaf / TREE_INDEX_FANOUT;
    let reused_level_one = first_changed_level_one
        .min(result_plan.level_one_count)
        .min(base_plan.level_one_count);
    let emitted = result_plan
        .leaf_count
        .checked_sub(reused_leaves)
        .and_then(|count| {
            result_plan
                .level_one_count
                .checked_sub(reused_level_one)
                .and_then(|level_one| count.checked_add(level_one))
        })
        .and_then(|count| count.checked_add(usize::from(result_plan.page_depth == 2)))
        .and_then(|count| count.checked_add(1))
        .ok_or(CoreError::IntegerOverflow)?;
    let emitted = u32::try_from(emitted).map_err(|_| CoreError::IntegerOverflow)?;

    sink.begin_private_tree_set(emitted).map_err(map_sink)?;
    let result = mutate_directory_entries_inner(
        base,
        evidence,
        source,
        result_count,
        result_plan,
        first_changed_leaf,
        reused_leaves,
        reused_level_one,
        emitted,
        sink,
        counters,
        object_scratch,
        page_scratch,
        logical,
        verified_base_root,
    );
    if result.is_err() {
        sink.abort_private_tree_set();
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn mutate_directory_entries_inner<T, S>(
    base: CanonicalDirectoryTreeV1,
    evidence: AuthenticatedTreeMutationEvidenceV1<'_>,
    source: &mut T,
    entry_count: usize,
    plan: TreePlanV1,
    first_changed_leaf: usize,
    reused_leaves: usize,
    reused_level_one: usize,
    emitted: u32,
    sink: &mut S,
    counters: &mut OperationCountersV1,
    object_scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
    page_scratch: &mut [Option<TreePageSummaryV1>],
    logical: DirectoryLogicalIdentityV1,
    verified_base_root: Option<TreePageSummaryV1>,
) -> CoreResult<CowTreeMutationV1>
where
    T: CanonicalTreeMutationSourceV1 + ?Sized,
    S: PreparedTreeSinkV1 + ?Sized,
{
    let required = TREE_INDEX_FANOUT
        .checked_add(plan.level_one_count)
        .ok_or(CoreError::IntegerOverflow)?;
    page_scratch[..required].fill(None);
    let level_one_start = TREE_INDEX_FANOUT;

    for group in 0..plan.level_one_count {
        let level_one_target = level_one_start + group;
        if group < reused_level_one {
            let summary = reused_level_one_summary(base, evidence, group, verified_base_root)?;
            page_scratch[level_one_target] = Some(summary);
            account_reused_page(summary, counters)?;
            continue;
        }
        let first_leaf = group
            .checked_mul(TREE_INDEX_FANOUT)
            .ok_or(CoreError::IntegerOverflow)?;
        let end_leaf = first_leaf
            .checked_add(TREE_INDEX_FANOUT)
            .ok_or(CoreError::IntegerOverflow)?
            .min(plan.leaf_count);
        page_scratch[..end_leaf - first_leaf].fill(None);
        for leaf in first_leaf..end_leaf {
            let target = leaf - first_leaf;
            let summary = if leaf < reused_leaves {
                reused_leaf_summary(base, evidence, leaf, verified_base_root)?
            } else {
                let first = leaf
                    .checked_mul(TREE_LEAF_FANOUT)
                    .ok_or(CoreError::IntegerOverflow)?;
                let end = first
                    .checked_add(TREE_LEAF_FANOUT)
                    .ok_or(CoreError::IntegerOverflow)?
                    .min(entry_count);
                encode_leaf_from_mutation_source(
                    source,
                    first,
                    end,
                    sink,
                    counters,
                    object_scratch,
                )?
            };
            if leaf < reused_leaves {
                account_reused_page(summary, counters)?;
            }
            page_scratch[target] = Some(summary);
        }
        let summary = encode_index_from_mutation_source(
            1,
            page_scratch[..end_leaf - first_leaf]
                .iter()
                .map(|summary| summary.ok_or(CoreError::MissingClosureEdge)),
            source,
            sink,
            counters,
            object_scratch,
        )?;
        page_scratch[level_one_target] = Some(summary);
    }

    let root_page = match plan.page_depth {
        0 if entry_count == 0 => None,
        0 if reused_leaves == 1 => {
            let summary = reused_leaf_summary(base, evidence, 0, verified_base_root)?;
            account_reused_page(summary, counters)?;
            page_scratch[0] = Some(summary);
            Some(summary)
        }
        0 => Some(encode_leaf_from_mutation_source(
            source,
            0,
            entry_count,
            sink,
            counters,
            object_scratch,
        )?),
        1 => page_scratch[level_one_start],
        2 => Some(encode_index_from_mutation_source(
            2,
            page_scratch[level_one_start..level_one_start + plan.level_one_count]
                .iter()
                .map(|entry| entry.ok_or(CoreError::MissingClosureEdge)),
            source,
            sink,
            counters,
            object_scratch,
        )?),
        _ => return Err(CoreError::CountCap),
    };
    let physical = encode_directory(
        base.mode.wire_mode()?,
        u32::try_from(entry_count).map_err(|_| CoreError::IntegerOverflow)?,
        plan.page_depth,
        root_page.map(TreePageSummaryV1::id),
        sink,
        counters,
        object_scratch,
    )?;
    sink.finish_private_tree_set(physical).map_err(map_sink)?;
    Ok(CowTreeMutationV1 {
        directory: CanonicalDirectoryTreeV1 {
            logical,
            physical,
            mode: base.mode,
            entry_count: u32::try_from(entry_count).map_err(|_| CoreError::IntegerOverflow)?,
            page_depth: plan.page_depth,
            tree_object_count: plan.tree_object_count,
            leaf_count: u32::try_from(plan.leaf_count).map_err(|_| CoreError::IntegerOverflow)?,
            level_one_count: u32::try_from(plan.level_one_count)
                .map_err(|_| CoreError::IntegerOverflow)?,
        },
        first_changed_leaf: u32::try_from(first_changed_leaf)
            .map_err(|_| CoreError::IntegerOverflow)?,
        structurally_reused_leaves: u32::try_from(reused_leaves)
            .map_err(|_| CoreError::IntegerOverflow)?,
        structurally_reused_level_one: u32::try_from(reused_level_one)
            .map_err(|_| CoreError::IntegerOverflow)?,
        emitted_objects: emitted,
    })
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct TreeEntrySnapshotV1 {
    name: [u8; 255],
    name_len: u8,
    child: CanonicalTreeChildV1,
}

impl TreeEntrySnapshotV1 {
    fn from_entry(entry: CanonicalTreeEntryV1<'_>) -> CoreResult<Self> {
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

    fn name(&self) -> &[u8] {
        &self.name[..usize::from(self.name_len)]
    }

    fn matches(self, entry: CanonicalTreeEntryV1<'_>) -> bool {
        self.name() == entry.name.as_bytes() && self.child == entry.child
    }
}

fn read_base_snapshot<T: CanonicalTreeMutationSourceV1 + ?Sized>(
    source: &mut T,
    ordinal: usize,
) -> CoreResult<TreeEntrySnapshotV1> {
    let ordinal = u32::try_from(ordinal).map_err(|_| CoreError::IntegerOverflow)?;
    let entry = source
        .read_base_entry(ordinal)
        .map_err(map_tree_mutation_source)?;
    TreeEntrySnapshotV1::from_entry(entry)
}

fn read_result_snapshot<T: CanonicalTreeMutationSourceV1 + ?Sized>(
    source: &mut T,
    ordinal: usize,
) -> CoreResult<TreeEntrySnapshotV1> {
    let ordinal = u32::try_from(ordinal).map_err(|_| CoreError::IntegerOverflow)?;
    let entry = source
        .read_result_entry(ordinal)
        .map_err(map_tree_mutation_source)?;
    TreeEntrySnapshotV1::from_entry(entry)
}

fn validate_mutation_relation<T: CanonicalTreeMutationSourceV1 + ?Sized>(
    source: &mut T,
    mutation_index: usize,
    mutation: TreeMutationKindV1<'_>,
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
            TreeMutationKindV1::Add(added) if result_ordinal == mutation_index => {
                if !result.matches(added) {
                    return Err(CoreError::Path);
                }
                None
            }
            TreeMutationKindV1::Add(_) => Some(if result_ordinal < mutation_index {
                result_ordinal
            } else {
                result_ordinal
                    .checked_sub(1)
                    .ok_or(CoreError::IntegerOverflow)?
            }),
            TreeMutationKindV1::Remove(_) => Some(if result_ordinal < mutation_index {
                result_ordinal
            } else {
                result_ordinal
                    .checked_add(1)
                    .ok_or(CoreError::IntegerOverflow)?
            }),
            TreeMutationKindV1::Move {
                insertion_index,
                moved,
                ..
            } if result_ordinal == insertion_index => {
                if !result.matches(moved) {
                    return Err(CoreError::Path);
                }
                None
            }
            TreeMutationKindV1::Move {
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
            TreeMutationKindV1::ReplacePair {
                first_index,
                first_replacement,
                ..
            } if result_ordinal == first_index => {
                if !result.matches(first_replacement) {
                    return Err(CoreError::Path);
                }
                None
            }
            TreeMutationKindV1::ReplacePair {
                second_index,
                second_replacement,
                ..
            } if result_ordinal == second_index => {
                if !result.matches(second_replacement) {
                    return Err(CoreError::Path);
                }
                None
            }
            TreeMutationKindV1::ReplacePair { .. } => Some(result_ordinal),
        };
        if let Some(base_ordinal) = expected {
            if read_base_snapshot(source, base_ordinal)? != result {
                return Err(CoreError::Path);
            }
        }
        previous_result = Some(result);
    }
    match mutation {
        TreeMutationKindV1::Remove(expected) => {
            if !read_base_snapshot(source, mutation_index)?.matches(expected) {
                return Err(CoreError::Path);
            }
        }
        TreeMutationKindV1::Move {
            removal_index,
            expected_removed,
            ..
        } => {
            if !read_base_snapshot(source, removal_index)?.matches(expected_removed) {
                return Err(CoreError::Path);
            }
        }
        TreeMutationKindV1::ReplacePair {
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
        TreeMutationKindV1::Add(_) => {}
    }
    Ok(())
}

fn mutation_entry_encoded_len(entry: CanonicalTreeEntryV1<'_>) -> CoreResult<u64> {
    u64::try_from(entry.name.as_bytes().len())
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
fn authenticate_and_derive_mutation_logical<T: CanonicalTreeMutationSourceV1 + ?Sized>(
    base: CanonicalDirectoryTreeV1,
    evidence: AuthenticatedTreeMutationEvidenceV1<'_>,
    mutation_index: usize,
    mutation: TreeMutationKindV1<'_>,
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
        TreeMutationKindV1::Add(entry) | TreeMutationKindV1::Remove(entry) => {
            mutation_entry_encoded_len(entry)?
        }
        TreeMutationKindV1::Move { .. } | TreeMutationKindV1::ReplacePair { .. } => 0,
    };
    let result_preimage_len = match mutation {
        TreeMutationKindV1::Add(_) => proof
            .old_preimage_len
            .checked_add(changed_len)
            .ok_or(CoreError::IntegerOverflow)?,
        TreeMutationKindV1::Remove(_) => proof
            .old_preimage_len
            .checked_sub(changed_len)
            .ok_or(CoreError::IdMismatch)?,
        TreeMutationKindV1::Move {
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
        TreeMutationKindV1::ReplacePair { .. } => proof.old_preimage_len,
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

#[allow(clippy::too_many_arguments)]
fn validate_mutation_physical_evidence<T: CanonicalTreeMutationSourceV1 + ?Sized>(
    base: CanonicalDirectoryTreeV1,
    plan: TreePlanV1,
    evidence: AuthenticatedTreeMutationEvidenceV1<'_>,
    mutation_index: usize,
    source: &mut T,
    scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
) -> CoreResult<Option<TreePageSummaryV1>> {
    if evidence.base_logical != base.logical
        || evidence.base_physical != base.physical
        || base.entry_count as usize != plan.entry_count
        || base.page_depth != plan.page_depth
        || base.leaf_count as usize != plan.leaf_count
        || base.level_one_count as usize != plan.level_one_count
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
            base.mode.wire_mode()?,
            0,
            0,
            None,
            &mut verifier,
            &mut ignored,
            scratch,
        )?;
        if physical != base.physical {
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
    let verified_leaf =
        encode_leaf_from_base_source(source, first, end, &mut verifier, &mut ignored, scratch)?;
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
            let first_entry = read_base_snapshot(source, first)?;
            let last_entry = read_base_snapshot(source, end - 1)?;
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
        base.mode.wire_mode()?,
        base.entry_count,
        base.page_depth,
        Some(root_page.id),
        &mut verifier,
        &mut ignored,
        scratch,
    )?;
    if physical != base.physical {
        return Err(CoreError::IdMismatch);
    }
    Ok(Some(root_page))
}

fn reused_level_one_summary(
    base: CanonicalDirectoryTreeV1,
    evidence: AuthenticatedTreeMutationEvidenceV1<'_>,
    group: usize,
    verified_base_root: Option<TreePageSummaryV1>,
) -> CoreResult<TreePageSummaryV1> {
    match base.page_depth {
        1 if group == 0 => verified_base_root.ok_or(CoreError::MissingClosureEdge),
        2 => evidence
            .level_one_group
            .get(group)
            .map(|boundary| boundary.summary)
            .ok_or(CoreError::MissingClosureEdge),
        _ => Err(CoreError::MissingClosureEdge),
    }
}

fn reused_leaf_summary(
    base: CanonicalDirectoryTreeV1,
    evidence: AuthenticatedTreeMutationEvidenceV1<'_>,
    leaf: usize,
    verified_base_root: Option<TreePageSummaryV1>,
) -> CoreResult<TreePageSummaryV1> {
    if base.page_depth == 0 && leaf == 0 {
        return verified_base_root.ok_or(CoreError::MissingClosureEdge);
    }
    let affected = evidence
        .affected_leaf_index
        .ok_or(CoreError::MissingClosureEdge)? as usize;
    let first_leaf = affected / TREE_INDEX_FANOUT * TREE_INDEX_FANOUT;
    evidence
        .leaf_group
        .get(
            leaf.checked_sub(first_leaf)
                .ok_or(CoreError::MissingClosureEdge)?,
        )
        .map(|boundary| boundary.summary)
        .ok_or(CoreError::MissingClosureEdge)
}

fn encode_leaf_from_base_source<T, S>(
    source: &mut T,
    first_entry: usize,
    end_entry: usize,
    sink: &mut S,
    counters: &mut OperationCountersV1,
    scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
) -> CoreResult<TreePageSummaryV1>
where
    T: CanonicalTreeMutationSourceV1 + ?Sized,
    S: PreparedTreeSinkV1 + ?Sized,
{
    encode_leaf_from_source(
        source,
        true,
        first_entry,
        end_entry,
        sink,
        counters,
        scratch,
    )
}

fn encode_leaf_from_mutation_source<T, S>(
    source: &mut T,
    first_entry: usize,
    end_entry: usize,
    sink: &mut S,
    counters: &mut OperationCountersV1,
    scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
) -> CoreResult<TreePageSummaryV1>
where
    T: CanonicalTreeMutationSourceV1 + ?Sized,
    S: PreparedTreeSinkV1 + ?Sized,
{
    encode_leaf_from_source(
        source,
        false,
        first_entry,
        end_entry,
        sink,
        counters,
        scratch,
    )
}

#[allow(clippy::too_many_arguments)]
fn encode_leaf_from_source<T, S>(
    source: &mut T,
    base_side: bool,
    first_entry: usize,
    end_entry: usize,
    sink: &mut S,
    counters: &mut OperationCountersV1,
    scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
) -> CoreResult<TreePageSummaryV1>
where
    T: CanonicalTreeMutationSourceV1 + ?Sized,
    S: PreparedTreeSinkV1 + ?Sized,
{
    let count = end_entry
        .checked_sub(first_entry)
        .ok_or(CoreError::IntegerOverflow)?;
    if count == 0 || count > TREE_LEAF_FANOUT {
        return Err(CoreError::CountCap);
    }
    let mut writer = ObjectWriterV1::new(scratch);
    writer.write(&[0x02, 0x00])?;
    writer.write(
        &u16::try_from(count)
            .map_err(|_| CoreError::IntegerOverflow)?
            .to_be_bytes(),
    )?;
    let mut previous: Option<TreeEntrySnapshotV1> = None;
    for ordinal in first_entry..end_entry {
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
        writer.write(&u16::from(entry.name_len).to_be_bytes())?;
        writer.write(entry.name())?;
        writer.write(&[entry.child.physical_kind()])?;
        writer.write(entry.child.physical_id_ref())?;
        previous = Some(entry);
    }
    let (id, object_len) = finish_tree_object(writer, sink, counters)?;
    Ok(TreePageSummaryV1 {
        id,
        depth: 0,
        first_entry: u32::try_from(first_entry).map_err(|_| CoreError::IntegerOverflow)?,
        last_entry: u32::try_from(end_entry - 1).map_err(|_| CoreError::IntegerOverflow)?,
        subtree_entry_count: u32::try_from(count).map_err(|_| CoreError::IntegerOverflow)?,
        object_len,
    })
}

fn encode_index_from_mutation_source<T, I, S>(
    depth: u8,
    children: I,
    source: &mut T,
    sink: &mut S,
    counters: &mut OperationCountersV1,
    scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
) -> CoreResult<TreePageSummaryV1>
where
    T: CanonicalTreeMutationSourceV1 + ?Sized,
    I: IntoIterator<Item = CoreResult<TreePageSummaryV1>>,
    S: PreparedTreeSinkV1 + ?Sized,
{
    if !(1..=2).contains(&depth) {
        return Err(CoreError::CountCap);
    }
    let mut writer = ObjectWriterV1::new(scratch);
    writer.write(&[0x03, depth])?;
    let count_offset = writer.position();
    writer.write(&[0, 0])?;
    let mut count = 0_u16;
    let mut total = 0_u32;
    let mut first_entry = None;
    let mut last_entry = None;
    for child in children {
        let child = child?;
        if child.depth.checked_add(1) != Some(depth) {
            return Err(CoreError::TypedEdge);
        }
        count = count.checked_add(1).ok_or(CoreError::IntegerOverflow)?;
        if usize::from(count) > TREE_INDEX_FANOUT {
            return Err(CoreError::CountCap);
        }
        total = total
            .checked_add(child.subtree_entry_count)
            .ok_or(CoreError::IntegerOverflow)?;
        let first = read_result_snapshot(source, child.first_entry as usize)?;
        let last = read_result_snapshot(source, child.last_entry as usize)?;
        writer.write(&child.subtree_entry_count.to_be_bytes())?;
        writer.write(&u16::from(first.name_len).to_be_bytes())?;
        writer.write(first.name())?;
        writer.write(&u16::from(last.name_len).to_be_bytes())?;
        writer.write(last.name())?;
        writer.write(child.id.as_bytes())?;
        first_entry.get_or_insert(child.first_entry);
        last_entry = Some(child.last_entry);
    }
    if count == 0 {
        return Err(CoreError::CountCap);
    }
    writer.overwrite(count_offset, &count.to_be_bytes())?;
    let (id, object_len) = finish_tree_object(writer, sink, counters)?;
    Ok(TreePageSummaryV1 {
        id,
        depth,
        first_entry: first_entry.ok_or(CoreError::Truncated)?,
        last_entry: last_entry.ok_or(CoreError::Truncated)?,
        subtree_entry_count: total,
        object_len,
    })
}

fn map_tree_mutation_source(
    TreeMutationSourceErrorV1::Failure: TreeMutationSourceErrorV1,
) -> CoreError {
    CoreError::SourceFailure
}

fn account_reused_page(
    summary: TreePageSummaryV1,
    counters: &mut OperationCountersV1,
) -> CoreResult<()> {
    counters.add(CounterFieldV1::TreeNodesReused, 1)?;
    counters.add(CounterFieldV1::PhysicalObjectsReused, 1)?;
    counters.add(
        CounterFieldV1::BytesStructurallyReused,
        u64::from(summary.object_len),
    )
}

fn validate_entries(entries: &[CanonicalTreeEntryV1<'_>]) -> CoreResult<()> {
    validate_entry_count(u64::try_from(entries.len()).map_err(|_| CoreError::IntegerOverflow)?)?;
    let mut previous: Option<&[u8]> = None;
    for entry in entries {
        let name = entry.name.as_bytes();
        if previous.is_some_and(|left| compare_unsigned(left, name) != core::cmp::Ordering::Less) {
            return Err(CoreError::NonCanonicalOrder);
        }
        previous = Some(name);
    }
    Ok(())
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
            .checked_add(entry.name.as_bytes().len())
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
fn validate_replacement_evidence(
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
    if evidence.base_logical != base.logical
        || evidence.base_physical != base.physical
        || base.entry_count as usize != plan.entry_count
        || base.page_depth != plan.page_depth
        || base.leaf_count as usize != plan.leaf_count
        || base.level_one_count as usize != plan.level_one_count
        || evidence.affected_leaf_index as usize != leaf_index
        || evidence.affected_entries.len() != leaf_end - leaf_first
        || evidence.affected_leaf.depth != 0
        || evidence.affected_leaf.first_entry as usize != leaf_first
        || evidence.affected_leaf.last_entry as usize != leaf_end - 1
        || evidence.affected_leaf.subtree_entry_count as usize != leaf_end - leaf_first
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
                    != evidence.affected_entries[0].name.as_bytes()
                || affected_boundary.last_name.as_bytes()
                    != evidence.affected_entries[evidence.affected_entries.len() - 1]
                        .name
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
        base.mode.wire_mode()?,
        base.entry_count,
        base.page_depth,
        Some(root_page.id),
        &mut verifier,
        &mut ignored,
        object_scratch,
    )?;
    if physical != base.physical {
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

fn validate_page_boundaries(
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

fn derive_replacement_logical(
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

fn directory_logical_bytes(logical: DirectoryLogicalIdentityV1) -> [u8; 32] {
    match logical {
        DirectoryLogicalIdentityV1::ImplicitRoot(id) => *id.id().as_bytes(),
        DirectoryLogicalIdentityV1::Explicit(id) => *id.id().as_bytes(),
    }
}

fn directory_logical_from_digest(
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

const HASH_PROOF_ACCUMULATOR_NODES: usize =
    MAX_DIRECTORY_HASH_PROOF_NODES + COMPARISON_WINDOW_BYTES / blake3::CHUNK_LEN + 2;

#[derive(Clone, Copy)]
struct HashProofNodeV1 {
    offset: u64,
    byte_len: u64,
    chunks: u64,
    chaining_value: [u8; 32],
}

const EMPTY_HASH_PROOF_NODE: HashProofNodeV1 = HashProofNodeV1 {
    offset: 0,
    byte_len: 0,
    chunks: 0,
    chaining_value: [0; 32],
};

struct HashProofAccumulatorV1 {
    nodes: [HashProofNodeV1; HASH_PROOF_ACCUMULATOR_NODES],
    len: usize,
    total_len: u64,
}

impl HashProofAccumulatorV1 {
    fn new(total_len: u64) -> Self {
        Self {
            nodes: [EMPTY_HASH_PROOF_NODE; HASH_PROOF_ACCUMULATOR_NODES],
            len: 0,
            total_len,
        }
    }

    fn push(&mut self, mut node: HashProofNodeV1) -> CoreResult<()> {
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

    fn finish(mut self) -> CoreResult<[u8; 32]> {
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

fn hash_directory_sparse_proof(
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

fn validate_hash_subtree(
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

fn derive_logical<'a, I>(
    mode: DirectoryBuildModeV1,
    count: usize,
    entries: I,
) -> CoreResult<DirectoryLogicalIdentityV1>
where
    I: IntoIterator<Item = CanonicalTreeEntryV1<'a>>,
{
    let count = u64::try_from(count).map_err(|_| CoreError::IntegerOverflow)?;
    match mode {
        DirectoryBuildModeV1::ImplicitRoot => derive_implicit_root_directory_iter_v1(
            count,
            entries.into_iter().map(CanonicalTreeEntryV1::logical_entry),
        )
        .map(DirectoryLogicalIdentityV1::ImplicitRoot),
        DirectoryBuildModeV1::Explicit(mode) => derive_explicit_directory_iter_v1(
            mode,
            count,
            entries.into_iter().map(CanonicalTreeEntryV1::logical_entry),
        )
        .map(DirectoryLogicalIdentityV1::Explicit),
    }
}

fn encode_leaf<'a, I, S>(
    entries: I,
    first_entry: usize,
    end_entry: usize,
    sink: &mut S,
    counters: &mut OperationCountersV1,
    scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
) -> CoreResult<TreePageSummaryV1>
where
    I: IntoIterator<Item = CanonicalTreeEntryV1<'a>>,
    S: PreparedTreeSinkV1 + ?Sized,
{
    let count = end_entry
        .checked_sub(first_entry)
        .ok_or(CoreError::IntegerOverflow)?;
    if count == 0 || count > TREE_LEAF_FANOUT {
        return Err(CoreError::CountCap);
    }
    let mut writer = ObjectWriterV1::new(scratch);
    writer.write(&[0x02, 0x00])?;
    writer.write(
        &u16::try_from(count)
            .map_err(|_| CoreError::IntegerOverflow)?
            .to_be_bytes(),
    )?;
    let mut actual = 0_usize;
    for entry in entries {
        actual = actual.checked_add(1).ok_or(CoreError::IntegerOverflow)?;
        let name = entry.name.as_bytes();
        writer.write(
            &u16::try_from(name.len())
                .map_err(|_| CoreError::IntegerOverflow)?
                .to_be_bytes(),
        )?;
        writer.write(name)?;
        writer.write(&[entry.child.physical_kind()])?;
        writer.write(entry.child.physical_id_ref())?;
    }
    if actual != count {
        return Err(CoreError::Truncated);
    }
    let (id, object_len) = finish_tree_object(writer, sink, counters)?;
    Ok(TreePageSummaryV1 {
        id,
        depth: 0,
        first_entry: u32::try_from(first_entry).map_err(|_| CoreError::IntegerOverflow)?,
        last_entry: u32::try_from(end_entry - 1).map_err(|_| CoreError::IntegerOverflow)?,
        subtree_entry_count: u32::try_from(count).map_err(|_| CoreError::IntegerOverflow)?,
        object_len,
    })
}

fn encode_index<I, S>(
    depth: u8,
    children: I,
    entries: &[CanonicalTreeEntryV1<'_>],
    sink: &mut S,
    counters: &mut OperationCountersV1,
    scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
) -> CoreResult<TreePageSummaryV1>
where
    I: IntoIterator<Item = CoreResult<TreePageSummaryV1>>,
    S: PreparedTreeSinkV1 + ?Sized,
{
    if !(1..=2).contains(&depth) {
        return Err(CoreError::CountCap);
    }
    let mut writer = ObjectWriterV1::new(scratch);
    writer.write(&[0x03, depth])?;
    let count_offset = writer.position();
    writer.write(&[0, 0])?;
    let mut count = 0_u16;
    let mut total = 0_u32;
    let mut first_entry = None;
    let mut last_entry = None;
    for child in children {
        let child = child?;
        if child.depth.checked_add(1) != Some(depth) {
            return Err(CoreError::TypedEdge);
        }
        count = count.checked_add(1).ok_or(CoreError::IntegerOverflow)?;
        if usize::from(count) > TREE_INDEX_FANOUT {
            return Err(CoreError::CountCap);
        }
        total = total
            .checked_add(child.subtree_entry_count)
            .ok_or(CoreError::IntegerOverflow)?;
        let first_name = entries
            .get(child.first_entry as usize)
            .ok_or(CoreError::MissingClosureEdge)?
            .name
            .as_bytes();
        let last_name = entries
            .get(child.last_entry as usize)
            .ok_or(CoreError::MissingClosureEdge)?
            .name
            .as_bytes();
        writer.write(&child.subtree_entry_count.to_be_bytes())?;
        writer.write(
            &u16::try_from(first_name.len())
                .map_err(|_| CoreError::IntegerOverflow)?
                .to_be_bytes(),
        )?;
        writer.write(first_name)?;
        writer.write(
            &u16::try_from(last_name.len())
                .map_err(|_| CoreError::IntegerOverflow)?
                .to_be_bytes(),
        )?;
        writer.write(last_name)?;
        writer.write(child.id.as_bytes())?;
        first_entry.get_or_insert(child.first_entry);
        last_entry = Some(child.last_entry);
    }
    if count == 0 {
        return Err(CoreError::CountCap);
    }
    writer.overwrite(count_offset, &count.to_be_bytes())?;
    let (id, object_len) = finish_tree_object(writer, sink, counters)?;
    Ok(TreePageSummaryV1 {
        id,
        depth,
        first_entry: first_entry.ok_or(CoreError::Truncated)?,
        last_entry: last_entry.ok_or(CoreError::Truncated)?,
        subtree_entry_count: total,
        object_len,
    })
}

fn encode_index_boundaries<'a, I, S>(
    depth: u8,
    children: I,
    sink: &mut S,
    counters: &mut OperationCountersV1,
    scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
) -> CoreResult<TreePageSummaryV1>
where
    I: IntoIterator<Item = CoreResult<TreePageBoundaryV1<'a>>>,
    S: PreparedTreeSinkV1 + ?Sized,
{
    if !(1..=2).contains(&depth) {
        return Err(CoreError::CountCap);
    }
    let mut writer = ObjectWriterV1::new(scratch);
    writer.write(&[0x03, depth])?;
    let count_offset = writer.position();
    writer.write(&[0, 0])?;
    let mut count = 0_u16;
    let mut total = 0_u32;
    let mut first_entry = None;
    let mut last_entry = None;
    for boundary in children {
        let boundary = boundary?;
        let child = boundary.summary;
        if child.depth.checked_add(1) != Some(depth) {
            return Err(CoreError::TypedEdge);
        }
        count = count.checked_add(1).ok_or(CoreError::IntegerOverflow)?;
        if usize::from(count) > TREE_INDEX_FANOUT {
            return Err(CoreError::CountCap);
        }
        total = total
            .checked_add(child.subtree_entry_count)
            .ok_or(CoreError::IntegerOverflow)?;
        let first_name = boundary.first_name.as_bytes();
        let last_name = boundary.last_name.as_bytes();
        writer.write(&child.subtree_entry_count.to_be_bytes())?;
        writer.write(
            &u16::try_from(first_name.len())
                .map_err(|_| CoreError::IntegerOverflow)?
                .to_be_bytes(),
        )?;
        writer.write(first_name)?;
        writer.write(
            &u16::try_from(last_name.len())
                .map_err(|_| CoreError::IntegerOverflow)?
                .to_be_bytes(),
        )?;
        writer.write(last_name)?;
        writer.write(child.id.as_bytes())?;
        first_entry.get_or_insert(child.first_entry);
        last_entry = Some(child.last_entry);
    }
    if count == 0 {
        return Err(CoreError::CountCap);
    }
    writer.overwrite(count_offset, &count.to_be_bytes())?;
    let (id, object_len) = finish_tree_object(writer, sink, counters)?;
    Ok(TreePageSummaryV1 {
        id,
        depth,
        first_entry: first_entry.ok_or(CoreError::Truncated)?,
        last_entry: last_entry.ok_or(CoreError::Truncated)?,
        subtree_entry_count: total,
        object_len,
    })
}

#[allow(clippy::too_many_arguments)]
fn encode_directory<S: PreparedTreeSinkV1 + ?Sized>(
    mode: u16,
    entry_count: u32,
    page_depth: u8,
    root_page: Option<PhysicalTreeIdV1>,
    sink: &mut S,
    counters: &mut OperationCountersV1,
    scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
) -> CoreResult<PhysicalTreeIdV1> {
    if (entry_count == 0) != root_page.is_none() {
        return Err(CoreError::TypedEdge);
    }
    let mut writer = ObjectWriterV1::new(scratch);
    writer.write(&[0x01])?;
    writer.write(&mode.to_be_bytes())?;
    writer.write(&entry_count.to_be_bytes())?;
    writer.write(&[page_depth, u8::from(root_page.is_some())])?;
    if let Some(id) = root_page {
        writer.write(id.as_bytes())?;
    }
    finish_tree_object(writer, sink, counters).map(|(id, _)| id)
}

fn finish_tree_object<S: PreparedTreeSinkV1 + ?Sized>(
    mut writer: ObjectWriterV1<'_>,
    sink: &mut S,
    counters: &mut OperationCountersV1,
) -> CoreResult<(PhysicalTreeIdV1, u32)> {
    writer.finish_header()?;
    let bytes = writer.complete_bytes();
    let decoded = decode_physical_object_v1(bytes, &mut DiscardStrongEdgesV1)?;
    if decoded.header().kind() != PhysicalObjectKindV1::Tree {
        return Err(CoreError::TypeDomain);
    }
    let id = derive_physical_tree_id_v1(bytes)?;
    if decoded.physical_id()? != TypedPhysicalObjectIdV1::Tree(id) {
        return Err(CoreError::IdMismatch);
    }
    let object_len = u32::try_from(bytes.len()).map_err(|_| CoreError::IntegerOverflow)?;
    match sink.admit_private_tree(id, bytes).map_err(map_sink)? {
        TreeObjectDispositionV1::Created => {
            counters.add(CounterFieldV1::TreeNodesCreated, 1)?;
            counters.add(CounterFieldV1::PhysicalObjectsCreated, 1)?;
            counters.add(CounterFieldV1::BytesWritten, u64::from(object_len))?;
        }
        TreeObjectDispositionV1::Reused => {
            counters.add(CounterFieldV1::TreeNodesReused, 1)?;
            counters.add(CounterFieldV1::PhysicalObjectsReused, 1)?;
            counters.add(
                CounterFieldV1::BytesStructurallyReused,
                u64::from(object_len),
            )?;
        }
    }
    Ok((id, object_len))
}

fn account_replacement_reuse(
    evidence: AuthenticatedTreeReplacementEvidenceV1<'_>,
    changed_leaf: usize,
    depth: u8,
    counters: &mut OperationCountersV1,
) -> CoreResult<()> {
    let first_leaf = (changed_leaf / TREE_INDEX_FANOUT) * TREE_INDEX_FANOUT;
    for (relative, boundary) in evidence.leaf_group.iter().enumerate() {
        if first_leaf + relative != changed_leaf {
            account_reused_page(boundary.summary, counters)?;
        }
    }
    if depth == 2 {
        let changed_level_one = changed_leaf / TREE_INDEX_FANOUT;
        for (index, boundary) in evidence.level_one_group.iter().enumerate() {
            if index != changed_level_one {
                account_reused_page(boundary.summary, counters)?;
            }
        }
    }
    Ok(())
}

struct ObjectWriterV1<'a> {
    bytes: &'a mut [u8; MAX_TREE_OBJECT_BYTES],
    position: usize,
}

impl<'a> ObjectWriterV1<'a> {
    fn new(bytes: &'a mut [u8; MAX_TREE_OBJECT_BYTES]) -> Self {
        bytes[..52].fill(0);
        Self {
            bytes,
            position: 52,
        }
    }

    const fn position(&self) -> usize {
        self.position
    }

    fn write(&mut self, value: &[u8]) -> CoreResult<()> {
        let end = self
            .position
            .checked_add(value.len())
            .ok_or(CoreError::IntegerOverflow)?;
        let target = self
            .bytes
            .get_mut(self.position..end)
            .ok_or(CoreError::PhysicalObjectCap)?;
        target.copy_from_slice(value);
        self.position = end;
        Ok(())
    }

    fn overwrite(&mut self, offset: usize, value: &[u8]) -> CoreResult<()> {
        let end = offset
            .checked_add(value.len())
            .ok_or(CoreError::IntegerOverflow)?;
        let target = self
            .bytes
            .get_mut(offset..end)
            .ok_or(CoreError::PhysicalObjectCap)?;
        target.copy_from_slice(value);
        Ok(())
    }

    fn finish_header(&mut self) -> CoreResult<()> {
        let payload_len = self
            .position
            .checked_sub(52)
            .ok_or(CoreError::IntegerOverflow)?;
        let payload_len = u64::try_from(payload_len).map_err(|_| CoreError::IntegerOverflow)?;
        self.bytes[..52].copy_from_slice(&encode_physical_object_header_v1(
            PhysicalObjectKindV1::Tree,
            payload_len,
        ));
        Ok(())
    }

    fn complete_bytes(&self) -> &[u8] {
        &self.bytes[..self.position]
    }
}

#[derive(Clone, Copy)]
struct TreePlanV1 {
    entry_count: usize,
    page_depth: u8,
    leaf_count: usize,
    level_one_count: usize,
    summary_count: usize,
    tree_object_count: u32,
}

impl TreePlanV1 {
    fn for_count(entry_count: usize) -> CoreResult<Self> {
        if entry_count > MAX_ENTRIES as usize {
            return Err(CoreError::CountCap);
        }
        let leaf_count = entry_count.div_ceil(TREE_LEAF_FANOUT);
        let level_one_count = if leaf_count > 1 {
            leaf_count.div_ceil(TREE_INDEX_FANOUT)
        } else {
            0
        };
        let page_depth = match entry_count {
            0..=TREE_LEAF_FANOUT => 0,
            193..=18_432 => 1,
            _ => 2,
        };
        let root_level_two = usize::from(page_depth == 2);
        let summary_count = leaf_count
            .checked_add(level_one_count)
            .and_then(|count| count.checked_add(root_level_two))
            .ok_or(CoreError::IntegerOverflow)?;
        if summary_count > MAX_TREE_PAGE_SUMMARIES {
            return Err(CoreError::CountCap);
        }
        let tree_object_count = summary_count
            .checked_add(1)
            .ok_or(CoreError::IntegerOverflow)?;
        Ok(Self {
            entry_count,
            page_depth,
            leaf_count,
            level_one_count,
            summary_count,
            tree_object_count: u32::try_from(tree_object_count)
                .map_err(|_| CoreError::IntegerOverflow)?,
        })
    }

    fn shape(self) -> CoreResult<CanonicalTreeShapeV1> {
        Ok(CanonicalTreeShapeV1 {
            entry_count: u32::try_from(self.entry_count).map_err(|_| CoreError::IntegerOverflow)?,
            page_depth: self.page_depth,
            leaf_count: u32::try_from(self.leaf_count).map_err(|_| CoreError::IntegerOverflow)?,
            level_one_count: u32::try_from(self.level_one_count)
                .map_err(|_| CoreError::IntegerOverflow)?,
            page_summary_count: u32::try_from(self.summary_count)
                .map_err(|_| CoreError::IntegerOverflow)?,
            tree_object_count: self.tree_object_count,
        })
    }
}

const fn map_sink(_: TreeSinkErrorV1) -> CoreError {
    CoreError::SinkRefused
}
