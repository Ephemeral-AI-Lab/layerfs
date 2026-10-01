//! Fixed existing slot-table state and checked pool operation counters.
use super::authority::Slot;
use crate::{StorageError, StorageResult};
/// Only the existing default two-owner service class admits idle reuse.
pub(crate) const IDLE_LIMIT: usize = 2;
/// Known and proposed counts for an operation; no saturated or wrapped telemetry.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PoolCounter {
    /// Every selected prospective action, including failures and Unknown.
    pub attempts: u64,
    /// Known completed successful actions.
    pub succeeded: u64,
    /// Known failure acknowledgements, including an explicitly observed Unknown.
    pub failed: u64,
    /// Actors selected but not yet returned to their fixed slot.
    pub pending: u64,
}
impl PoolCounter {
    pub(crate) fn begin(&mut self) -> StorageResult<()> {
        let attempts = self
            .attempts
            .checked_add(1)
            .ok_or(StorageError::Integrity("scratch pool counter exhausted"))?;
        // attempts already bounds every possible eventual success/failure count.
        let pending = self
            .pending
            .checked_add(1)
            .ok_or(StorageError::Integrity("scratch pool counter exhausted"))?;
        self.attempts = attempts;
        self.pending = pending;
        Ok(())
    }
    pub(crate) fn finish(&mut self, succeeded: bool) -> StorageResult<()> {
        if self.pending == 0 {
            return Err(StorageError::Integrity(
                "scratch pool counter acknowledgement",
            ));
        }
        let target = if succeeded {
            &mut self.succeeded
        } else {
            &mut self.failed
        };
        *target = target
            .checked_add(1)
            .ok_or(StorageError::Integrity("scratch pool counter exhausted"))?;
        self.pending -= 1;
        Ok(())
    }
}
/// Actual same-authority infrastructure events, independent of timer samples.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PoolCounters {
    /// Fresh selection and native file birth with captured descriptor identity.
    /// Later SQL initialization failure does not erase that actual native birth.
    pub fresh: PoolCounter,
    /// Matching-class complete header/scopes/native rebind acknowledgement.
    pub rebind: PoolCounter,
    /// Exact successful active-to-idle transfers.
    pub returns: PoolCounter,
    /// Explicit known-idle close/unlink attempts and removal acknowledgements.
    pub drain: PoolCounter,
    /// No matching idle/native slot could admit the requested exact class.
    pub class_refusals: u64,
}
/// Fixed current pool facts; idle bytes remain part of reserved native bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PoolStatus {
    /// Fixed existing operation-owner capacity.
    pub slots: usize,
    /// Explicit idle capacity, at most the existing default two slots.
    pub idle_limit: usize,
    /// Current known idle rows in the fixed slot table.
    pub idle: usize,
    /// Native S retained by those exact idle owners.
    pub idle_bytes: u64,
    /// Actual checked infrastructure counters.
    pub counters: PoolCounters,
}
pub(crate) struct Slots {
    pub(crate) entries: Vec<Slot>,
    pub(crate) counters: PoolCounters,
}
impl std::ops::Deref for Slots {
    type Target = [Slot];
    fn deref(&self) -> &[Slot] {
        &self.entries
    }
}
impl std::ops::DerefMut for Slots {
    fn deref_mut(&mut self) -> &mut [Slot] {
        &mut self.entries
    }
}
impl Slots {
    pub(crate) fn refusal(&mut self) -> StorageResult<()> {
        self.counters.class_refusals = self
            .counters
            .class_refusals
            .checked_add(1)
            .ok_or(StorageError::Integrity("scratch pool counter exhausted"))?;
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Action {
    Fresh,
    Rebind,
    Drain,
}
