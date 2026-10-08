//! Fair event-driven admission to the Store's fixed opened read sessions.
use super::{
    open::Counts,
    read_handle::{ReadLease, StoreReader},
    read_state::{Disposition, State},
    PortError,
};
use layerfs_history::WorkspaceId;
use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard},
    task::{Context, Poll, Wake, Waker},
    thread::{self, Thread},
    time::Instant,
};

/// Fixed concurrent scheduling windows, not limits on filesystem populations.
#[derive(Clone, Copy, Debug)]
pub struct ReadLimits {
    pub namespaces: usize,
    pub requests_per_namespace: usize,
}
impl Default for ReadLimits {
    fn default() -> Self {
        Self {
            namespaces: 17,
            requests_per_namespace: 18,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadAdmissionError {
    Capacity,
    Stopped,
    NoReaders,
    Poisoned,
    InvalidLimits,
}
impl fmt::Display for ReadAdmissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Store read admission: {self:?}")
    }
}
impl std::error::Error for ReadAdmissionError {}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReadServiceWork {
    pub stopping: bool,
    /// Poisoned observations cannot establish a clean drain.
    pub poisoned: bool,
    pub readers: usize,
    pub quarantined: usize,
    pub waiting: usize,
    pub assigned: usize,
    pub leased: usize,
    pub outstanding: usize,
    pub peak_outstanding: usize,
    pub grants: u64,
    pub queue_wait_ns: u64,
    pub maximum_queue_wait_ns: u64,
    /// Fixed Rust scheduler/reader storage only; excludes provider and OS state.
    pub scheduler_bytes: usize,
}
pub(super) struct ReadPool {
    state: Mutex<State>,
    pub counts: Arc<Counts>,
    limits: ReadLimits,
}
impl ReadPool {
    pub fn new(
        readers: Vec<StoreReader>,
        limits: ReadLimits,
        counts: Arc<Counts>,
    ) -> Result<Arc<Self>, ReadAdmissionError> {
        if readers.is_empty()
            || limits.namespaces == 0
            || limits.requests_per_namespace == 0
            || limits
                .namespaces
                .checked_mul(limits.requests_per_namespace)
                .and_then(|n| n.checked_mul(std::mem::size_of::<super::read_state::Slot>()))
                .is_none_or(|bytes| bytes > isize::MAX as usize)
        {
            return Err(ReadAdmissionError::InvalidLimits);
        }
        let mut state = State::new(readers, limits);
        state.work.scheduler_bytes += std::mem::size_of::<Self>() - std::mem::size_of::<State>()
            + 2 * std::mem::size_of::<usize>();
        Ok(Arc::new(Self {
            state: Mutex::new(state),
            counts,
            limits,
        }))
    }
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poison| {
            let mut state = poison.into_inner();
            state.poisoned = true;
            state.stopping = true;
            state.distribute();
            state
        })
    }
    fn error(state: &State) -> ReadAdmissionError {
        if state.poisoned {
            ReadAdmissionError::Poisoned
        } else if state.stopping {
            ReadAdmissionError::Stopped
        } else {
            ReadAdmissionError::NoReaders
        }
    }
    pub fn request(
        self: &Arc<Self>,
        workspace: Option<WorkspaceId>,
    ) -> Result<ReadTicket, ReadAdmissionError> {
        let mut state = self.lock();
        if state.terminal() {
            return Err(Self::error(&state));
        }
        let lane = state
            .lanes
            .iter()
            .position(|lane| lane.active && lane.workspace == workspace)
            .or_else(|| state.lanes.iter().position(|lane| !lane.active))
            .ok_or(ReadAdmissionError::Capacity)?;
        if state.lanes[lane].outstanding == self.limits.requests_per_namespace {
            return Err(ReadAdmissionError::Capacity);
        }
        let slot = state
            .slots
            .iter()
            .position(|slot| matches!(slot.state, Disposition::Free))
            .ok_or(ReadAdmissionError::Capacity)?;
        if !state.lanes[lane].active {
            state.lanes[lane].active = true;
            state.lanes[lane].workspace = workspace;
        }
        state.lanes[lane].outstanding += 1;
        state.lanes[lane].queue.push_back(slot);
        state.slots[slot].lane = lane;
        state.slots[slot].admitted = Instant::now();
        state.slots[slot].state = Disposition::Waiting;
        state.work.waiting += 1;
        state.work.outstanding += 1;
        state.work.peak_outstanding = state.work.peak_outstanding.max(state.work.outstanding);
        state.distribute();
        drop(state);
        self.wake_ready();
        Ok(ReadTicket {
            pool: self.clone(),
            slot: Some(slot),
        })
    }
    /// Invoke arbitrary task behavior outside every scheduler lock. Scanning is
    /// bounded by the startup table, independent of names, objects or file size.
    fn wake_ready(&self) {
        for index in 0..self.limits.namespaces * self.limits.requests_per_namespace {
            let waker = {
                let mut state = self.lock();
                let slot = &mut state.slots[index];
                if matches!(slot.state, Disposition::Ready(..) | Disposition::Failed) {
                    slot.waker.take()
                } else {
                    None
                }
            };
            if let Some(waker) = waker {
                waker.wake();
            }
        }
    }
    pub fn release(
        &self,
        slot: usize,
        id: usize,
        reader: Box<StoreReader>,
        retired: Option<Arc<PortError>>,
    ) {
        let mut state = self.lock();
        state.work.leased -= 1;
        if let Some(error) = retired {
            state.retired[id] = Some((reader, error));
            state.work.quarantined += 1;
        } else {
            state.idle.push_back((id, reader));
        }
        state.remove(slot);
        state.distribute();
        drop(state);
        self.wake_ready();
    }
    fn cancel(&self, index: usize) {
        let mut state = self.lock();
        let old = state.slots[index].waker.take();
        match std::mem::replace(&mut state.slots[index].state, Disposition::Free) {
            Disposition::Waiting => {
                let lane = state.slots[index].lane;
                state.lanes[lane].queue.retain(|slot| *slot != index);
                state.work.waiting -= 1;
            }
            Disposition::Ready(id, reader) => {
                state.work.assigned -= 1;
                state.idle.push_back((id, reader));
            }
            Disposition::Failed => {}
            Disposition::Free | Disposition::Leased => unreachable!("owned unconsumed read ticket"),
        }
        state.remove(index);
        state.distribute();
        drop(state);
        drop(old);
        self.wake_ready();
    }
    pub fn work(&self) -> ReadServiceWork {
        let state = self.lock();
        ReadServiceWork {
            stopping: state.stopping,
            poisoned: state.poisoned,
            ..state.work
        }
    }
    pub fn outstanding(&self, workspace: WorkspaceId) -> Result<usize, ReadAdmissionError> {
        let state = self.lock();
        if state.poisoned {
            return Err(ReadAdmissionError::Poisoned);
        }
        Ok(state
            .lanes
            .iter()
            .find(|lane| lane.active && lane.workspace == Some(workspace))
            .map_or(0, |lane| lane.outstanding))
    }
    pub fn failures(&self) -> Vec<(usize, Arc<PortError>)> {
        self.lock()
            .retired
            .iter()
            .enumerate()
            .filter_map(|(id, reader)| reader.as_ref().map(|(_, error)| (id, error.clone())))
            .collect()
    }
    /// Stops only future/unstarted read admission. Leased consumers keep their
    /// original session until they finish; this method is not a drain receipt.
    pub fn stop(&self) {
        let mut state = self.lock();
        state.stopping = true;
        for index in 0..state.slots.len() {
            if matches!(state.slots[index].state, Disposition::Ready(..)) {
                if let Disposition::Ready(id, reader) =
                    std::mem::replace(&mut state.slots[index].state, Disposition::Failed)
                {
                    state.idle.push_back((id, reader));
                    state.work.assigned -= 1;
                }
            }
        }
        state.distribute();
        drop(state);
        self.wake_ready();
    }
}

