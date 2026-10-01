//! Exact empty DirectoryRoots transcript and logical completion.
use super::empty_owner::{bad, growth};
use super::*;
use crate::ContentResult;
impl IndexedState for VerifiedEmptyState {
    fn capacity(&self, scope: &StateScope) -> ContentResult<StateCapacity> {
        self.roots_scope(scope)?;
        StateCapacity::new(0, 0)
    }
    fn append(&mut self, scope: &StateScope, records: &[StateRecord]) -> ContentResult<()> {
        self.roots_scope(scope)?;
        growth(records.len())?;
        if self.data.roots_seal.is_some() {
            return Err(bad("empty Roots sealed"));
        }
        Ok(())
    }
    fn seal(&mut self, scope: &StateScope) -> ContentResult<StateSeal> {
        self.roots_scope(scope)?;
        if self.data.roots_seal.is_some() {
            return Err(bad("empty Roots seal replay"));
        }
        let seal = StateLedger::new(scope.clone()).seal();
        self.data.roots_seal = Some(seal.clone());
        Ok(seal)
    }
    fn get(&mut self, seal: &StateSeal, key: StateKey) -> ContentResult<Option<StateRecord>> {
        self.roots_scope(seal.scope())?;
        if self.data.roots_seal.as_ref() != Some(seal) {
            return Err(bad("empty Roots selected seal"));
        }
        StateKey::decode(seal.scope(), key.as_bytes())?;
        Ok(None)
    }
    fn page(
        &mut self,
        seal: &StateSeal,
        after: Option<StateKey>,
        limit: PageLimit,
    ) -> ContentResult<StatePage> {
        self.roots_scope(seal.scope())?;
        if self.data.roots_seal.as_ref() != Some(seal) || after.is_some() {
            return Err(bad("empty Roots selected EOF"));
        }
        limit.check_records(0)?;
        let page = StatePage::new(seal.clone(), Vec::new(), true)?;
        self.data.roots_eof = true;
        Ok(page)
    }
    fn release(&mut self, scope: &StateScope) -> ContentResult<()> {
        self.roots_scope(scope)?;
        if self.data.roots_seal.is_none() || !self.data.roots_eof || !self.data.parents_retired {
            return Err(bad("empty Roots completion before EOF"));
        }
        self.data.roots_released = true;
        Ok(())
    }
}
