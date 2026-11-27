//! Coalescing adjacent slices of one payload into a single extent.
//!
//! The canonical extent partition forbids two consecutive slices of the same
//! payload that are contiguous in the payload. A deletion inside a chunk leaves
//! exactly such a pair, so the frontier merges them before the page is sealed;
//! without this step the final page would be non-canonical and its identity would
//! depend on how the caller split its edits.

use crate::file::mapping::ExtentSlice;

/// Merges two contiguous slices of the same payload, if they are contiguous.
pub fn coalesce_adjacent(previous: ExtentSlice, next: ExtentSlice) -> Option<ExtentSlice> {
    if previous.payload_object_id() != next.payload_object_id() {
        return None;
    }
    let previous_end =
        u64::from(previous.source_offset()).checked_add(u64::from(previous.logical_length()))?;
    if previous_end != u64::from(next.source_offset()) {
        return None;
    }
    let length =
        u64::from(previous.logical_length()).checked_add(u64::from(next.logical_length()))?;
    ExtentSlice::new(
        previous.payload_object_id(),
        previous.source_offset(),
        u32::try_from(length).ok()?,
    )
    .ok()
}
