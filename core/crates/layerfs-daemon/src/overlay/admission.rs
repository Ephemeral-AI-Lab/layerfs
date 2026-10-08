//! Fixed notification slots for before-effect admission waits.
use crate::{Command, OwnerClient, OwnerConfig, OwnerError, Pending};
use layerfs_overlay::Route;
use std::{
    future::Future,
    mem::size_of,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll, Wake, Waker},
    thread::{self, Thread},
};

#[derive(Default)]
struct Slot {
    active: bool,
    changed: bool,
    waker: Option<Waker>,
}

pub(crate) struct Notifications {
    slots: Mutex<Vec<Slot>>,
    bytes: usize,
}
impl Notifications {
    pub fn planned_bytes(config: OwnerConfig) -> Option<usize> {
        config
            .jobs_per_namespace
            .checked_add(config.lifecycle_jobs_per_namespace)?
            .checked_mul(config.namespaces)?
            .checked_mul(size_of::<Slot>())
    }
    pub fn new(config: OwnerConfig) -> Self {
        let count =
            config.namespaces * (config.jobs_per_namespace + config.lifecycle_jobs_per_namespace);
        let mut slots = Vec::with_capacity(count);
        slots.resize_with(count, Slot::default);
        Self {
            bytes: slots.capacity() * size_of::<Slot>(),
            slots: Mutex::new(slots),
        }
    }
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    fn reserve(&self) -> Result<usize, OwnerError> {
        let mut slots = self.slots.lock().map_err(|_| OwnerError::Stopped)?;
        let index = slots
            .iter()
            .position(|slot| !slot.active)
            .ok_or(OwnerError::AdmissionFull)?;
        slots[index].active = true;
        slots[index].changed = true;
        Ok(index)
    }
    /// Arm before checking credit availability. A release on either side of
    /// the check is observed; a spurious poll cannot resubmit an admitted job.
    fn arm(&self, index: usize, waker: Waker) -> Result<bool, OwnerError> {
        let mut slots = self.slots.lock().map_err(|_| OwnerError::Stopped)?;
        let slot = &mut slots[index];
        let old = slot.waker.replace(waker);
        let changed = std::mem::replace(&mut slot.changed, false);
        drop(slots);
        drop(old);
        Ok(changed)
    }
    fn release(&self, index: usize) {
        let old = {
            let mut slots = self.slots.lock().unwrap_or_else(|error| error.into_inner());
            std::mem::take(&mut slots[index])
        };
        drop(old);
    }
    /// No queue lock is held by the caller. Wake/drop user tasks outside both
    /// locks, with no event-sized allocation and only the fixed slot scan.
    pub fn notify(&self) {
        let count = self.slots.lock().unwrap_or_else(|e| e.into_inner()).len();
        for index in 0..count {
            let waker = {
                let mut slots = self.slots.lock().unwrap_or_else(|e| e.into_inner());
                let slot = &mut slots[index];
                if slot.active {
                    slot.changed = true;
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
}

/// An original command waiting before SQL admission. The caller must account
/// for its command's storage within its own bounded ingress ownership. The
/// owner's startup allocation bounds notification registrations independently.
/// Once admitted, the returned Pending owns the original job/result credit.
pub struct Admission {
    client: OwnerClient,
    slot: Option<usize>,
    route: Option<Route>,
    command: Option<Command>,
}
impl OwnerClient {
    /// Reserves one fixed notification slot without attempting the command.
    /// Registration saturation returns that same unattempted input. Native
    /// assembly must budget registrations for its admitted ingress slots.
    pub fn submit_when_available(
        &self,
        route: Option<Route>,
        command: Command,
    ) -> Result<Admission, (OwnerError, Command)> {
        if (route.is_none()
            && !matches!(
                command,
                Command::Open { .. }
                    | Command::ObserveCleanup { .. }
                    | Command::Resources { global: true }
            ))
            || (route.is_some()
                && matches!(
                    command,
                    Command::Open { .. } | Command::ObserveCleanup { .. }
                ))
        {
            return Err((OwnerError::InvalidAdmission, command));
        }
        let Some(bytes) = command.charge() else {
            return Err((OwnerError::InvalidAdmission, command));
        };
        {
            let state = match self.shared.state.lock() {
                Ok(state) => state,
                Err(_) => return Err((OwnerError::Stopped, command)),
            };
            if state.stopping {
                return Err((OwnerError::Stopped, command));
            }
            // Occupied credit can become available; a charge exceeding the
            // entire fixed capacity cannot. Refuse that original input now.
            if bytes > self.shared.job_capacity(command.class(), &state.work) {
                return Err((OwnerError::AdmissionFull, command));
            }
        }
        let slot = match self.shared.admission.reserve() {
            Ok(slot) => slot,
            Err(error) => return Err((error, command)),
        };
        Ok(Admission {
            client: self.clone(),
            slot: Some(slot),
            route,
            command: Some(command),
        })
    }
}
impl OwnerClient {
    /// The fixed limits this owner was started with.
    pub fn configuration(&self) -> OwnerConfig {
        self.shared.config
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
impl OwnerClient {
    /// The synchronous form for a control or constructor thread: one
    /// before-effect readiness wait for a credit, then the single
    /// submission. A stopped owner or a refusal that waiting cannot cure
    /// returns the original input. Never call it on a Fuse receive or
    /// request-service worker, or while holding the credits it waits for.
    pub fn submit_waiting(
        &self,
        route: Option<Route>,
        command: Command,
    ) -> Result<Pending, (OwnerError, Command)> {
        let mut admission = self.submit_when_available(route, command)?;
        let waker = Waker::from(Arc::new(ThreadWake(thread::current())));
        loop {
            match Pin::new(&mut admission).poll(&mut Context::from_waker(&waker)) {
                Poll::Ready(outcome) => return outcome,
                Poll::Pending => thread::park(),
            }
        }
    }
}
impl Admission {
    /// Cancels only this pre-admission wait and returns its original input.
    /// After Poll::Ready there is no unattempted command to recover here.
    pub fn into_command(mut self) -> Option<Command> {
        self.command.take()
    }
    fn release_slot(&mut self) {
        if let Some(slot) = self.slot.take() {
            self.client.shared.admission.release(slot);
        }
    }
}
impl Future for Admission {
    type Output = Result<Pending, (OwnerError, Command)>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let slot = self.slot.expect("admission polled after completion");
        // RawWaker implementations may reenter the service even on clone/drop.
        let waker = cx.waker().clone();
        match self.client.shared.admission.arm(slot, waker) {
            Ok(false) => return Poll::Pending,
            Ok(true) => {}
            Err(error) => {
                self.release_slot();
                return Poll::Ready(Err((error, self.command.take().unwrap())));
            }
        }
        let command = self.command.take().unwrap();
        match self.client.try_submit(self.route, command) {
            Err((OwnerError::AdmissionFull, command)) => {
                self.command = Some(command);
                Poll::Pending
            }
            outcome => {
                self.release_slot();
                Poll::Ready(outcome)
            }
        }
    }
}
impl Drop for Admission {
    fn drop(&mut self) {
        self.release_slot();
    }
}
