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
    ExplicitDirectoryNodeV1, FileNodeIdV1, ImplicitRootDirectoryV1, LogicalChildIdV1,
    LogicalDirectoryEntryV1, PhysicalFileIdV1, PhysicalSymlinkIdV1, PhysicalTreeIdV1,
    SymlinkNodeIdV1, COMPARISON_WINDOW_BYTES, IDENTITY_HASHER_BYTES_V1,
};
use crate::limits::OperationReservationV1;
use crate::limits::ResourceLedgerV1;
use crate::limits::{
    CounterFieldV1, MemoryComponentV1, OperationCountersV1, OperationMemoryPlanV1,
};
use crate::object::{
    decode_physical_object_v1, seal_physical_object_in_place_v1, DiscardStrongEdgesV1,
    TypedPhysicalObjectIdV1,
};
use crate::{CoreError, CoreResult};
use blake3::hazmat::{merge_subtrees_non_root, merge_subtrees_root, HasherExt, Mode};

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
    pub(super) fn wire_mode(self) -> CoreResult<u16> {
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
    pub(super) const fn logical(self) -> LogicalChildIdV1 {
        match self {
            Self::Directory { logical, .. } => LogicalChildIdV1::Directory(logical),
            Self::File { logical, .. } => LogicalChildIdV1::File(logical),
            Self::Symlink { logical, .. } => LogicalChildIdV1::Symlink(logical),
        }
    }

    pub(super) const fn physical_kind(self) -> u8 {
        match self {
            Self::Directory { .. } => 0x01,
            Self::File { .. } => 0x02,
            Self::Symlink { .. } => 0x03,
        }
    }

    pub(super) const fn physical_id_ref(&self) -> &[u8; 32] {
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
    pub(super) name: ValidatedComponent<'a>,
    pub(super) child: CanonicalTreeChildV1,
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
    pub(super) id: PhysicalTreeIdV1,
    pub(super) depth: u8,
    pub(super) first_entry: u32,
    pub(super) last_entry: u32,
    pub(super) subtree_entry_count: u32,
    pub(super) object_len: u32,
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
    pub(super) logical: DirectoryLogicalIdentityV1,
    pub(super) physical: PhysicalTreeIdV1,
    pub(super) mode: DirectoryBuildModeV1,
    pub(super) entry_count: u32,
    pub(super) page_depth: u8,
    pub(super) tree_object_count: u32,
    pub(super) leaf_count: u32,
    pub(super) level_one_count: u32,
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

/// A page summary plus the exact boundary names committed by its parent.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TreePageBoundaryV1<'a> {
    pub(super) summary: TreePageSummaryV1,
    pub(super) first_name: ValidatedComponent<'a>,
    pub(super) last_name: ValidatedComponent<'a>,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CowTreeReplacementV1 {
    pub(super) directory: CanonicalDirectoryTreeV1,
    pub(super) changed_leaf_index: u32,
    pub(super) changed_leaf: TreePageSummaryV1,
    pub(super) changed_level_one: Option<(u32, TreePageSummaryV1)>,
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
    pub(super) directory: CanonicalDirectoryTreeV1,
    pub(super) first_changed_leaf: u32,
    pub(super) structurally_reused_leaves: u32,
    pub(super) structurally_reused_level_one: u32,
    pub(super) emitted_objects: u32,
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

pub(super) fn validate_entries(entries: &[CanonicalTreeEntryV1<'_>]) -> CoreResult<()> {
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

trait CanonicalTreeEntryVisitorV1 {
    fn visit<F>(&mut self, first_entry: usize, end_entry: usize, emit: &mut F) -> CoreResult<()>
    where
        F: for<'entry> FnMut(CanonicalTreeEntryV1<'entry>) -> CoreResult<()>;
}

struct IteratorTreeEntryVisitor<I> {
    entries: I,
}

impl<'a, I> CanonicalTreeEntryVisitorV1 for IteratorTreeEntryVisitor<I>
where
    I: Iterator<Item = CoreResult<CanonicalTreeEntryV1<'a>>>,
{
    fn visit<F>(&mut self, _first_entry: usize, _end_entry: usize, emit: &mut F) -> CoreResult<()>
    where
        F: for<'entry> FnMut(CanonicalTreeEntryV1<'entry>) -> CoreResult<()>,
    {
        for entry in self.entries.by_ref() {
            emit(entry?)?;
        }
        Ok(())
    }
}

fn encode_leaf_with_visitor<V, S>(
    visitor: &mut V,
    first_entry: usize,
    end_entry: usize,
    sink: &mut S,
    counters: &mut OperationCountersV1,
    scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
) -> CoreResult<TreePageSummaryV1>
where
    V: CanonicalTreeEntryVisitorV1 + ?Sized,
    S: PreparedTreeSinkV1 + ?Sized,
{
    let count = end_entry
        .checked_sub(first_entry)
        .ok_or(CoreError::IntegerOverflow)?;
    if count == 0 || count > TREE_LEAF_FANOUT {
        return Err(CoreError::CountCap);
    }
    let mut writer = TreePayloadWriterV1::new(scratch);
    writer.write(&[0x02, 0x00])?;
    writer.write(
        &u16::try_from(count)
            .map_err(|_| CoreError::IntegerOverflow)?
            .to_be_bytes(),
    )?;
    let mut actual = 0_usize;
    let mut emit = |entry: CanonicalTreeEntryV1<'_>| {
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
        Ok(())
    };
    visitor.visit(first_entry, end_entry, &mut emit)?;
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

pub(super) trait CanonicalTreeEntryReaderV1 {
    fn read_entry(&mut self, ordinal: usize) -> CoreResult<CanonicalTreeEntryV1<'_>>;
}

struct ReaderTreeEntryVisitor<'a, R: ?Sized> {
    reader: &'a mut R,
}

impl<'a, R> CanonicalTreeEntryVisitorV1 for ReaderTreeEntryVisitor<'a, R>
where
    R: CanonicalTreeEntryReaderV1 + ?Sized,
{
    fn visit<F>(&mut self, first_entry: usize, end_entry: usize, emit: &mut F) -> CoreResult<()>
    where
        F: for<'entry> FnMut(CanonicalTreeEntryV1<'entry>) -> CoreResult<()>,
    {
        for ordinal in first_entry..end_entry {
            emit(self.reader.read_entry(ordinal)?)?;
        }
        Ok(())
    }
}

