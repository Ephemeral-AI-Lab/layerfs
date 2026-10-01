//! Dedicated C1 frontier delegation preserves the original typed C2 error.
use super::ScratchAdapter;
use crate::StorageError;
use layerfs_content::filesystem::state::{
    AliasCapacity, AliasCurrent, AliasFrontier, AliasProgress, AliasSeal, SiteMembership,
};
use layerfs_content::{ContentError, ContentResult};
impl ScratchAdapter<'_> {
    fn alias_failure(&self, error: StorageError) -> ContentError {
        match error {
            StorageError::Content(
                error @ ContentError::InvalidOrderingRecord("alias foreign membership"),
            ) => error,
            error => self.session.keep_failure(error),
        }
    }
}
impl AliasFrontier for ScratchAdapter<'_> {
    fn alias_capacity(&self, m: &SiteMembership) -> ContentResult<AliasCapacity> {
        self.session
            .alias_capacity(m)
            .map_err(|e| self.alias_failure(e))
    }
    fn alias_begin(&mut self, m: &SiteMembership, r: Option<u64>) -> ContentResult<()> {
        self.session
            .alias_begin(m, r)
            .map_err(|e| self.alias_failure(e))
    }
    fn alias_enqueue(&mut self, m: &SiteMembership, c: &[u64]) -> ContentResult<()> {
        self.session
            .alias_enqueue(m, c)
            .map_err(|e| self.alias_failure(e))
    }
    fn alias_take(&mut self, m: &SiteMembership) -> ContentResult<Option<AliasCurrent>> {
        self.session
            .alias_take(m)
            .map_err(|e| self.alias_failure(e))
    }
    fn alias_advance(
        &mut self,
        m: &SiteMembership,
        c: AliasCurrent,
        b: &AliasProgress,
        a: &AliasProgress,
    ) -> ContentResult<()> {
        self.session
            .alias_advance(m, c, b, a)
            .map_err(|e| self.alias_failure(e))
    }
    fn alias_complete(&mut self, m: &SiteMembership, c: AliasCurrent) -> ContentResult<()> {
        self.session
            .alias_complete(m, c)
            .map_err(|e| self.alias_failure(e))
    }
    fn alias_finish(&mut self, m: &SiteMembership) -> ContentResult<AliasSeal> {
        self.session
            .alias_finish(m)
            .map_err(|e| self.alias_failure(e))
    }
    fn alias_retire(&mut self, s: &AliasSeal) -> ContentResult<()> {
        self.session
            .alias_retire(s)
            .map_err(|e| self.alias_failure(e))
    }
    fn alias_abandon(&mut self, m: &SiteMembership) -> ContentResult<()> {
        self.session
            .alias_abandon(m)
            .map_err(|e| self.alias_failure(e))
    }
}
