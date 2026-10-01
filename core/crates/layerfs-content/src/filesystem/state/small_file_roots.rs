//! Actual hardzero Roots under the Canonical8 open-Facts lifecycle.
use super::empty_owner::{bad, growth};
use super::small_file_owner::SmallStage;
use super::*;
use crate::ContentResult;
impl VerifiedSmallFileState {
    fn roots_context(&self, scope: &StateScope) -> ContentResult<()> {
        self.inner.live()?;
        if scope != self.scopes().roots()
            || self.inner.data.graph_stage != GraphStage::Retired
            || !self.inner.data.parents_closed
            || self.inner.data.parent_seal.is_none()
            || self.data.stage == SmallStage::Unbegun
            || self.inner.data.roots_released
        {
            return Err(bad("small file selected Roots phase"));
        }
        Ok(())
    }
}
impl IndexedState for VerifiedSmallFileState {
    fn capacity(&self, scope: &StateScope) -> ContentResult<StateCapacity> {
        self.roots_context(scope)?;
        StateCapacity::new(0, 0)
    }
    fn append(&mut self, scope: &StateScope, records: &[StateRecord]) -> ContentResult<()> {
        self.roots_context(scope)?;
        growth(records.len())?;
        if self.inner.data.roots_seal.is_some() {
            return Err(bad("small file Roots sealed"));
        }
        Ok(())
    }
    fn seal(&mut self, scope: &StateScope) -> ContentResult<StateSeal> {
        self.roots_context(scope)?;
        if self.inner.data.roots_seal.is_some() {
            return Err(bad("small file Roots seal replay"));
        }
        let seal = StateLedger::new(scope.clone()).seal();
        self.inner.data.roots_seal = Some(seal.clone());
        Ok(seal)
    }
    fn get(&mut self, seal: &StateSeal, key: StateKey) -> ContentResult<Option<StateRecord>> {
        self.roots_context(seal.scope())?;
        if self.inner.data.roots_seal.as_ref() != Some(seal) {
            return Err(bad("small file selected Roots seal"));
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
        self.roots_context(seal.scope())?;
        if self.inner.data.roots_seal.as_ref() != Some(seal) || after.is_some() {
            return Err(bad("small file Roots EOF/continuation"));
        }
        limit.check_records(0)?;
        let page = StatePage::new(seal.clone(), Vec::new(), true)?;
        self.inner.data.roots_eof = true;
        Ok(page)
    }
    fn release(&mut self, scope: &StateScope) -> ContentResult<()> {
        self.roots_context(scope)?;
        if self.data.stage != SmallStage::Retired
            || !self.data.release_retired
            || !self.inner.data.facts_retired
            || !self.inner.data.parents_retired
            || !self.inner.data.roots_eof
            || self.inner.data.roots_seal.is_none()
        {
            return Err(bad("small file release before canonical/parent/root EOF"));
        }
        self.inner.data.roots_released = true;
        Ok(())
    }
}
