//! Exact authenticated-table binding and zero fact/parent transcripts.
use super::empty_owner::{bad, growth};
use super::*;
use crate::ContentResult;
impl VerifiedEmptyState {
    fn fact_scope(&self, scope: &FactScope, parent: bool) -> ContentResult<()> {
        if scope.state().selection() != self.selection()
            || scope.subject().selected() != self.subject()
            || scope.subject().table().is_none()
            || scope.state().phase() != if parent { 5 } else { 4 }
            || scope.state().table()
                != if parent {
                    StateTable::ParentEligibility
                } else {
                    StateTable::BaseFacts
                }
        {
            return Err(bad("empty foreign authenticated table"));
        }
        self.live()
    }
    fn selected_facts(&self, seal: &FactSeal) -> ContentResult<()> {
        self.fact_scope(&seal.scope, false)?;
        if self.data.fact_seal.as_ref() != Some(seal) || self.data.facts_retired {
            return Err(bad("empty selected Facts"));
        }
        Ok(())
    }
    fn selected_parents(&self, seal: &ParentSeal) -> ContentResult<()> {
        self.fact_scope(&seal.facts.scope, true)?;
        if self.data.parent_seal.as_ref() != Some(seal) || self.data.parents_retired {
            return Err(bad("empty selected Parents"));
        }
        Ok(())
    }
    fn fact_page_limit(after: Option<u64>, records: usize, bytes: usize) -> ContentResult<()> {
        if after.is_some()
            || records == 0
            || records > 128
            || !(FACT_PAGE_HEADER_BYTES..=65536).contains(&bytes)
        {
            return Err(bad("empty fact EOF/limit"));
        }
        Ok(())
    }
}
impl FactState for VerifiedEmptyState {
    fn fact_capacity(&self, scope: &FactScope) -> ContentResult<FactCapacity> {
        self.fact_scope(scope, false)?;
        Ok(FactCapacity::verified_empty())
    }
    fn fact_memory(&self, scope: &FactScope) -> ContentResult<GraphMemory> {
        self.fact_scope(scope, false)?;
        Ok(self.memory.clone())
    }
    fn fact_bind(&mut self, scope: &FactScope) -> ContentResult<()> {
        self.fact_scope(scope, false)?;
        if self.data.facts.is_some() {
            return Err(bad("empty fact bind replay"));
        }
        self.data.facts = Some(scope.clone());
        Ok(())
    }
    fn fact_get(&mut self, scope: &FactScope, serial: u64) -> ContentResult<Option<BaseFact>> {
        self.fact_scope(scope, false)?;
        scope.key(serial)?;
        if self.data.facts.as_ref() != Some(scope) || self.data.facts_retired {
            return Err(bad("empty fact get phase"));
        }
        Ok(None)
    }
    fn fact_insert(&mut self, scope: &FactScope, records: &[BaseFact]) -> ContentResult<()> {
        self.fact_scope(scope, false)?;
        growth(records.len())?;
        if self.data.facts.as_ref() != Some(scope) || self.data.fact_seal.is_some() {
            return Err(bad("empty Facts insertion phase"));
        }
        Ok(())
    }
    fn fact_seal(&mut self, scope: &FactScope) -> ContentResult<FactSeal> {
        self.fact_scope(scope, false)?;
        if self.data.facts.as_ref() != Some(scope) || self.data.fact_seal.is_some() {
            return Err(bad("empty fact seal replay/phase"));
        }
        let seal = FactLedger::new(scope.clone())?.seal();
        self.data.fact_seal = Some(seal.clone());
        Ok(seal)
    }
    fn fact_page(
        &mut self,
        seal: &FactSeal,
        after: Option<u64>,
        records: usize,
        bytes: usize,
    ) -> ContentResult<FactPage<BaseFact>> {
        self.selected_facts(seal)?;
        Self::fact_page_limit(after, records, bytes)?;
        let lease = self
            .memory
            .reserve(std::mem::size_of::<FactPage<BaseFact>>())?;
        let page = FactPage::new(lease, seal.clone(), Vec::new(), None, true)?;
        self.data.fact_eof = true;
        Ok(page)
    }
    fn fact_retire(&mut self, seal: &FactSeal) -> ContentResult<()> {
        self.selected_facts(seal)?;
        if !self.data.fact_eof {
            return Err(bad("empty Facts retirement before EOF"));
        }
        self.data.facts_retired = true;
        Ok(())
    }
    fn fact_abandon(&mut self, scope: &FactScope) -> ContentResult<()> {
        self.fact_scope(scope, false)?;
        self.abandon();
        Ok(())
    }
}
impl ParentEligibilityState for VerifiedEmptyState {
    fn parent_bind(&mut self, scope: &FactScope) -> ContentResult<()> {
        self.fact_scope(scope, true)?;
        if self.data.parents.is_some() {
            return Err(bad("empty parent bind replay"));
        }
        self.data.parents = Some(scope.clone());
        Ok(())
    }
    fn parent_insert(&mut self, scope: &FactScope, serials: &[u64]) -> ContentResult<()> {
        self.fact_scope(scope, true)?;
        growth(serials.len())?;
        if self.data.parents.as_ref() != Some(scope) || self.data.parents_closed {
            return Err(bad("empty parent insertion phase"));
        }
        Ok(())
    }
    fn parent_close_declarations(&mut self, scope: &FactScope) -> ContentResult<()> {
        self.fact_scope(scope, true)?;
        if self.data.parents.as_ref() != Some(scope) || self.data.parents_closed {
            return Err(bad("empty parent declarations replay/phase"));
        }
        self.data.parents_closed = true;
        Ok(())
    }
    fn parent_mark_bound(&mut self, scope: &FactScope, children: &[u64]) -> ContentResult<()> {
        self.fact_scope(scope, true)?;
        growth(children.len())?;
        if !self.data.parents_closed || self.data.parent_seal.is_some() {
            return Err(bad("empty parent marking phase"));
        }
        Ok(())
    }
    fn parent_seal(&mut self, scope: &FactScope) -> ContentResult<ParentSeal> {
        self.fact_scope(scope, true)?;
        if !self.data.parents_closed || self.data.parent_seal.is_some() {
            return Err(bad("empty parent seal phase/replay"));
        }
        let seal = FactLedger::new(scope.clone())?.parent_seal()?;
        self.data.parent_seal = Some(seal.clone());
        Ok(seal)
    }
    fn parent_get(&mut self, seal: &ParentSeal, serial: u64) -> ContentResult<Option<ParentFact>> {
        self.selected_parents(seal)?;
        seal.facts.scope.key(serial)?;
        Ok(None)
    }
    fn parent_page(
        &mut self,
        seal: &ParentSeal,
        after: Option<u64>,
        records: usize,
        bytes: usize,
    ) -> ContentResult<FactPage<ParentFact>> {
        self.selected_parents(seal)?;
        Self::fact_page_limit(after, records, bytes)?;
        let lease = self
            .memory
            .reserve(std::mem::size_of::<FactPage<ParentFact>>())?;
        let page = FactPage::new(lease, seal.facts.clone(), Vec::new(), None, true)?;
        self.data.parent_eof = true;
        Ok(page)
    }
    fn parent_retire(&mut self, seal: &ParentSeal) -> ContentResult<()> {
        self.selected_parents(seal)?;
        if self.data.roots_seal.is_none()
            || !self.data.roots_eof
            || self.data.roots_released
            || !self.data.parent_eof
        {
            return Err(bad("empty Parents retirement before final consumers/EOF"));
        }
        self.data.parents_retired = true;
        Ok(())
    }
    fn parent_abandon(&mut self, scope: &FactScope) -> ContentResult<()> {
        self.fact_scope(scope, true)?;
        self.abandon();
        Ok(())
    }
}
