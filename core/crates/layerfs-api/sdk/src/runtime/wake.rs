//! Coalesced publication hints; no original event or provider outcome is stored.
use layerfs_bridge::contract::FrameError;
use std::{
    sync::{mpsc, Arc, Condvar, Mutex},
    time::Instant,
};

pub(crate) struct Signal {
    changed: Mutex<bool>,
    wake: Condvar,
}
impl Signal {
    pub(crate) fn new() -> Self {
        Self {
            changed: Mutex::new(false),
            wake: Condvar::new(),
        }
    }
    /// Clear before polling every occupied owner, never after the final poll.
    pub(crate) fn begin_round(&self) -> Result<(), FrameError> {
        *self.changed.lock().map_err(|_| FrameError::Poisoned)? = false;
        Ok(())
    }
    pub(crate) fn changed(&self) -> Result<bool, FrameError> {
        self.changed
            .lock()
            .map(|changed| *changed)
            .map_err(|_| FrameError::Poisoned)
    }
    /// Publication precedes notification. No other ledger lock may span this call.
    pub(crate) fn notify(&self) -> Result<(), FrameError> {
        let result = match self.changed.lock() {
            Ok(mut changed) => {
                *changed = true;
                Ok(())
            }
            Err(_) => Err(FrameError::Poisoned),
        };
        // Also wake a waiter when the latch is unavailable; poison is not quiet.
        self.wake.notify_all();
        result
    }
    pub(crate) fn wait_until(&self, deadline: Instant) -> Result<SupervisorWake, FrameError> {
        let mut changed = self.changed.lock().map_err(|_| FrameError::Poisoned)?;
        loop {
            if *changed {
                return Ok(SupervisorWake::Notified);
            }
            let now = Instant::now();
            if now >= deadline {
                return Ok(SupervisorWake::DeadlineReached);
            }
            let (next, _) = self
                .wake
                .wait_timeout(changed, deadline.saturating_duration_since(now))
                .map_err(|_| FrameError::Poisoned)?;
            changed = next;
        }
    }
}

/// A wait result only; neither variant closes a socket or decides an operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SupervisorWake {
    /// A coalesced hint exists. Resume the ordinary fair turn to inspect originals.
    Notified,
    /// The caller's wait bound elapsed; all original owners remain unchanged.
    DeadlineReached,
}
/// Advisory application wake after publishing its own bounded control/listener work.
/// It carries no Connection, authority, request, reply or completion.
#[derive(Clone)]
pub struct SupervisorWakeHandle {
    pub(crate) signal: Arc<Signal>,
}
impl SupervisorWakeHandle {
    /// Notify the serving thread, without dispatch, shutdown or failed-call replay.
    pub fn notify(&self) -> Result<(), FrameError> {
        self.signal.notify()
    }
}

/// The existing bounded sender with publication and actual-drop notification.
/// Its queue, original values, blocking send and SendError ownership are unchanged.
pub(crate) struct PublishedSender<T> {
    sender: Option<mpsc::SyncSender<T>>,
    signal: Option<Arc<Signal>>,
}
impl<T> PublishedSender<T> {
    pub(crate) fn new(sender: mpsc::SyncSender<T>, signal: Option<Arc<Signal>>) -> Self {
        Self {
            sender: Some(sender),
            signal,
        }
    }
    pub(crate) fn send(&self, value: T) -> Result<(), mpsc::SendError<T>> {
        let result = self.sender.as_ref().expect("live sender").send(value);
        if result.is_ok() {
            self.notify();
        }
        result
    }
    fn notify(&self) {
        if let Some(signal) = &self.signal {
            // The observed turn/wait reports unavailable notification independently
            // of original queued events and native worker failures.
            let _ = signal.notify();
        }
    }
}
impl<T> Drop for PublishedSender<T> {
    fn drop(&mut self) {
        // This order also runs during unwind and unattempted worker-start refusal.
        drop(self.sender.take());
        self.notify();
    }
}
