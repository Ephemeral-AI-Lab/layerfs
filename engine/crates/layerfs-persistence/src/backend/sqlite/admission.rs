//! FIFO connection ownership before a single SQL attempt (#352 F-040).
//!
//! A queued caller waits for its turn with no time limit: waiting is
//! readiness, never a retry, and correct queued work is not refused for
//! waiting. Each queued entry is one blocked thread, so the queue is bounded
//! by the threads the process can run, not by a count. A waiter leaves
//! without its turn only by a named exit:
//! - `Closed`: the Store was closed (`Admission::close`) before its turn;
//! - `Unknown`: the queue's mutex was poisoned by a panicking holder;
//! - `Integrity`: the caller's own thread already holds the turn outside a
//!   lane, so waiting would deadlock on itself.
//!
//! A holder that unwinds still releases its turn (`Turn`'s drop), so a gone
//! owner hands the turn on rather than stranding the queue.
use crate::backend::records::BackendError;
use layerfs_telemetry::timer::waits::{self, Wait};
use std::{
    collections::VecDeque,
    sync::{Condvar, Mutex},
    thread::ThreadId,
    time::Instant,
};

#[derive(Default)]
struct Queue {
    next: u64,
    /// The thread whose turn is in progress.
    active: Option<ThreadId>,
    /// Set once by `close`; queued and later entries then fail `Closed`.
    closed: bool,
    /// Waiting tickets in arrival order; the front is admitted next.
    pending: VecDeque<u64>,
    /// The thread holding the publication lane: its own nested SQL work runs
    /// inside the turn it already owns instead of queueing behind itself.
    holder: Option<ThreadId>,
}

/// Adds a queued entry's wait to the thread's total on every exit.
struct Waited(Instant, Wait);
impl Drop for Waited {
    fn drop(&mut self) {
        waits::add(self.1, self.0.elapsed());
    }
}

#[derive(Default)]
pub(crate) struct Admission {
    queue: Mutex<Queue>,
    changed: Condvar,
}

pub(crate) struct Turn<'a> {
    admission: &'a Admission,
    /// Work by the lane holder inside its held turn; releasing it is the hold's.
    pub(crate) nested: bool,
}

impl Admission {
    /// Waits, first come first served, for this connection's turn. Time spent
    /// queued is added to the calling thread's wait total for `kind`.
    pub(crate) fn enter(&self, kind: Wait) -> Result<Turn<'_>, BackendError> {
        let mut queue = self.queue.lock().map_err(|_| BackendError::Unknown)?;
        let thread = std::thread::current().id();
        // The job in progress finishes, even after `close`.
        if queue.holder == Some(thread) {
            return Ok(Turn {
                admission: self,
                nested: true,
            });
        }
        if queue.closed {
            return Err(BackendError::Closed);
        }
        if queue.active == Some(thread) {
            return Err(BackendError::Integrity);
        }
        if queue.active.is_none() && queue.pending.is_empty() {
            queue.active = Some(thread);
            return Ok(Turn {
                admission: self,
                nested: false,
            });
        }
        // Unique while fewer than 2^64 tickets wait at once.
        let ticket = queue.next;
        queue.next = queue.next.wrapping_add(1);
        queue.pending.push_back(ticket);
        let _waited = Waited(Instant::now(), kind);
        loop {
            if queue.closed {
                queue.pending.retain(|value| *value != ticket);
                return Err(BackendError::Closed);
            }
            if queue.active.is_none() && queue.pending.front() == Some(&ticket) {
                queue.pending.pop_front();
                queue.active = Some(thread);
                return Ok(Turn {
                    admission: self,
                    nested: false,
                });
            }
            queue = self
                .changed
                .wait(queue)
                .map_err(|_| BackendError::Unknown)?;
        }
    }

    /// Refuses every queued and later entry with `Closed` and wakes the queue.
    /// The turn in progress, and its lane's nested work, run to completion.
    pub(crate) fn close(&self) {
        let mut queue = self.queue.lock().unwrap_or_else(|error| error.into_inner());
        queue.closed = true;
        self.changed.notify_all();
    }
}

impl Turn<'_> {
    /// Keeps this turn for the calling thread until `release_hold`: its later
    /// SQL work enters as nested turns, and every other caller queues.
    pub(crate) fn hold(self) -> Result<(), BackendError> {
        let mut queue = self
            .admission
            .queue
            .lock()
            .map_err(|_| BackendError::Unknown)?;
        queue.holder = Some(std::thread::current().id());
        drop(queue);
        std::mem::forget(self);
        Ok(())
    }
}

impl Admission {
    /// Ends a turn kept by `Turn::hold`.
    pub(crate) fn release_hold(&self) {
        let mut queue = self.queue.lock().unwrap_or_else(|error| error.into_inner());
        queue.holder = None;
        queue.active = None;
        self.changed.notify_all();
    }
}

impl Drop for Turn<'_> {
    fn drop(&mut self) {
        if self.nested {
            return;
        }
        // The operation never owns this mutex while invoking a caller or SQL.
        // Even a poisoned queue is released; future admissions still refuse it.
        let mut queue = self
            .admission
            .queue
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        queue.active = None;
        self.admission.changed.notify_all();
    }
}
