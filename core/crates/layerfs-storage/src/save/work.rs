//! Fixed-size recent save observations. Inclusive stages are never additive.
use std::{cell::Cell, time::Instant};
/// One stage's invocation count and inclusive owner-thread wall.
#[derive(Clone, Copy, Debug, Default)]
pub struct StageWork {
    /// Stage invocations, including failed attempts.
    pub calls: u64,
    /// Inclusive wall time; nested stages overlap.
    pub wall_ns: u64,
}
/// Observed save work: begin, membership, admission, pack flush, registration
/// preparation, metadata registration, pack reservation,
/// ordinal reservation and finish pack closure, in that order.
#[derive(Clone, Copy, Debug, Default)]
pub struct SaveWork {
    /// Fixed nine-stage aggregate.
    pub stages: [StageWork; 9],
}
impl SaveWork {
    fn difference(self, earlier: Self) -> Self {
        let mut out = Self::default();
        for (index, row) in out.stages.iter_mut().enumerate() {
            row.calls = self.stages[index].calls - earlier.stages[index].calls;
            row.wall_ns = self.stages[index].wall_ns - earlier.stages[index].wall_ns;
        }
        out
    }
}
/// Cumulative stage work and the four most recently acknowledged saves.
#[derive(Clone, Copy, Debug, Default)]
pub struct SaveHistory {
    /// All attempted stage work for this Storage handle.
    pub total: SaveWork,
    /// Successful acknowledged save count.
    pub completed: u64,
    /// Recent successful saves in a ring, index `(save_number - 1) % 4`.
    pub recent: [SaveWork; 4],
}
#[derive(Clone, Copy)]
pub(crate) enum Stage {
    Begin,
    Membership,
    Admission,
    Flush,
    Publication,
    Metadata,
    PackReserve,
    OrdinalReserve,
    FinishClose,
}
#[derive(Default)]
pub(crate) struct Work(Cell<SaveHistory>);
pub(crate) struct Span<'a> {
    work: &'a Work,
    stage: Stage,
    start: Instant,
}
impl Work {
    pub(crate) fn span(&self, stage: Stage) -> Span<'_> {
        Span {
            work: self,
            stage,
            start: Instant::now(),
        }
    }
    pub(crate) fn snapshot(&self) -> SaveHistory {
        self.0.get()
    }
    pub(crate) fn close(&self, start: SaveWork) {
        let mut value = self.0.get();
        let index = (value.completed % 4) as usize;
        value.recent[index] = value.total.difference(start);
        value.completed += 1;
        self.0.set(value);
    }
}
impl Drop for Span<'_> {
    fn drop(&mut self) {
        let elapsed = self.start.elapsed().as_nanos() as u64;
        let mut value = self.work.0.get();
        let row = &mut value.total.stages[self.stage as usize];
        row.calls += 1;
        row.wall_ns += elapsed;
        self.work.0.set(value);
    }
}
