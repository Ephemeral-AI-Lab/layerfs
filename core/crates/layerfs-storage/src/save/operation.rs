//! Save lifetime, one producer and explicit terminal failures.
use super::{state::State, WriteOutcome};
use crate::{
    error::{StorageError, StorageResult},
    pack::layout::PackLane,
    storage::Storage,
};
use layerfs_content::{ContentError, ContentResult, FinalizedConsumer, FinalizedObject};
use std::cell::{Cell, RefCell};

/// One bounded producer. Published reference-closed batches survive later failure.
pub struct Save<'a> {
    pub(super) state: RefCell<State<'a>>,
    pub(super) terminal: Cell<bool>,
    failure: RefCell<Option<StorageError>>,
    finished: Cell<bool>,
    work_start: super::SaveWork,
}
impl<'a> Save<'a> {
    pub(crate) fn new(storage: &'a Storage) -> StorageResult<Self> {
        let work_start = storage.work.snapshot().total;
        let _work = storage.work.span(super::Stage::Begin);
        Ok(Self {
            work_start,
            state: RefCell::new(State::new(storage)?),
            terminal: Cell::new(false),
            failure: RefCell::new(None),
            finished: Cell::new(false),
        })
    }
    /// Accepts a finalized object, preparing the preceding bounded wave as needed.
    pub fn accept(&self, object: FinalizedObject) -> StorageResult<()> {
        if self.terminal.get() {
            return Err(StorageError::Aborted);
        }
        let result = (|| {
            let mut state = self.state.borrow_mut();
            if let Some(objects) = state.pending.push(object)? {
                state.wave(objects)?;
            }
            Ok(())
        })();
        if result.is_err() {
            self.terminal.set(true);
        }
        result
    }
    /// C1 handoff consumer sharing this save's same-operation provider.
    pub fn sink(&self) -> SaveSink<'_, 'a> {
        SaveSink { save: self }
    }
    /// Seals and registers the remaining reference-closed output once.
    pub fn finish(self) -> StorageResult<WriteOutcome> {
        if let Some(error) = self.failure.borrow_mut().take() {
            return Err(error);
        }
        if self.terminal.get() {
            return Err(StorageError::Aborted);
        }
        let mut state = self.state.borrow_mut();
        let objects = state.pending.drain();
        state.wave(objects)?;
        let closing = state.storage.work.span(super::Stage::FinishClose);
        state.finishing = true;
        if state.packer.unfinished() {
            let required = state.packer.finish_pack_bound();
            state.reserve_packs(required, required)?;
        }
        for lane in PackLane::ALL {
            let State {
                packer,
                compression,
                next_pack,
                pack_end,
                ..
            } = &mut *state;
            packer.seal(lane, compression, next_pack, *pack_end)?;
        }
        {
            let State {
                packer,
                next_pack,
                pack_end,
                ..
            } = &mut *state;
            packer.flush(next_pack, *pack_end)?;
        }
        state.packer.seal_pool()?;
        state.flush_signatures()?;
        drop(closing);
        state.register_ready()?;
        state.outcome.pool = state.pool_stats;
        state.profile.group_ns += state.packer.group_ns;
        state.storage.work.close(self.work_start, state.profile);
        self.finished.set(true);
        Ok(state.outcome)
    }
    /// Takes the original error retained by a C1 sink before abandoning a save.
    pub fn take_failure(&self) -> Option<StorageError> {
        self.failure.borrow_mut().take()
    }
    /// Actual delta-selection outcomes for this producer.
    pub fn delta_counters(&self) -> crate::encoding::delta::select::DeltaCounters {
        self.state.borrow().delta
    }
    /// Actual dependency reconstruction work for this producer.
    pub fn chain_counters(&self) -> crate::encoding::delta::read::ChainCounters {
        self.state.borrow().chain_total
    }
    /// Actual pooled value reuse, allocation and selection outcomes.
    pub fn pool_counters(&self) -> crate::save::PoolCounters {
        self.state.borrow().pool_stats
    }
    /// Bounded candidate index's current charged bytes.
    pub fn pooled_index_bytes(&self) -> usize {
        self.state.borrow().pool_index.live_bytes()
    }
    /// Canonical bytes waiting in the bounded producer batch.
    pub fn pending_canonical_bytes(&self) -> u64 {
        self.state.borrow().pending.canonical_bytes()
    }
}
impl Drop for Save<'_> {
    fn drop(&mut self) {
        if !self.finished.get() {
            let state = self.state.get_mut();
            state.candidates.invalidate();
            state.pool_index.invalidate();
            state.storage.source.refresh_window();
            state.storage.source.invalidate_signatures();
        }
    }
}
/// C1's consumer adapter; the original typed storage error is retained for finish.
pub struct SaveSink<'s, 'a> {
    save: &'s Save<'a>,
}
impl FinalizedConsumer for SaveSink<'_, '_> {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.save.accept(object).map_err(|error| {
            let mut failure = self.save.failure.borrow_mut();
            if failure.is_none() {
                *failure = Some(error);
            }
            ContentError::OutputRejected
        })
    }
}
