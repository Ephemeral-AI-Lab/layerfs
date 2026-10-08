//! Fixed mount slots and per-mount FIFO, with one step per round-robin turn.
use super::{diagnostics::MountWork, task::Task, types::HANDOFFS, DispatchError};
use layerfs_overlay::NativeMount;
use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex, MutexGuard},
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
    pub tasks: Vec<Option<Slot>>,
    pub ready: VecDeque<usize>,
}
impl Lane {
    pub fn new(mount: NativeMount) -> Self {
        Self {
            mount,
            token: Arc::new(()),
            work: MountWork::default(),
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
}
pub(crate) struct Shared {
    pub state: Mutex<State>,
    pub changed: Condvar,
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
                state
            }
        }
    }
    pub fn wait<'a>(&self, guard: MutexGuard<'a, State>) -> MutexGuard<'a, State> {
        match self.changed.wait(guard) {
            Ok(state) => state,
            Err(poison) => {
                let mut state = poison.into_inner();
                state.failed = true;
                state.stopping = true;
                self.changed.notify_all();
                state
            }
        }
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
