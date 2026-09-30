//! C1 IndexedState adapter that preserves the original typed C2 failure.

use layerfs_content::filesystem::state::{
    IndexedState, PageLimit, StateCapacity, StateKey, StatePage, StateRecord, StateScope, StateSeal,
};
use layerfs_content::ContentResult;

use super::ScratchSession;

/// Borrowed C1 port for one exact private C2 session.
pub struct ScratchAdapter<'a> {
    session: &'a mut ScratchSession,
}

impl ScratchSession {
    /// Supplies the same selected C2 state to the real C1 producer/consumer.
    pub fn adapter(&mut self) -> ScratchAdapter<'_> {
        ScratchAdapter { session: self }
    }
}

impl IndexedState for ScratchAdapter<'_> {
    fn capacity(&self, scope: &StateScope) -> ContentResult<StateCapacity> {
        self.session
            .capacity(scope)
            .map_err(|error| self.session.keep_failure(error))
    }
    fn append(&mut self, scope: &StateScope, records: &[StateRecord]) -> ContentResult<()> {
        self.session
            .append(scope, records)
            .map_err(|error| self.session.keep_failure(error))
    }
    fn seal(&mut self, scope: &StateScope) -> ContentResult<StateSeal> {
        self.session
            .seal(scope)
            .map_err(|error| self.session.keep_failure(error))
    }
    fn get(&mut self, seal: &StateSeal, key: StateKey) -> ContentResult<Option<StateRecord>> {
        self.session
            .get(seal, key)
            .map_err(|error| self.session.keep_failure(error))
    }
    fn page(
        &mut self,
        seal: &StateSeal,
        after: Option<StateKey>,
        limit: PageLimit,
    ) -> ContentResult<StatePage> {
        self.session
            .page(seal, after, limit)
            .map_err(|error| self.session.keep_failure(error))
    }
    fn release(&mut self, scope: &StateScope) -> ContentResult<()> {
        self.session
            .complete_phase(scope)
            .map_err(|error| self.session.keep_failure(error))
    }
}
