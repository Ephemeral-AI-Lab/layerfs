//! Substantive structural COW mutation mechanics.
//!
//! Canonical tree construction remains in `tree`; this module owns mutation
//! classification, authenticated validation, affected-spine rebuilding, and
//! exact reuse accounting.

use super::tree::{
    encode_directory, encode_index_boundaries, encode_leaf, finish_tree_object, map_sink,
    validate_entries, CanonicalDirectoryTreeV1, CanonicalTreeChildV1, CanonicalTreeEntryV1,
    CowTreeMutationV1, CowTreeReplacementV1, DirectoryLogicalIdentityV1, PreparedTreeSinkV1,
    TreeObjectDispositionV1, TreePageBoundaryV1, TreePageSummaryV1, TreePayloadWriterV1,
    TreePlanV1, MAX_TREE_OBJECT_BYTES, TREE_INDEX_FANOUT, TREE_LEAF_FANOUT,
};
use super::view::{
    authenticate_and_derive_mutation_logical, mutation_evidence_resident_bytes_v1,
    mutation_hash_state_bytes_v1, read_base_snapshot, read_result_snapshot,
    replacement_evidence_resident_bytes_v1, validate_mutation_physical_evidence,
    validate_mutation_relation, validate_replacement_evidence, AuthenticatedTreeMutationEvidenceV1,
    AuthenticatedTreeReplacementEvidenceV1, CanonicalTreeMutationSourceV1, TreeEntrySnapshotV1,
    TreeProofMutationV1,
};
use crate::format::compare_unsigned;
use crate::identity::{COMPARISON_WINDOW_BYTES, IDENTITY_HASHER_BYTES_V1};
use crate::limits::OperationReservationV1;
use crate::limits::ResourceLedgerV1;
use crate::limits::{
    CounterFieldV1, MemoryComponentV1, OperationCountersV1, OperationMemoryPlanV1,
};
use crate::{CoreError, CoreResult};

#[allow(clippy::too_many_arguments)]
pub fn replace_directory_entry_cow_v1<S: PreparedTreeSinkV1 + ?Sized>(
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
pub(crate) fn replace_directory_entry_cow_borrowed_v1<S: PreparedTreeSinkV1 + ?Sized>(
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

#[allow(clippy::too_many_arguments)]
pub fn add_directory_entry_cow_v1<T, S>(
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
pub(crate) fn add_directory_entry_cow_borrowed_v1<T, S>(
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

#[allow(clippy::too_many_arguments)]
pub fn remove_directory_entry_cow_v1<T, S>(
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
pub(crate) fn remove_directory_entry_cow_borrowed_v1<T, S>(
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
pub(crate) fn move_directory_entry_cow_borrowed_v1<T, S>(
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
pub(crate) fn replace_two_directory_entries_cow_borrowed_v1<T, S>(
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

impl<'a> TreeMutationKindV1<'a> {
    fn proof(self) -> TreeProofMutationV1<'a> {
        match self {
            Self::Add(entry) => TreeProofMutationV1::Add(entry),
            Self::Remove(entry) => TreeProofMutationV1::Remove(entry),
            Self::Move {
                removal_index,
                insertion_index,
                expected_removed,
                moved,
            } => TreeProofMutationV1::Move {
                removal_index,
                insertion_index,
                expected_removed,
                moved,
            },
            Self::ReplacePair {
                first_index,
                first_expected,
                first_replacement,
                second_index,
                second_expected,
                second_replacement,
            } => TreeProofMutationV1::ReplacePair {
                first_index,
                first_expected,
                first_replacement,
                second_index,
                second_expected,
                second_replacement,
            },
        }
    }
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
    let proof_mutation = mutation.proof();
    validate_mutation_relation(
        source,
        mutation_index,
        proof_mutation,
        base_count,
        result_count,
    )?;
    let logical = authenticate_and_derive_mutation_logical(
        base,
        evidence,
        mutation_index,
        proof_mutation,
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
    let mut writer = TreePayloadWriterV1::new(scratch);
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
