//! Callback-entry R+N accounting; only this borrowed handoff wait may block.
use super::{
    queue::{Phase, Shared, Slot},
    task::Task,
    types::{Failure, HANDOFFS, MAX_INPUT_BYTES, RECEIVE_SLOTS},
    DispatchError, MountWork, RequestFuture,
};
use crate::ports::Fence;
use layerfs_overlay::NativeMount;
use std::{fmt, mem::size_of_val, sync::Arc, time::Instant};

#[derive(Clone)]
pub struct MountQueue {
    pub(crate) shared: Arc<Shared>,
    pub(crate) index: usize,
    pub(crate) mount: NativeMount,
    pub(crate) token: Arc<()>,
}
pub struct Received {
    mount: MountQueue,
    active: bool,
}
pub struct Permit {
    mount: MountQueue,
    slot: usize,
    active: bool,
}
/// The borrowed receive unit remains owned until its terminal reply/disposal.
pub struct AdmissionFailure {
    pub reason: DispatchError,
    received: Received,
}
impl fmt::Debug for AdmissionFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.reason.fmt(f)
    }
}
impl AdmissionFailure {
    pub fn mount(&self) -> NativeMount {
        self.received.mount.mount
    }
}
impl MountQueue {
    pub fn identity(&self) -> NativeMount {
        self.mount
    }
    /// Call at entry, before owned payload/name copies or any engine job. The
    /// configured two receive loops can hold at most two such stack guards.
    pub fn receive(&self) -> Result<Received, DispatchError> {
        let mut state = self.shared.lock();
        let lane = state.lane_mut(self.index, self.mount, &self.token)?;
        if lane.work.received == RECEIVE_SLOTS {
            return Err(DispatchError::Capacity);
        }
        lane.work.received += 1;
        lane.work.receive_units = lane.work.receive_units.saturating_add(1);
        Ok(Received {
            mount: self.clone(),
            active: true,
        })
    }
    pub fn work(&self) -> Result<MountWork, DispatchError> {
        Ok(self
            .shared
            .lock()
            .lane(self.index, self.mount, &self.token)?
            .observe())
    }
    /// Revokes new handoff and wakes every borrowed waiter. Already admitted
    /// work keeps running to its own original disposition; nothing is replayed.
    pub fn stop_admission(&self) -> Result<(), DispatchError> {
        self.shared
            .lock()
            .lane_mut(self.index, self.mount, &self.token)?
            .work
            .terminal = true;
        self.shared.changed.notify_all();
        Ok(())
    }
    /// This mount's terminal stop flag. A lane already released is terminal:
    /// its fence reads stopped and counts nothing.
    pub fn fence(&self) -> Fence {
        match self.shared.lock().lane(self.index, self.mount, &self.token) {
            Ok(lane) => lane.fence.clone(),
            Err(_) => {
                let fence = Fence::default();
                fence.stop();
                fence
            }
        }
    }
    /// The terminal fence of forced teardown: revokes new handoff, stops the
    /// fence and gives every parked request one more turn, so a request
    /// waiting before an attempt observes the stop and ends itself. A request
    /// waiting on a job it already submitted parks again on that job. Nothing
    /// is cancelled, dropped or replayed here.
    pub fn stop_service(&self) -> Result<(), DispatchError> {
        let mut state = self.shared.lock();
        let lane = state.lane_mut(self.index, self.mount, &self.token)?;
        lane.work.terminal = true;
        lane.fence.stop();
        for (slot, entry) in lane.tasks.iter_mut().enumerate() {
            let Some(entry) = entry else { continue };
            match entry.phase {
                Phase::Parked => {
                    entry.phase = Phase::Queued;
                    lane.ready.push_back(slot);
                }
                Phase::Running(ref mut notified) => *notified = true,
                Phase::Reserved | Phase::Queued | Phase::Retained => {}
            }
        }
        drop(state);
        self.shared.changed.notify_all();
        Ok(())
    }
    /// Observation wait for request drain. Returns once no unit is received,
    /// reserved, queued, running or parked, or at the caller's deadline. Retained
    /// failures remain admitted and are reported, never disposed here.
    pub fn wait_quiescent(&self, deadline: Instant) -> Result<MountWork, DispatchError> {
        let mut state = self.shared.lock();
        loop {
            let work = state.lane(self.index, self.mount, &self.token)?.observe();
            if (work.received == 0 && work.admitted == work.retained) || Instant::now() >= deadline
            {
                return Ok(work);
            }
            state = self.shared.wait_until(state, deadline);
        }
    }
    /// After actual connection detach/join and all consumer disposal, release
    /// the fixed mount slot. A nonzero received/admitted count refuses unchanged.
    pub fn finish(&self) -> Result<(), DispatchError> {
        let mut state = self.shared.lock();
        let lane = state.lane(self.index, self.mount, &self.token)?;
        if !lane.work.terminal || lane.work.received != 0 || lane.work.admitted != 0 {
            return Err(DispatchError::Busy);
        }
        state.lanes[self.index] = None;
        self.shared.changed.notify_all();
        Ok(())
    }
    /// Inspect retained original failure without removing its request/future.
    /// The inspector must be bounded and perform no provider I/O or waits.
    pub fn inspect_retained<T>(
        &self,
        slot: usize,
        inspect: impl FnOnce(super::types::FailureView<'_>) -> T,
    ) -> Result<T, DispatchError> {
        let task = {
            let state = self.shared.lock();
            let lane = state.lane(self.index, self.mount, &self.token)?;
            let entry = lane
                .tasks
                .get(slot)
                .and_then(Option::as_ref)
                .filter(|entry| entry.phase == Phase::Retained)
                .ok_or(DispatchError::Stale)?;
            entry.task.clone().ok_or(DispatchError::Stale)?
        };
        let failure = task
            .failure
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        match failure.as_ref().ok_or(DispatchError::Stale)? {
            Failure::Request(error) => {
                Ok(inspect(super::types::FailureView::Request(error.as_ref())))
            }
            Failure::Panic(payload) => {
                Ok(inspect(super::types::FailureView::Panic(payload.as_ref())))
            }
        }
    }
}
impl Received {
    /// input_bytes is the bounded copy capacity known from the borrowed frame.
    /// Waits hold only this receive reservation and release the scheduler mutex.
    pub fn admit(mut self, input_bytes: usize) -> Result<Permit, AdmissionFailure> {
        if input_bytes > MAX_INPUT_BYTES {
            return Err(AdmissionFailure {
                reason: DispatchError::Capacity,
                received: self,
            });
        }
        let shared = self.mount.shared.clone();
        let mut state = shared.lock();
        loop {
            let stop = state.stopping || state.failed;
            let lane = match state.lane_mut(self.mount.index, self.mount.mount, &self.mount.token) {
                Ok(lane) => lane,
                Err(reason) => {
                    drop(state);
                    return Err(AdmissionFailure {
                        reason,
                        received: self,
                    });
                }
            };
            if stop || lane.work.terminal {
                drop(state);
                return Err(AdmissionFailure {
                    reason: DispatchError::Stopped,
                    received: self,
                });
            }
            if lane.work.admitted < HANDOFFS {
                let slot = lane
                    .tasks
                    .iter()
                    .position(Option::is_none)
                    .expect("native handoff slot");
                lane.tasks[slot] = Some(Slot {
                    task: None,
                    phase: Phase::Reserved,
                    input: input_bytes,
                    future: 0,
                });
                lane.work.received -= 1;
                lane.work.admitted += 1;
                self.active = false;
                return Ok(Permit {
                    mount: self.mount.clone(),
                    slot,
                    active: true,
                });
            }
            state = shared.wait(state);
        }
    }
}
impl Drop for Received {
    fn drop(&mut self) {
        if self.active {
            if let Ok(lane) = self.mount.shared.lock().lane_mut(
                self.mount.index,
                self.mount.mount,
                &self.mount.token,
            ) {
                lane.work.received -= 1;
            }
            self.mount.shared.changed.notify_all();
        }
    }
}
impl Permit {
    /// Ownership transfers even when stopping races handoff: that original
    /// future is then retained in its slot, never dropped as an unowned reply.
    pub fn handoff(mut self, future: RequestFuture) -> Result<(), DispatchError> {
        let bytes = size_of_val(future.as_ref().get_ref());
        let task = Task::new(
            Arc::downgrade(&self.mount.shared),
            self.mount.mount,
            self.mount.index,
            self.slot,
            self.mount.token.clone(),
            future,
        );
        let mut state = self.mount.shared.lock();
        let stopping = state.stopping || state.failed;
        let lane = state.lane_mut(self.mount.index, self.mount.mount, &self.mount.token)?;
        let entry = lane.tasks[self.slot].as_mut().ok_or(DispatchError::Stale)?;
        entry.task = Some(task.clone());
        entry.future = bytes;
        self.active = false;
        if stopping {
            *task
                .failure
                .lock()
                .unwrap_or_else(|poison| poison.into_inner()) =
                Some(Failure::Request(Box::new(DispatchError::Stopped)));
            entry.phase = Phase::Retained;
            lane.work.terminal = true;
            self.mount.shared.changed.notify_all();
            Err(DispatchError::Stopped)
        } else {
            entry.phase = Phase::Queued;
            lane.ready.push_back(self.slot);
            self.mount.shared.changed.notify_all();
            Ok(())
        }
    }
}
impl Drop for Permit {
    fn drop(&mut self) {
        if self.active {
            if let Ok(lane) = self.mount.shared.lock().lane_mut(
                self.mount.index,
                self.mount.mount,
                &self.mount.token,
            ) {
                lane.tasks[self.slot] = None;
                lane.work.admitted -= 1;
            }
            self.mount.shared.changed.notify_all();
        }
    }
}
