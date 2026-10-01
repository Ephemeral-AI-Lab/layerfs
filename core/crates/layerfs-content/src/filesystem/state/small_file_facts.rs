//! Same open authenticated Facts epoch, selected positive file answers only.
use super::empty_owner::bad;
use super::*;
use crate::ContentResult;
impl VerifiedSmallFileState {
    fn fact_context(&self, scope: &FactScope) -> ContentResult<()> {
        self.inner.live()?;
        if scope.state().selection() != self.selection()
            || scope.subject().selected() != self.subject()
            || scope.subject().table() != Some(self.data.table)
            || scope.state().phase() != 4
            || scope.state().table() != StateTable::BaseFacts
        {
            return Err(bad("small file foreign actual Facts table"));
        }
        Ok(())
    }
}
impl FactState for VerifiedSmallFileState {
    fn fact_capacity(&self, scope: &FactScope) -> ContentResult<FactCapacity> {
        self.fact_context(scope)?;
        FactCapacity::new(8, 0, ALIAS_FIXED_BYTES + FACT_OWNER_ALLOWANCE + 8 * 105)
    }
    fn fact_memory(&self, scope: &FactScope) -> ContentResult<GraphMemory> {
        self.fact_context(scope)?;
        Ok(self.inner.memory.clone())
    }
    fn fact_bind(&mut self, scope: &FactScope) -> ContentResult<()> {
        self.fact_context(scope)?;
        if self.data.fact_scope.is_some() {
            return Err(bad("small file fact bind replay"));
        }
        self.inner.fact_bind(scope)?;
        self.data.fact_scope = Some(scope.clone());
        Ok(())
    }
    fn fact_get(&mut self, scope: &FactScope, serial: u64) -> ContentResult<Option<BaseFact>> {
        self.fact_context(scope)?;
        scope.key(serial)?;
        if self.data.fact_scope.as_ref() != Some(scope)
            || self.inner.data.facts_retired
            || self.data.fact_seal.is_some()
        {
            return Err(bad("small file fact get phase"));
        }
        // Unknown is not an invented absent inode; only the verified source's
        // declared file answers are admitted by this separately selected class.
        Ok(Some(self.base(serial)?))
    }
    fn fact_insert(&mut self, scope: &FactScope, records: &[BaseFact]) -> ContentResult<()> {
        self.fact_context(scope)?;
        if self.data.fact_scope.as_ref() != Some(scope)
            || self.inner.data.facts_retired
            || self.data.fact_seal.is_some()
            || records.len() > 8
        {
            return Err(bad("small file fact insertion phase/capacity"));
        }
        for record in records {
            if *record != self.base(record.serial)? {
                return Err(bad("small file unverified fact"));
            }
        }
        Ok(())
    }
    fn fact_seal(&mut self, scope: &FactScope) -> ContentResult<FactSeal> {
        self.fact_context(scope)?;
        if self.data.fact_scope.as_ref() != Some(scope)
            || self.data.fact_seal.is_some()
            || self.data.stage != super::small_file_owner::SmallStage::Final
            || !self.data.final_eof
        {
            return Err(bad("small file Facts before final consumer EOF"));
        }
        let mut ledger = FactLedger::new(scope.clone())?;
        for fact in self.data.bases[..self.data.len].iter().flatten() {
            ledger.base(&[*fact])?;
        }
        let seal = ledger.seal();
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
        self.fact_context(&seal.scope)?;
        if self.data.fact_seal.as_ref() != Some(seal) || self.inner.data.facts_retired {
            return Err(bad("small file selected Fact seal"));
        }
        let capacity = Self::page_limit(
            after,
            seal.maximum,
            records,
            bytes,
            FACT_PAGE_HEADER_BYTES,
            105,
        )?;
        let memory = self.inner.memory.reserve(
            std::mem::size_of::<FactPage<BaseFact>>() + capacity * std::mem::size_of::<BaseFact>(),
        )?;
        let mut found = Vec::new();
        found.try_reserve_exact(capacity).map_err(|_| {
            crate::ContentError::ResourceUnavailable {
                what: "small_file.fact_page",
            }
        })?;
        for fact in self.data.bases[..self.data.len]
            .iter()
            .flatten()
            .filter(|r| after.is_none_or(|s| r.serial > s))
            .take(capacity)
        {
            found.push(*fact);
        }
        let last = found.last().map(|r| r.serial).or(after);
        let eof = last == seal.maximum;
        let page = FactPage::new(memory, seal.clone(), found, last, eof)?;
        self.data.fact_eof |= eof;
        Ok(page)
    }
    fn fact_retire(&mut self, seal: &FactSeal) -> ContentResult<()> {
        self.fact_context(&seal.scope)?;
        if self.data.fact_seal.as_ref() != Some(seal)
            || !self.data.fact_eof
            || self.inner.data.facts_retired
        {
            return Err(bad("small file Facts retire phase/EOF"));
        }
        self.inner.data.facts_retired = true;
        Ok(())
    }
    fn fact_abandon(&mut self, scope: &FactScope) -> ContentResult<()> {
        self.fact_context(scope)?;
        self.abandon();
        Ok(())
    }
}
