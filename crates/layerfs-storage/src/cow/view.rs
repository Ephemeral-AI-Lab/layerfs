//! Stable, bounded canonical-tree views used by structural COW mutation.
//!
//! The view port exposes declarations and one borrowed entry at a time. It
//! cannot publish, allocate a source-sized cache, or return an entry that
//! outlives the next read, so mutation mechanics remain independent of the
//! storage backend.

use super::tree::CanonicalTreeEntryV1;
use crate::CoreResult;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TreeMutationSourceErrorV1 {
    Failure,
}

/// Immutable entry view for general add/remove COW. Every method observes one
/// authenticated snapshot; callers preflight the two declared shapes before
/// reading the first entry.
pub(crate) trait CanonicalTreeMutationSourceV1 {
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
