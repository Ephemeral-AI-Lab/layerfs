//! Typed count delegation preserves foreign custody and original storage failures.
use super::ScratchAdapter;
use crate::StorageError;
use layerfs_content::filesystem::state::{
    BaseFact, CanonicalCapacity, CanonicalScope, CountEpoch, CountPage, CountRecord, CountSeal,
    CountState, GraphMemory, ZeroPage, ZeroSeal,
};
use layerfs_content::{ContentError, ContentResult};
impl ScratchAdapter<'_> {
    fn count_failure(&self, e: StorageError) -> ContentError {
        match e {
            StorageError::Content(
                error @ ContentError::InvalidOrderingRecord("count foreign scope"),
            ) => error,
            error => self.session.keep_failure(error),
        }
    }
}
impl CountState for ScratchAdapter<'_> {
    fn count_capacity(&self, scope: &CanonicalScope) -> ContentResult<CanonicalCapacity> {
        self.session
            .count_capacity(scope)
            .map_err(|e| self.count_failure(e))
    }
    fn count_memory(&self, scope: &CanonicalScope) -> ContentResult<GraphMemory> {
        self.session
            .count_memory(scope)
            .map_err(|e| self.count_failure(e))
    }
    fn count_begin(&mut self, scope: &CanonicalScope) -> ContentResult<()> {
        self.session
            .count_begin(scope)
            .map_err(|e| self.count_failure(e))
    }
    fn count_get(
        &mut self,
        scope: &CanonicalScope,
        serial: u64,
    ) -> ContentResult<Option<CountRecord>> {
        self.session
            .count_get(scope, serial)
            .map_err(|e| self.count_failure(e))
    }
    fn count_cas(
        &mut self,
        scope: &CanonicalScope,
        before: Option<CountRecord>,
        after: &CountRecord,
    ) -> ContentResult<CountRecord> {
        self.session
            .count_cas(scope, before, after)
            .map_err(|e| self.count_failure(e))
    }
    fn count_seal(
        &mut self,
        scope: &CanonicalScope,
        epoch: CountEpoch,
    ) -> ContentResult<CountSeal> {
        self.session
            .count_seal(scope, epoch)
            .map_err(|e| self.count_failure(e))
    }
    fn count_page(
        &mut self,
        seal: &CountSeal,
        after: Option<u64>,
        records: usize,
        bytes: usize,
    ) -> ContentResult<CountPage> {
        self.session
            .count_page(seal, after, records, bytes)
            .map_err(|e| self.count_failure(e))
    }
    fn zero_append(&mut self, counts: &CountSeal, records: &[BaseFact]) -> ContentResult<()> {
        self.session
            .zero_append(counts, records)
            .map_err(|e| self.count_failure(e))
    }
    fn zero_seal(&mut self, counts: &CountSeal) -> ContentResult<ZeroSeal> {
        self.session
            .zero_seal(counts)
            .map_err(|e| self.count_failure(e))
    }
    fn zero_page(
        &mut self,
        seal: &ZeroSeal,
        after: Option<u64>,
        records: usize,
        bytes: usize,
    ) -> ContentResult<ZeroPage> {
        self.session
            .zero_page(seal, after, records, bytes)
            .map_err(|e| self.count_failure(e))
    }
    fn count_resume(&mut self, seeds: &ZeroSeal) -> ContentResult<()> {
        self.session
            .count_resume(seeds)
            .map_err(|e| self.count_failure(e))
    }
    fn count_retire(&mut self, seal: &CountSeal, seeds: Option<&ZeroSeal>) -> ContentResult<()> {
        self.session
            .count_retire(seal, seeds)
            .map_err(|e| self.count_failure(e))
    }
    fn count_abandon(&mut self, scope: &CanonicalScope) -> ContentResult<()> {
        self.session
            .count_abandon(scope)
            .map_err(|e| self.count_failure(e))
    }
}
