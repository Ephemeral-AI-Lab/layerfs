//! Explicit successful return differs from native close/unlink and never refunds S.
use super::{authority::Slot, pool_state::IDLE_LIMIT, status::ScratchDisposition, ScratchSession};
use crate::{StorageError, StorageResult};
impl ScratchSession {
    /// Observe the owned descriptor number for identity/lifetime diagnostics only.
    /// This grants no descriptor adoption, close or reconstruction authority.
    pub fn native_descriptor_observation(&self) -> StorageResult<i32> {
        let r = self
            .resource
            .as_ref()
            .ok_or(StorageError::Integrity("scratch pool released owner"))?;
        r.verify()?;
        r.native.descriptor_observation()
    }

    /// Transfer a known successful reset owner to idle; preserve its native S.
    /// Held data/metadata refusal leaves this session available for explicit release.
    pub fn return_to_idle(&mut self) -> StorageResult<()> {
        self.pool_check_success()?;
        if self
            .resource
            .as_ref()
            .is_some_and(|r| r.plan.version() == 6 && !r.known_clean)
        {
            self.reset_completed_drafts()?;
        }
        let r = self
            .resource
            .as_ref()
            .ok_or(StorageError::Integrity("scratch pool released owner"))?;
        r.pool_ready(1)?;
        let (shared, index) = self.pool_destination();
        let mut slots = shared
            .slots
            .lock()
            .map_err(|_| StorageError::Integrity("construction scratch owner lock"))?;
        if !matches!(&slots[index],Slot::Active(s) if s.token==r.selection.token()) {
            return Err(StorageError::Integrity("scratch pool active slot identity"));
        }
        if slots
            .iter()
            .filter(|s| matches!(s, Slot::Idle { .. }))
            .count()
            >= shared.limit.min(IDLE_LIMIT)
        {
            slots.refusal()?;
            return Err(StorageError::OwnershipUnavailable);
        }
        slots.counters.returns.begin()?;
        let mut status = r.status(ScratchDisposition::Sealed, None, false, false);
        // Status is not another controller owner while Idle. Old external
        // snapshots were already excluded by pool_ready's exact handle check.
        status.graph_working = None;
        let resource = self
            .pool_take_resource()
            .ok_or(StorageError::Integrity("scratch pool released owner"))?;
        slots[index] = Slot::Idle { resource, status };
        slots.counters.returns.finish(true)?;
        Ok(())
    }
}
