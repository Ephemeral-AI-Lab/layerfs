//! Race-safe runnable notifications and original continuation retention.
use super::{
    queue::{Phase, Shared, State},
    types::Failure,
    RequestDisposition, RequestFuture,
};
use layerfs_overlay::NativeMount;
use std::{
    cell::Cell,
    future::Future,
    panic::{catch_unwind, AssertUnwindSafe},
    pin::Pin,
    sync::{Arc, Mutex, Weak},
    task::{Context, Poll, Wake, Waker},
};

pub(crate) struct Task {
    pub shared: Weak<Shared>,
    pub mount: NativeMount,
    pub lane: usize,
    pub slot: usize,
    pub token: Arc<()>,
    pub future: Mutex<Option<RequestFuture>>,
    pub failure: Mutex<Option<Failure>>,
    waker: Waker,
}
/// Notification registration is not independent request custody. In particular,
/// Task -> Pending -> registered Waker must not be a strong ownership cycle.
struct Notification(Weak<Task>);
impl Wake for Notification {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        if let Some(task) = self.0.upgrade() {
            task.notify();
        }
    }
}
impl Task {
    pub fn new(
        shared: Weak<Shared>,
        mount: NativeMount,
        lane: usize,
        slot: usize,
        token: Arc<()>,
        future: RequestFuture,
    ) -> Arc<Self> {
        Arc::new_cyclic(|task| Self {
            shared,
            mount,
            lane,
            slot,
            token,
            future: Mutex::new(Some(future)),
            failure: Mutex::new(None),
            waker: Waker::from(Arc::new(Notification(task.clone()))),
        })
    }
    fn notify(self: &Arc<Self>) {
        let Some(shared) = self.shared.upgrade() else {
            return;
        };
        let mut state = shared.lock();
        if state.stopping {
            return;
        }
        let Ok(lane) = state.lane_mut(self.lane, self.mount, &self.token) else {
            return;
        };
        let Some(entry) = lane.tasks[self.slot].as_mut() else {
            return;
        };
        if !entry
            .task
            .as_ref()
            .is_some_and(|task| Arc::ptr_eq(task, self))
        {
            return;
        }
        lane.work.wakes = lane.work.wakes.saturating_add(1);
        match entry.phase {
            Phase::Parked => {
                entry.phase = Phase::Queued;
                lane.ready.push_back(self.slot);
                shared.wake_worker(&state);
            }
            Phase::Running(ref mut notified) => *notified = true,
            Phase::Reserved | Phase::Queued | Phase::Retained => {}
        }
    }
}
/// Restores the original future if the worker unwinds outside its poll boundary.
struct Running {
    task: Arc<Task>,
    future: Option<RequestFuture>,
}
impl Drop for Running {
    fn drop(&mut self) {
        if let Some(future) = self.future.take() {
            *self
                .task
                .future
                .lock()
                .unwrap_or_else(|poison| poison.into_inner()) = Some(future);
        }
    }
}
thread_local! {
    /// Set while this thread runs a request's first step from its receive
    /// callback, so that step can leave the loop before provider I/O.
    static RECEIVING: Cell<bool> = const { Cell::new(false) };
}
struct Receiving(bool);
impl Receiving {
    fn enter(receiving: bool) -> Self {
        Self(RECEIVING.replace(receiving))
    }
}
impl Drop for Receiving {
    fn drop(&mut self) {
        RECEIVING.set(self.0);
    }
}
/// A first step running on its receive thread. Shutdown waits for it before
/// it counts the requests it retains.
struct FirstStep<'a>(Option<&'a Shared>);
impl FirstStep<'_> {
    /// Ends the step under the lock its last state change already holds.
    fn end(&mut self, state: &mut State) {
        if let Some(shared) = self.0.take() {
            state.receiving -= 1;
            shared.announce(state);
        }
    }
}
impl Drop for FirstStep<'_> {
    fn drop(&mut self) {
        if let Some(shared) = self.0.take() {
            let mut state = shared.lock();
            state.receiving -= 1;
            shared.announce(&state);
        }
    }
}
/// One bounded step of a request. `receiving` is the first step, taken on
/// the thread that received the request; every later step runs on a worker.
pub(crate) fn advance(task: Arc<Task>, shared: &Shared, receiving: bool) {
    let mut first = FirstStep(receiving.then_some(shared));
    let future = task
        .future
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .take()
        .expect("native original future");
    let mut running = Running {
        task: task.clone(),
        future: Some(future),
    };
    let waker = task.waker.clone();
    // No scheduler, future, provider or registry lock spans the user's step.
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let _receiving = Receiving::enter(receiving);
        running
            .future
            .as_mut()
            .unwrap()
            .as_mut()
            .poll(&mut Context::from_waker(&waker))
    }));
    let failure = match outcome {
        Ok(Poll::Ready(RequestDisposition::Complete)) => None,
        Ok(Poll::Ready(RequestDisposition::Retained(error))) => Some(Failure::Request(error)),
        Err(payload) => Some(Failure::Panic(payload)),
        Ok(Poll::Pending) => {
            drop(running);
            let mut state = shared.lock();
            let lane = state
                .lane_mut(task.lane, task.mount, &task.token)
                .expect("owned native lane");
            let entry = lane.tasks[task.slot].as_mut().expect("owned native slot");
            if matches!(entry.phase, Phase::Running(true)) {
                entry.phase = Phase::Queued;
                lane.ready.push_back(task.slot);
                // A worker takes its own requeued step on its next turn; it
                // wakes another only when more than that one step is queued.
                if receiving || state.runnable() > 1 {
                    shared.wake_worker(&state);
                }
            } else {
                entry.phase = Phase::Parked;
            }
            first.end(&mut state);
            return;
        }
    };
    if let Some(failure) = failure {
        *task
            .failure
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = Some(failure);
        drop(running);
        let mut state = shared.lock();
        let lane = state
            .lane_mut(task.lane, task.mount, &task.token)
            .expect("retained native lane");
        lane.tasks[task.slot]
            .as_mut()
            .expect("retained native slot")
            .phase = Phase::Retained;
        lane.work.terminal = true;
        first.end(&mut state);
        shared.announce(&state);
        return;
    }
    // Dispose the original continuation before releasing its handoff credit.
    let completed = running.future.take().unwrap();
    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| drop(completed))) {
        *task
            .failure
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = Some(Failure::Panic(payload));
        let mut state = shared.lock();
        let lane = state
            .lane_mut(task.lane, task.mount, &task.token)
            .expect("retained native lane");
        lane.tasks[task.slot].as_mut().unwrap().phase = Phase::Retained;
        lane.work.terminal = true;
        first.end(&mut state);
        shared.announce(&state);
        return;
    }
    let mut state = shared.lock();
    let lane = state
        .lane_mut(task.lane, task.mount, &task.token)
        .expect("completed native lane");
    lane.tasks[task.slot] = None;
    lane.work.admitted -= 1;
    lane.work.completed = lane.work.completed.saturating_add(1);
    first.end(&mut state);
    shared.announce(&state);
}

/// A completed bounded window yields one runnable turn even when every awaited
/// prerequisite was already ready. This never polls an unfinished operation.
#[derive(Default)]
pub struct NextTurn(bool);
impl Future for NextTurn {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}
/// Ready at once on a pool worker. On the thread that received the request
/// it yields one turn, so the rest of the request continues on a worker and
/// the receive loop returns to the kernel before any provider I/O.
#[derive(Default)]
pub struct LeaveReceiver(bool);
impl Future for LeaveReceiver {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 || !RECEIVING.get() {
            Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}
