//! Public-to-crate structural COW mutation operations.
//!
//! This owner keeps add, remove, and same-name replacement classified as
//! distinct mutations while delegating canonical page encoding to `tree`.

use super::tree::{
    self, AuthenticatedTreeMutationEvidenceV1, AuthenticatedTreeReplacementEvidenceV1,
    CanonicalDirectoryTreeV1, CanonicalTreeEntryV1, CowTreeMutationV1, CowTreeReplacementV1,
    PreparedTreeSinkV1, TreePageSummaryV1, MAX_TREE_OBJECT_BYTES,
};
use super::view::CanonicalTreeMutationSourceV1;
use crate::identity::COMPARISON_WINDOW_BYTES;
use crate::limits::OperationCountersV1;
#[cfg(feature = "operation-polymorphism")]
use crate::limits::OperationReservationV1;
#[cfg(test)]
use crate::limits::ResourceLedgerV1;
use crate::CoreResult;

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
pub(crate) fn replace_directory_entry_cow_v1<S: PreparedTreeSinkV1 + ?Sized>(
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
    tree::replace_directory_entry_cow_impl_v1(
        base,
        evidence,
        replacement_index,
        replacement,
        sink,
        ledger,
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
    tree::replace_directory_entry_cow_borrowed_impl_v1(
        base,
        evidence,
        replacement_index,
        replacement,
        sink,
        reservation,
        counters,
        object_scratch,
        logical_scratch,
    )
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
pub(crate) fn add_directory_entry_cow_v1<T, S>(
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
    tree::add_directory_entry_cow_impl_v1(
        base,
        evidence,
        insertion_index,
        added,
        source,
        sink,
        ledger,
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
    tree::add_directory_entry_cow_borrowed_impl_v1(
        base,
        evidence,
        insertion_index,
        added,
        source,
        sink,
        reservation,
        counters,
        object_scratch,
        logical_scratch,
        page_scratch,
    )
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
pub(crate) fn remove_directory_entry_cow_v1<T, S>(
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
    tree::remove_directory_entry_cow_impl_v1(
        base,
        evidence,
        removal_index,
        expected_removed,
        source,
        sink,
        ledger,
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
    tree::remove_directory_entry_cow_borrowed_impl_v1(
        base,
        evidence,
        removal_index,
        expected_removed,
        source,
        sink,
        reservation,
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
    tree::move_directory_entry_cow_borrowed_impl_v1(
        base,
        evidence,
        removal_index,
        insertion_index,
        expected_removed,
        moved,
        source,
        sink,
        reservation,
        counters,
        object_scratch,
        logical_scratch,
        page_scratch,
    )
}

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
    tree::replace_two_directory_entries_cow_borrowed_impl_v1(
        base,
        evidence,
        first_index,
        first_expected,
        first_replacement,
        second_index,
        second_expected,
        second_replacement,
        source,
        sink,
        reservation,
        counters,
        object_scratch,
        logical_scratch,
        page_scratch,
    )
}
