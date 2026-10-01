//! Typed C1 fact/parent delegation retains original provider and Unknown errors.
use super::ScratchAdapter;
use crate::StorageError;
use layerfs_content::filesystem::state::{
    BaseFact, FactCapacity, FactPage, FactScope, FactSeal, FactState, GraphMemory,
    ParentEligibilityState, ParentFact, ParentSeal,
};
use layerfs_content::{ContentError, ContentResult};
impl ScratchAdapter<'_> {
    fn fact_failure(&self, error: StorageError) -> ContentError {
        match error {
            StorageError::Content(
                error @ ContentError::InvalidOrderingRecord("fact foreign scope"),
            ) => error,
            error => self.session.keep_failure(error),
        }
    }
}
impl FactState for ScratchAdapter<'_> {
    fn fact_capacity(&self, s: &FactScope) -> ContentResult<FactCapacity> {
        self.session
            .fact_capacity(s)
            .map_err(|e| self.fact_failure(e))
    }
    fn fact_memory(&self, s: &FactScope) -> ContentResult<GraphMemory> {
        self.session
            .fact_memory(s)
            .map_err(|e| self.fact_failure(e))
    }
    fn fact_bind(&mut self, s: &FactScope) -> ContentResult<()> {
        self.session.fact_bind(s).map_err(|e| self.fact_failure(e))
    }
    fn fact_get(&mut self, s: &FactScope, k: u64) -> ContentResult<Option<BaseFact>> {
        self.session
            .fact_get(s, k)
            .map_err(|e| self.fact_failure(e))
    }
    fn fact_insert(&mut self, s: &FactScope, r: &[BaseFact]) -> ContentResult<()> {
        self.session
            .fact_insert(s, r)
            .map_err(|e| self.fact_failure(e))
    }
    fn fact_seal(&mut self, s: &FactScope) -> ContentResult<FactSeal> {
        self.session.fact_seal(s).map_err(|e| self.fact_failure(e))
    }
    fn fact_page(
        &mut self,
        s: &FactSeal,
        a: Option<u64>,
        r: usize,
        b: usize,
    ) -> ContentResult<FactPage<BaseFact>> {
        self.session
            .fact_page(s, a, r, b)
            .map_err(|e| self.fact_failure(e))
    }
    fn fact_retire(&mut self, s: &FactSeal) -> ContentResult<()> {
        self.session
            .fact_retire(s)
            .map_err(|e| self.fact_failure(e))
    }
    fn fact_abandon(&mut self, s: &FactScope) -> ContentResult<()> {
        self.session
            .fact_abandon(s)
            .map_err(|e| self.fact_failure(e))
    }
}
impl ParentEligibilityState for ScratchAdapter<'_> {
    fn parent_bind(&mut self, s: &FactScope) -> ContentResult<()> {
        self.session
            .parent_bind(s)
            .map_err(|e| self.fact_failure(e))
    }
    fn parent_insert(&mut self, s: &FactScope, r: &[u64]) -> ContentResult<()> {
        self.session
            .parent_insert(s, r)
            .map_err(|e| self.fact_failure(e))
    }
    fn parent_close_declarations(&mut self, s: &FactScope) -> ContentResult<()> {
        self.session
            .parent_close_declarations(s)
            .map_err(|e| self.fact_failure(e))
    }
    fn parent_mark_bound(&mut self, s: &FactScope, r: &[u64]) -> ContentResult<()> {
        self.session
            .parent_mark_bound(s, r)
            .map_err(|e| self.fact_failure(e))
    }
    fn parent_seal(&mut self, s: &FactScope) -> ContentResult<ParentSeal> {
        self.session
            .parent_seal(s)
            .map_err(|e| self.fact_failure(e))
    }
    fn parent_get(&mut self, s: &ParentSeal, k: u64) -> ContentResult<Option<ParentFact>> {
        self.session
            .parent_get(s, k)
            .map_err(|e| self.fact_failure(e))
    }
    fn parent_page(
        &mut self,
        s: &ParentSeal,
        a: Option<u64>,
        r: usize,
        b: usize,
    ) -> ContentResult<FactPage<ParentFact>> {
        self.session
            .parent_page(s, a, r, b)
            .map_err(|e| self.fact_failure(e))
    }
    fn parent_retire(&mut self, s: &ParentSeal) -> ContentResult<()> {
        self.session
            .parent_retire(s)
            .map_err(|e| self.fact_failure(e))
    }
    fn parent_abandon(&mut self, s: &FactScope) -> ContentResult<()> {
        self.session
            .fact_abandon(s)
            .map_err(|e| self.fact_failure(e))
    }
}
