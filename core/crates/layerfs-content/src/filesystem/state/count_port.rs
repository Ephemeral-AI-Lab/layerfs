//! Narrow actual count/declared/touched/zero/final-row authority.
use super::{
    BaseFact, CanonicalCapacity, CanonicalScope, CountEpoch, CountPage, CountRecord, CountSeal,
    GraphMemory, ZeroPage, ZeroSeal,
};
use crate::ContentResult;
/// One selected actual count table with known immutable read epochs.
pub trait CountState {
    /// Previously captured independent classes and combined aggregate bytes.
    fn count_capacity(&self, scope: &CanonicalScope) -> ContentResult<CanonicalCapacity>;
    /// Same scoped64KiB working admission as the namespace owner.
    fn count_memory(&self, scope: &CanonicalScope) -> ContentResult<GraphMemory>;
    /// Bind authenticated table and admit the deferred owner before first SQL.
    fn count_begin(&mut self, scope: &CanonicalScope) -> ContentResult<()>;
    /// Exact current count/declaration record, without creating a touched row.
    fn count_get(
        &mut self,
        scope: &CanonicalScope,
        serial: u64,
    ) -> ContentResult<Option<CountRecord>>;
    /// Compare full before/proposed values before one known mutation acknowledgement.
    fn count_cas(
        &mut self,
        scope: &CanonicalScope,
        before: Option<CountRecord>,
        after: &CountRecord,
    ) -> ContentResult<CountRecord>;
    /// Freeze an exact Effects or Final immutable epoch, with full ordered transcript.
    fn count_seal(&mut self, scope: &CanonicalScope, epoch: CountEpoch)
        -> ContentResult<CountSeal>;
    /// Bounded advancing exact immutable rows, keeping consumer-held capacity charged.
    fn count_page(
        &mut self,
        seal: &CountSeal,
        after: Option<u64>,
        records: usize,
        bytes: usize,
    ) -> ContentResult<CountPage>;
    /// Ordered zero candidates derived from the selected immutable effect snapshot.
    fn zero_append(&mut self, counts: &CountSeal, records: &[BaseFact]) -> ContentResult<()>;
    /// Known exact candidate EOF before any descendant traversal.
    fn zero_seal(&mut self, counts: &CountSeal) -> ContentResult<ZeroSeal>;
    /// Bounded advancing carried base records with exact candidate EOF.
    fn zero_page(
        &mut self,
        seal: &ZeroSeal,
        after: Option<u64>,
        records: usize,
        bytes: usize,
    ) -> ContentResult<ZeroPage>;
    /// Enable descendant effects only for that known exact complete seed epoch.
    fn count_resume(&mut self, seeds: &ZeroSeal) -> ContentResult<()>;
    /// Exact bounded count/candidate retirement after final consumer EOF.
    fn count_retire(
        &mut self,
        final_seal: &CountSeal,
        seeds: Option<&ZeroSeal>,
    ) -> ContentResult<()>;
    /// Selected metadata-only terminalization, with no guessed refund or cleanup.
    fn count_abandon(&mut self, scope: &CanonicalScope) -> ContentResult<()>;
}