/// Admission to one original read session, before a provider demand is attempted.
/// Dropping a queued/assigned ticket cancels only that unstarted admission.
pub struct ReadTicket {
    pool: Arc<ReadPool>,
    slot: Option<usize>,
}
impl Future for ReadTicket {
    type Output = Result<ReadLease, ReadAdmissionError>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let index = self.slot.expect("read ticket polled after completion");
        let mut replacement = Some(cx.waker().clone());
        let pool = self.pool.clone();
        let mut state = pool.lock();
        let old = state.slots[index].waker.take();
        let result = match std::mem::replace(&mut state.slots[index].state, Disposition::Leased) {
            Disposition::Waiting => {
                state.slots[index].state = Disposition::Waiting;
                state.slots[index].waker = replacement.take();
                Poll::Pending
            }
            Disposition::Ready(id, reader) => {
                self.slot = None;
                state.work.assigned -= 1;
                state.work.leased += 1;
                Poll::Ready(Ok(ReadLease {
                    pool: self.pool.clone(),
                    slot: index,
                    index: id,
                    reader: Some(reader),
                    failure: None,
                    workspace: state.lanes[state.slots[index].lane].workspace,
                    queue_wait_ns: state.slots[index].queue_wait_ns,
                }))
            }
            Disposition::Failed => {
                self.slot = None;
                let error = ReadPool::error(&state);
                state.remove(index);
                Poll::Ready(Err(error))
            }
            Disposition::Free | Disposition::Leased => unreachable!("original read admission"),
        };
        drop(state);
        drop(old);
        drop(replacement);
        result
    }
}
struct ThreadWake(Thread);
impl Wake for ThreadWake {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}
impl ReadTicket {
    /// Compatibility for control/constructor callers. Native workers await the
    /// Future instead; this wait must never run on a Fuse request-service worker.
    pub fn wait(mut self) -> Result<ReadLease, ReadAdmissionError> {
        let waker = Waker::from(Arc::new(ThreadWake(thread::current())));
        loop {
            match Pin::new(&mut self).poll(&mut Context::from_waker(&waker)) {
                Poll::Ready(result) => return result,
                Poll::Pending => thread::park(),
            }
        }
    }
}
impl Drop for ReadTicket {
    fn drop(&mut self) {
        if let Some(index) = self.slot.take() {
            self.pool.cancel(index);
        }
    }
}
