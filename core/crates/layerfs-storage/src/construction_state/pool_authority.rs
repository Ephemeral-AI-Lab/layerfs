//! Fixed same-authority idle selection and explicit close/unlink of known idle only.
use super::status::ScratchDisposition;
use super::{
    authority::Slot,
    plan::Plan,
    pool_state::{Action, PoolStatus, IDLE_LIMIT},
    ScratchAuthority, ScratchSession,
};
use crate::{StorageError, StorageResult};
use layerfs_content::filesystem::state::StateSelection;
impl ScratchAuthority {
    pub(crate) fn pick_idle(
        &self,
        selection: &StateSelection,
        plan: Plan,
    ) -> StorageResult<Option<(usize, super::session::Resource)>> {
        if !(4..=8).contains(&plan.version()) {
            return Ok(None);
        }
        let mut slots = self
            .shared
            .slots
            .lock()
            .map_err(|_| StorageError::Integrity("construction scratch owner lock"))?;
        let index=slots.iter().position(|s|matches!(s,Slot::Idle {resource,..} if resource.plan.version()==plan.version() && resource.native.reserved_bytes==plan.scratch_bytes() && resource.engine.map(std::ptr::from_ref)==self.shared.engine.map(std::ptr::from_ref)));
        let Some(index) = index else {
            return Ok(None);
        };
        slots.counters.rebind.begin()?;
        let Slot::Idle { status, .. } = &slots[index] else {
            unreachable!()
        };
        let mut active = status.clone();
        active.token = selection.token();
        active.selector = *selection.selector();
        active.disposition = ScratchDisposition::Admitted;
        active.known_clean = false;
        active.graph_working = None;
        active.retained = false;
        let Slot::Idle { resource, .. } =
            std::mem::replace(&mut slots[index], Slot::Active(active))
        else {
            unreachable!()
        };
        Ok(Some((index, resource)))
    }
    pub(crate) fn complete_pool_action(&self, action: Action, success: bool) -> StorageResult<()> {
        let mut slots = self
            .shared
            .slots
            .lock()
            .map_err(|_| StorageError::Integrity("construction scratch owner lock"))?;
        match action {
            Action::Fresh => slots.counters.fresh.finish(success),
            Action::Rebind => slots.counters.rebind.finish(success),
            Action::Drain => slots.counters.drain.finish(success),
        }
    }
    /// Exact fixed pool/current idle facts; this performs no native or SQL I/O.
    pub fn pool_status(&self) -> StorageResult<PoolStatus> {
        let slots = self
            .shared
            .slots
            .lock()
            .map_err(|_| StorageError::Integrity("construction scratch owner lock"))?;
        let mut idle = 0;
        let mut idle_bytes = 0u64;
        for slot in slots.iter() {
            if let Slot::Idle { status, .. } = slot {
                idle += 1;
                idle_bytes = idle_bytes
                    .checked_add(status.reserved_bytes)
                    .ok_or(StorageError::Integrity("scratch pool idle byte overflow"))?;
            }
        }
        Ok(PoolStatus {
            slots: self.shared.limit,
            idle_limit: self.shared.limit.min(IDLE_LIMIT),
            idle,
            idle_bytes,
            counters: slots.counters,
        })
    }
    /// Explicitly close/unlink known idle owners; active/failed/Unknown stay owned.
    pub fn drain_idle(&self) -> StorageResult<usize> {
        let mut drained = 0;
        for index in 0..self.shared.limit {
            let resource = {
                let mut slots = self
                    .shared
                    .slots
                    .lock()
                    .map_err(|_| StorageError::Integrity("construction scratch owner lock"))?;
                let Slot::Idle { resource, status } = &slots[index] else {
                    continue;
                };
                resource.pool_ready(0)?;
                let active = status.clone();
                slots.counters.drain.begin()?;
                let Slot::Idle { resource, .. } =
                    std::mem::replace(&mut slots[index], Slot::Active(active))
                else {
                    unreachable!()
                };
                resource
            };
            let mut session = ScratchSession::new(self.shared.clone(), index, resource);
            let result = session.release();
            self.complete_pool_action(Action::Drain, result.is_ok())?;
            result?;
            drained += 1;
        }
        Ok(drained)
    }
}
