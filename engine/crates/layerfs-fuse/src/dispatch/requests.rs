//! Per-mount request drain and failure custody. Receive workers run requests directly.
use super::{DispatchError, FailureView, MountWork, RequestDisposition, RequestFuture};
use crate::ports::Fence;
use layerfs_overlay::KernelMount;
use std::{
    any::Any,
    fmt,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{Arc, Condvar, Mutex},
    task::{Context, Poll, Wake, Waker},
    thread,
    time::Instant,
};
enum Failure {
    Request(Box<dyn std::error::Error + Send + Sync>),
    Panic {
        payload: Box<dyn Any + Send>,
        _future: RequestFuture,
    },
}
struct State {
    work: MountWork,
    failures: Vec<Failure>,
    threads: Vec<thread::Thread>,
    finished: bool,
}
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
    fence: Fence,
}
#[derive(Clone)]
pub struct MountRequests {
    shared: Arc<Shared>,
    mount: KernelMount,
}
pub struct Received {
    mount: MountRequests,
    active: bool,
}
pub struct RequestOwner {
    received: Received,
}
pub struct InputFailure {
    pub reason: DispatchError,
    _received: Received,
}
impl fmt::Debug for InputFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.reason.fmt(f)
    }
}
struct Current(thread::Thread);
impl Wake for Current {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}
impl MountRequests {
    pub fn new(mount: KernelMount) -> Self {
        Self {
            mount,
            shared: Arc::new(Shared {
                state: Mutex::new(State {
                    work: MountWork::default(),
                    failures: Vec::new(),
                    threads: Vec::new(),
                    finished: false,
                }),
                changed: Condvar::new(),
                fence: Fence::default(),
            }),
        }
    }
    pub fn identity(&self) -> KernelMount {
        self.mount
    }
    pub fn receive(&self) -> Result<Received, DispatchError> {
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| DispatchError::Failed)?;
        if state.finished {
            return Err(DispatchError::Stale);
        }
        if state.work.terminal {
            return Err(DispatchError::Stopped);
        }
        state.threads.push(thread::current());
        state.work.active += 1;
        state.work.requests = state.work.requests.saturating_add(1);
        Ok(Received {
            mount: self.clone(),
            active: true,
        })
    }
    pub fn work(&self) -> Result<MountWork, DispatchError> {
        self.shared
            .state
            .lock()
            .map(|state| state.work)
            .map_err(|_| DispatchError::Failed)
    }
    pub fn fence(&self) -> Fence {
        self.shared.fence.clone()
    }
    pub fn stop_receive(&self) -> Result<(), DispatchError> {
        self.shared
            .state
            .lock()
            .map_err(|_| DispatchError::Failed)?
            .work
            .terminal = true;
        self.shared.changed.notify_all();
        Ok(())
    }
    pub fn stop_service(&self) -> Result<(), DispatchError> {
        self.stop_receive()?;
        self.shared.fence.stop();
        let threads = self
            .shared
            .state
            .lock()
            .map_err(|_| DispatchError::Failed)?
            .threads
            .clone();
        for thread in threads {
            thread.unpark();
        }
        Ok(())
    }
    pub fn wait_quiescent(&self, deadline: Instant) -> Result<MountWork, DispatchError> {
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| DispatchError::Failed)?;
        while state.work.active != 0 && Instant::now() < deadline {
            state = self
                .shared
                .changed
                .wait_timeout(state, deadline.saturating_duration_since(Instant::now()))
                .map_err(|_| DispatchError::Failed)?
                .0;
        }
        Ok(state.work)
    }
    pub fn finish(&self) -> Result<(), DispatchError> {
        let mut state = self
            .shared
            .state
            .lock()
            .map_err(|_| DispatchError::Failed)?;
        if !state.work.terminal || state.work.active != 0 || state.work.retained != 0 {
            return Err(DispatchError::Busy);
        }
        state.finished = true;
        Ok(())
    }
    pub fn inspect_failure<T>(
        &self,
        slot: usize,
        inspect: impl FnOnce(FailureView<'_>) -> T,
    ) -> Result<T, DispatchError> {
        let state = self
            .shared
            .state
            .lock()
            .map_err(|_| DispatchError::Failed)?;
        match state.failures.get(slot).ok_or(DispatchError::Stale)? {
            Failure::Request(error) => Ok(inspect(FailureView::Request(error.as_ref()))),
            Failure::Panic { payload, .. } => Ok(inspect(FailureView::Panic(payload.as_ref()))),
        }
    }
}
impl Received {
    pub fn request(self, input_bytes: usize) -> Result<RequestOwner, InputFailure> {
        if input_bytes > super::MAX_INPUT_BYTES {
            Err(InputFailure {
                reason: DispatchError::Capacity,
                _received: self,
            })
        } else {
            Ok(RequestOwner { received: self })
        }
    }
}
impl Drop for Received {
    fn drop(&mut self) {
        if self.active {
            let mut state = self
                .mount
                .shared
                .state
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            state.work.active -= 1;
            let current = thread::current().id();
            if let Some(index) = state
                .threads
                .iter()
                .position(|thread| thread.id() == current)
            {
                state.threads.swap_remove(index);
            }
            drop(state);
            self.mount.shared.changed.notify_all();
        }
    }
}
impl RequestOwner {
    pub fn serve(self, mut future: RequestFuture) -> Result<(), DispatchError> {
        let waker = Waker::from(Arc::new(Current(thread::current())));
        let mut context = Context::from_waker(&waker);
        let result = catch_unwind(AssertUnwindSafe(|| loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(result) => break result,
                Poll::Pending => thread::park(),
            }
        }));
        let failure = match result {
            Ok(RequestDisposition::Complete) => None,
            Ok(RequestDisposition::Retained(error)) => Some(Failure::Request(error)),
            Err(payload) => Some(Failure::Panic {
                payload,
                _future: future,
            }),
        };
        let shared = &self.received.mount.shared;
        let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(failure) = failure {
            // The mount is terminal: later receives are refused. A request that
            // is already running keeps its own original disposition and can add
            // at most its own failure, retained exactly and never replayed.
            state.failures.push(failure);
            state.work.retained += 1;
            state.work.terminal = true;
        } else {
            state.work.completed = state.work.completed.saturating_add(1);
        }
        Ok(())
    }
}