pub(super) fn encode_leaf<'a, I, S>(
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
    let mut visitor = IteratorTreeEntryVisitor {
        entries: entries.into_iter().map(Ok),
    };
    encode_leaf_with_visitor(
        &mut visitor,
        first_entry,
        end_entry,
        sink,
        counters,
        scratch,
    )
}

pub(super) fn encode_leaf_from_reader<R, S>(
    reader: &mut R,
    first_entry: usize,
    end_entry: usize,
    sink: &mut S,
    counters: &mut OperationCountersV1,
    scratch: &mut [u8; MAX_TREE_OBJECT_BYTES],
) -> CoreResult<TreePageSummaryV1>
where
    R: CanonicalTreeEntryReaderV1 + ?Sized,
    S: PreparedTreeSinkV1 + ?Sized,
{
    let mut visitor = ReaderTreeEntryVisitor { reader };
    encode_leaf_with_visitor(
        &mut visitor,
        first_entry,
        end_entry,
        sink,
        counters,
        scratch,
    )
}
pub(super) fn encode_index<I, S>(
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
    let mut writer = TreePayloadWriterV1::new(scratch);
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

pub(super) fn encode_index_boundaries<'a, I, S>(
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
    let mut writer = TreePayloadWriterV1::new(scratch);
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
pub(super) fn encode_directory<S: PreparedTreeSinkV1 + ?Sized>(
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
    let mut writer = TreePayloadWriterV1::new(scratch);
    writer.write(&[0x01])?;
    writer.write(&mode.to_be_bytes())?;
    writer.write(&entry_count.to_be_bytes())?;
    writer.write(&[page_depth, u8::from(root_page.is_some())])?;
    if let Some(id) = root_page {
        writer.write(id.as_bytes())?;
    }
    finish_tree_object(writer, sink, counters).map(|(id, _)| id)
}

pub(super) fn finish_tree_object<S: PreparedTreeSinkV1 + ?Sized>(
    mut writer: TreePayloadWriterV1<'_>,
    sink: &mut S,
    counters: &mut OperationCountersV1,
) -> CoreResult<(PhysicalTreeIdV1, u32)> {
    let payload_len = writer.position();
    let (typed_id, complete_len) = seal_physical_object_in_place_v1(
        PhysicalObjectKindV1::Tree,
        writer.bytes_mut(),
        payload_len,
    )?;
    writer.position = complete_len;
    let bytes = writer.complete_bytes();
    let decoded = decode_physical_object_v1(bytes, &mut DiscardStrongEdgesV1)?;
    if decoded.header().kind() != PhysicalObjectKindV1::Tree {
        return Err(CoreError::TypeDomain);
    }
    let id = match typed_id {
        TypedPhysicalObjectIdV1::Tree(id) => id,
        _ => return Err(CoreError::TypeDomain),
    };
    if decoded.physical_id()? != typed_id {
        return Err(CoreError::IdMismatch);
    }
    let object_len = u32::try_from(complete_len).map_err(|_| CoreError::IntegerOverflow)?;
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

pub(super) struct TreePayloadWriterV1<'a> {
    bytes: &'a mut [u8; MAX_TREE_OBJECT_BYTES],
    position: usize,
}

impl<'a> TreePayloadWriterV1<'a> {
    pub(super) fn new(bytes: &'a mut [u8; MAX_TREE_OBJECT_BYTES]) -> Self {
        Self { bytes, position: 0 }
    }

    pub(super) const fn position(&self) -> usize {
        self.position
    }

    pub(super) fn write(&mut self, value: &[u8]) -> CoreResult<()> {
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

    pub(super) fn overwrite(&mut self, offset: usize, value: &[u8]) -> CoreResult<()> {
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

    pub(super) fn bytes_mut(&mut self) -> &mut [u8] {
        self.bytes
    }

    pub(super) fn complete_bytes(&self) -> &[u8] {
        &self.bytes[..self.position]
    }
}

#[derive(Clone, Copy)]
pub(super) struct TreePlanV1 {
    pub(super) entry_count: usize,
    pub(super) page_depth: u8,
    pub(super) leaf_count: usize,
    pub(super) level_one_count: usize,
    pub(super) summary_count: usize,
    pub(super) tree_object_count: u32,
}

impl TreePlanV1 {
    pub(super) fn for_count(entry_count: usize) -> CoreResult<Self> {
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

pub(super) const fn map_sink(_: TreeSinkErrorV1) -> CoreError {
    CoreError::SinkRefused
}
