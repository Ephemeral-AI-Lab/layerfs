//! Fixed mount slots and per-mount FIFO, with one step per round-robin turn.
use super::{diagnostics::MountWork, task::Task, types::HANDOFFS, DispatchError};
use crate::ports::Fence;
use layerfs_overlay::NativeMount;
use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex, MutexGuard},
    time::Instant,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Phase {
    Reserved,
    Queued,
    Running(bool),
    Parked,
    Retained,
}
pub(crate) struct Slot {
    pub task: Option<Arc<Task>>,
    pub phase: Phase,
    pub input: usize,
    pub future: usize,
}
pub(crate) struct Lane {
    pub mount: NativeMount,
    pub token: Arc<()>,
    pub work: MountWork,
    /// Stopped only by forced teardown; `work.terminal` alone is not a stop.
    pub fence: Fence,
    pub tasks: Vec<Option<Slot>>,
    pub ready: VecDeque<usize>,
}
impl Lane {
    pub fn new(mount: NativeMount) -> Self {
        Self {
            mount,
            token: Arc::new(()),
            work: MountWork::default(),
            fence: Fence::default(),
            tasks: (0..HANDOFFS).map(|_| None).collect(),
            ready: VecDeque::with_capacity(HANDOFFS),
        }
    }
    pub fn observe(&self) -> MountWork {
        let mut work = self.work;
        for slot in self.tasks.iter().flatten() {
            work.owned_input_bytes += slot.input;
            work.future_bytes += slot.future;
            match slot.phase {
                Phase::Queued => work.queued += 1,
                Phase::Running(_) => work.running += 1,
                Phase::Parked => work.parked += 1,
                Phase::Retained => work.retained += 1,
                Phase::Reserved => {}
            }
        }
        work
    }
}
pub(crate) struct State {
    pub lanes: Vec<Option<Lane>>,
    pub next: usize,
    pub stopping: bool,
    pub failed: bool,
    pub entered: usize,
    pub live: usize,
    /// Workers waiting for a runnable step.
    pub idle: usize,
    /// Receive, start and drain observers waiting for a state change.
    pub observers: usize,
}
pub(crate) struct Shared {
    pub state: Mutex<State>,
    /// State changes for observers: a freed slot, a returned receive unit, a
    /// terminal lane, a worker's entry or exit.
    pub changed: Condvar,
    /// Runnable steps, for workers only.
    pub runnable: Condvar,
    pub workers: usize,
}
impl Shared {
    pub fn lock(&self) -> MutexGuard<'_, State> {
        match self.state.lock() {
            Ok(state) => state,
            Err(poison) => {
                let mut state = poison.into_inner();
                state.failed = true;
                state.stopping = true;
                for lane in state.lanes.iter_mut().flatten() {
                    lane.work.terminal = true;
                }
                self.changed.notify_all();
                self.runnable.notify_all();
                state
            }
        }
    }
    fn poisoned<'a>(&self, mut state: MutexGuard<'a, State>) -> MutexGuard<'a, State> {
        state.failed = true;
        state.stopping = true;
        self.changed.notify_all();
        self.runnable.notify_all();
        state
    }
    /// An observer's wait for the next announced change.
    pub fn wait<'a>(&self, mut guard: MutexGuard<'a, State>) -> MutexGuard<'a, State> {
        guard.observers += 1;
        let mut state = match self.changed.wait(guard) {
            Ok(state) => state,
            Err(poison) => self.poisoned(poison.into_inner()),
        };
        state.observers -= 1;
        state
    }
    /// A worker's wait for a runnable step or a stop.
    pub fn wait_runnable<'a>(&self, mut guard: MutexGuard<'a, State>) -> MutexGuard<'a, State> {
        guard.idle += 1;
        let mut state = match self.runnable.wait(guard) {
            Ok(state) => state,
            Err(poison) => self.poisoned(poison.into_inner()),
        };
        state.idle -= 1;
        state
    }
    /// Tells observers of a change they can wait for. Called with the lock.
    pub fn announce(&self, state: &State) {
        if state.observers != 0 {
            self.changed.notify_all();
        }
    }
    /// One runnable step was queued. Called with the lock.
    pub fn wake_worker(&self, state: &State) {
        if state.idle != 0 {
            self.runnable.notify_one();
        }
    }
    /// A stop or a terminal fence: every waiter looks again.
    pub fn wake_all(&self) {
        self.changed.notify_all();
        self.runnable.notify_all();
    }
    /// One bounded observation wait; expiry changes no request or lane state.
    pub fn wait_until<'a>(
        &self,
        mut guard: MutexGuard<'a, State>,
        deadline: Instant,
    ) -> MutexGuard<'a, State> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        guard.observers += 1;
        let mut state = match self.changed.wait_timeout(guard, remaining) {
            Ok((state, _)) => state,
            Err(poison) => self.poisoned(poison.into_inner().0),
        };
        state.observers -= 1;
        state
    }
}
impl State {
    pub fn lane(
        &self,
        index: usize,
        mount: NativeMount,
        token: &Arc<()>,
    ) -> Result<&Lane, DispatchError> {
        self.lanes
            .get(index)
            .and_then(Option::as_ref)
            .filter(|lane| lane.mount == mount && Arc::ptr_eq(&lane.token, token))
            .ok_or(DispatchError::Stale)
    }
    pub fn lane_mut(
        &mut self,
        index: usize,
        mount: NativeMount,
        token: &Arc<()>,
    ) -> Result<&mut Lane, DispatchError> {
        self.lanes
            .get_mut(index)
            .and_then(Option::as_mut)
            .filter(|lane| lane.mount == mount && Arc::ptr_eq(&lane.token, token))
            .ok_or(DispatchError::Stale)
    }
    /// Steps queued across every lane.
    pub fn runnable(&self) -> usize {
        self.lanes
            .iter()
            .flatten()
            .map(|lane| lane.ready.len())
            .sum()
    }
    pub fn take(&mut self) -> Option<Arc<Task>> {
        let length = self.lanes.len();
        for offset in 0..length {
            let index = (self.next + offset) % length;
            let Some(lane) = self.lanes[index].as_mut() else {
                continue;
            };
            let Some(slot) = lane.ready.pop_front() else {
                continue;
            };
            let entry = lane.tasks[slot].as_mut().expect("queued native slot");
            entry.phase = Phase::Running(false);
            lane.work.steps = lane.work.steps.saturating_add(1);
            self.next = (index + 1) % length;
            return entry.task.clone();
        }
        None
    }
}
