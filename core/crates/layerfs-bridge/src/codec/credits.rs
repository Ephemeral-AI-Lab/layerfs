//! Aggregate credit follows a received message across worker/caller ownership.
use super::{ReassemblyConfig, ReassemblyWork};
use crate::contract::{FrameError, FrameResult, MessageClass};
use std::{
    collections::BTreeSet,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};
pub(super) struct Ledger {
    pub config: ReassemblyConfig,
    pub work: ReassemblyWork,
    ids: BTreeSet<(u64, u64)>,
}
impl Ledger {
    pub fn new(config: ReassemblyConfig) -> Self {
        Self {
            config,
            work: Default::default(),
            ids: BTreeSet::new(),
        }
    }
}
/// Shared aggregate receive admission across authenticated channel owners.
#[derive(Clone)]
pub struct ReceiveBudget {
    pub(super) config: ReassemblyConfig,
    pub(super) ledger: Arc<Mutex<Ledger>>,
    next_owner: Arc<AtomicU64>,
}
impl ReceiveBudget {
    /// Selects one host/client aggregate before channel body allocation.
    pub fn new(config: ReassemblyConfig) -> FrameResult<Self> {
        let reserves = config
            .demand_reserve
            .checked_add(config.control_reserve)
            .ok_or(FrameError::Invalid("receive reserves"))?;
        let message_reserves = config
            .demand_messages
            .checked_add(config.control_messages)
            .ok_or(FrameError::Invalid("receive message reserves"))?;
        if config.messages <= message_reserves
            || config.demand_reserve == 0
            || config.demand_messages == 0
            || config.control_messages == 0
            || config.message_bytes == 0
            || config.bytes <= reserves
            || config.control_reserve < std::mem::size_of::<super::Message>()
        {
            return Err(FrameError::Invalid("receive windows"));
        }
        Ok(Self {
            config,
            ledger: Arc::new(Mutex::new(Ledger::new(config))),
            next_owner: Arc::new(AtomicU64::new(1)),
        })
    }
    pub(super) fn owner(&self) -> FrameResult<u64> {
        self.next_owner
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .map_err(|_| FrameError::IdentityExhausted)
    }
    /// Aggregate original copies and live caller-held credit across all receivers.
    pub fn work(&self) -> FrameResult<ReassemblyWork> {
        Ok(self.ledger.lock().map_err(|_| FrameError::Poisoned)?.work)
    }
    /// Selected aggregate windows, never inferred from a single receiver's allowance.
    pub const fn config(&self) -> ReassemblyConfig {
        self.config
    }
}

pub(super) struct Credit {
    pub ledger: Arc<Mutex<Ledger>>,
    id: (u64, u64),
    bytes: usize,
    class: MessageClass,
}
impl Credit {
    pub fn acquire(
        ledger: &Arc<Mutex<Ledger>>,
        id: (u64, u64),
        class: MessageClass,
        bytes: usize,
    ) -> FrameResult<Self> {
        let mut state = ledger.lock().map_err(|_| FrameError::Poisoned)?;
        if state.ids.contains(&id) {
            return Err(FrameError::Invalid("retained message identity"));
        }
        let limit = limit(state.config, class);
        let jobs = match class {
            MessageClass::Save => {
                state.config.messages - state.config.demand_messages - state.config.control_messages
            }
            MessageClass::Demand => state.config.messages - state.config.control_messages,
            MessageClass::Control => state.config.messages,
        };
        if bytes > limit
            || state.work.credited_bytes > limit - bytes
            || state.work.live_messages >= jobs
        {
            state.work.refused = state.work.refused.saturating_add(1);
            return Err(FrameError::AdmissionUnavailable);
        }
        state.ids.insert(id);
        state.work.live_messages += 1;
        state.work.admitted = state.work.admitted.saturating_add(1);
        state.work.credited_bytes += bytes;
        state.work.peak_credited_bytes = state
            .work
            .peak_credited_bytes
            .max(state.work.credited_bytes);
        drop(state);
        Ok(Self {
            ledger: ledger.clone(),
            id,
            bytes,
            class,
        })
    }
    pub fn add_capacity(&mut self, extra: usize) -> FrameResult<()> {
        let mut state = self.ledger.lock().map_err(|_| FrameError::Poisoned)?;
        let limit = limit(state.config, self.class);
        if extra > limit || state.work.credited_bytes > limit - extra {
            state.work.refused = state.work.refused.saturating_add(1);
            return Err(FrameError::AdmissionUnavailable);
        }
        self.bytes = self
            .bytes
            .checked_add(extra)
            .ok_or(FrameError::Invalid("credit overflow"))?;
        state.work.credited_bytes += extra;
        state.work.peak_credited_bytes = state
            .work
            .peak_credited_bytes
            .max(state.work.credited_bytes);
        Ok(())
    }
}
fn limit(config: ReassemblyConfig, class: MessageClass) -> usize {
    match class {
        MessageClass::Save => config.bytes - config.demand_reserve - config.control_reserve,
        MessageClass::Demand => config.bytes - config.control_reserve,
        MessageClass::Control => config.bytes,
    }
}
impl Drop for Credit {
    fn drop(&mut self) {
        // Poison makes subsequent observations unavailable; do not repair the
        // mutex or manufacture a zero/valid receipt during unwinding.
        if let Ok(mut state) = self.ledger.lock() {
            state.ids.remove(&self.id);
            state.work.live_messages -= 1;
            state.work.credited_bytes -= self.bytes;
        }
    }
}
