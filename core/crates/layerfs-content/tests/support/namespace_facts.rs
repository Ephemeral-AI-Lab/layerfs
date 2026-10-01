//! External finite fact fixture; maps are an oracle, not native RAM qualification.
use super::ObservedGraph;
use layerfs_content::filesystem::state::*;
use layerfs_content::{ContentError, ContentResult};
use std::collections::BTreeMap;
#[derive(Default)]
pub struct FixtureFacts {
    pub base: Option<FactScope>,
    pub parent: Option<FactScope>,
    pub bases: BTreeMap<u64, BaseFact>,
    pub parents: BTreeMap<u64, ParentFact>,
    pub base_seal: Option<FactSeal>,
    pub parent_seal: Option<ParentSeal>,
    pub closed: bool,
    pub memory: GraphMemory,
}
fn bad() -> ContentError {
    ContentError::InvalidOrderingRecord("external fact fixture")
}
impl FactState for ObservedGraph {
    fn fact_capacity(&self, scope: &FactScope) -> ContentResult<FactCapacity> {
        self.fact_memory(scope)?;
        FactCapacity::new(100000, 100000, 16 * 1024 * 1024)
    }
    fn fact_memory(&self, scope: &FactScope) -> ContentResult<GraphMemory> {
        if self.facts.base.as_ref() != Some(scope) {
            return Err(bad());
        }
        Ok(self.facts.memory.clone())
    }
    fn fact_bind(&mut self, scope: &FactScope) -> ContentResult<()> {
        if self.facts.base.is_some() || scope.subject().selected() != self.scope.subject() {
            return Err(bad());
        }
        self.facts.base = Some(scope.clone());
        Ok(())
    }
    fn fact_get(&mut self, scope: &FactScope, serial: u64) -> ContentResult<Option<BaseFact>> {
        self.fact_memory(scope)?;
        Ok(self.facts.bases.get(&serial).copied())
    }
    fn fact_insert(&mut self, scope: &FactScope, records: &[BaseFact]) -> ContentResult<()> {
        self.fact_memory(scope)?;
        if self.facts.base_seal.is_some() || records.len() > 128 {
            return Err(bad());
        }
        for row in records {
            if self.facts.bases.insert(row.serial, *row).is_some() {
                return Err(bad());
            }
        }
        Ok(())
    }
    fn fact_seal(&mut self, scope: &FactScope) -> ContentResult<FactSeal> {
        self.fact_memory(scope)?;
        let mut ledger = FactLedger::new(scope.clone())?;
        for row in self.facts.bases.values() {
            ledger.base(&[*row])?;
        }
        let seal = ledger.seal();
        self.facts.base_seal = Some(seal.clone());
        Ok(seal)
    }
    fn fact_page(
        &mut self,
        seal: &FactSeal,
        after: Option<u64>,
        records: usize,
        bytes: usize,
    ) -> ContentResult<FactPage<BaseFact>> {
        if self.facts.base_seal.as_ref() != Some(seal)
            || records == 0
            || records > 128
            || bytes < 301
        {
            return Err(bad());
        }
        let count = records.min((bytes - 301) / 105);
        let rows: Vec<_> = self
            .facts
            .bases
            .values()
            .filter(|r| after.is_none_or(|s| r.serial > s))
            .take(count)
            .copied()
            .collect();
        let last = rows.last().map(|r| r.serial).or(after);
        let eof = last == seal.maximum;
        let lease = self.facts.memory.reserve(
            std::mem::size_of::<FactPage<BaseFact>>()
                + rows.capacity() * std::mem::size_of::<BaseFact>(),
        )?;
        FactPage::new(lease, seal.clone(), rows, last, eof)
    }
    fn fact_retire(&mut self, seal: &FactSeal) -> ContentResult<()> {
        if self.facts.base_seal.as_ref() != Some(seal) {
            return Err(bad());
        }
        self.facts.bases.clear();
        Ok(())
    }
    fn fact_abandon(&mut self, _scope: &FactScope) -> ContentResult<()> {
        self.failed = true;
        Ok(())
    }
}
impl ParentEligibilityState for ObservedGraph {
    fn parent_bind(&mut self, scope: &FactScope) -> ContentResult<()> {
        if self.facts.parent.is_some()
            || self
                .facts
                .base
                .as_ref()
                .is_none_or(|base| base.subject() != scope.subject())
        {
            return Err(bad());
        }
        self.facts.parent = Some(scope.clone());
        Ok(())
    }
    fn parent_insert(&mut self, scope: &FactScope, serials: &[u64]) -> ContentResult<()> {
        if self.facts.parent.as_ref() != Some(scope) || self.facts.closed {
            return Err(bad());
        }
        for serial in serials {
            if self
                .facts
                .parents
                .insert(
                    *serial,
                    ParentFact {
                        serial: *serial,
                        bound: false,
                    },
                )
                .is_some()
            {
                return Err(bad());
            }
        }
        Ok(())
    }
    fn parent_close_declarations(&mut self, scope: &FactScope) -> ContentResult<()> {
        if self.facts.parent.as_ref() != Some(scope) || self.facts.closed {
            return Err(bad());
        }
        self.facts.closed = true;
        Ok(())
    }
    fn parent_mark_bound(&mut self, scope: &FactScope, children: &[u64]) -> ContentResult<()> {
        if self.facts.parent.as_ref() != Some(scope) || !self.facts.closed {
            return Err(bad());
        }
        for serial in children {
            if let Some(p) = self.facts.parents.get_mut(serial) {
                p.bound = true;
            }
        }
        Ok(())
    }
    fn parent_seal(&mut self, scope: &FactScope) -> ContentResult<ParentSeal> {
        if self.facts.parent.as_ref() != Some(scope) || !self.facts.closed {
            return Err(bad());
        }
        let mut ledger = FactLedger::new(scope.clone())?;
        for row in self.facts.parents.values() {
            ledger.parents(&[*row])?;
        }
        let seal = ledger.parent_seal()?;
        self.facts.parent_seal = Some(seal.clone());
        Ok(seal)
    }
    fn parent_get(&mut self, seal: &ParentSeal, serial: u64) -> ContentResult<Option<ParentFact>> {
        if self.facts.parent_seal.as_ref() != Some(seal) {
            return Err(bad());
        }
        Ok(self.facts.parents.get(&serial).copied())
    }
    fn parent_page(
        &mut self,
        seal: &ParentSeal,
        after: Option<u64>,
        records: usize,
        bytes: usize,
    ) -> ContentResult<FactPage<ParentFact>> {
        if self.facts.parent_seal.as_ref() != Some(seal)
            || records == 0
            || records > 128
            || bytes < 301
        {
            return Err(bad());
        }
        let count = records.min((bytes - 301) / 32);
        let rows: Vec<_> = self
            .facts
            .parents
            .values()
            .filter(|r| after.is_none_or(|s| r.serial > s))
            .take(count)
            .copied()
            .collect();
        let last = rows.last().map(|r| r.serial).or(after);
        let eof = last == seal.facts.maximum;
        let lease = self.facts.memory.reserve(
            std::mem::size_of::<FactPage<ParentFact>>()
                + rows.capacity() * std::mem::size_of::<ParentFact>(),
        )?;
        FactPage::new(lease, seal.facts.clone(), rows, last, eof)
    }
    fn parent_retire(&mut self, seal: &ParentSeal) -> ContentResult<()> {
        if self.facts.parent_seal.as_ref() != Some(seal) {
            return Err(bad());
        }
        self.facts.parents.clear();
        Ok(())
    }
    fn parent_abandon(&mut self, _scope: &FactScope) -> ContentResult<()> {
        self.failed = true;
        Ok(())
    }
}
