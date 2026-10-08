//! Forced teardown of one connection: one abort write, the terminal fence,
//! the local drain, then one plain detach. Nothing here is repeated.
use super::state::{DetachAttempt, DrainStage, Drained, ForceRefusal, NativeSession, Undrained};
use crate::mount::{syscalls, AbortWrite};
use std::{sync::atomic::Ordering, time::Instant};

impl NativeSession {
    /// The abort control bound at attach is still held, unused.
    pub fn abort_bound(&self) -> bool {
        self.abort.is_some()
    }
    /// The connection's single abort write. Refused before any effect when
    /// the session is already detached or aborted, or when no control was
    /// bound. The control leaves the session before the write, so no second
    /// write can follow any result. Only `Written` stops the mount's request
    /// service; a short or failed write does nothing further.
    pub fn abort(&mut self) -> Result<AbortWrite, ForceRefusal> {
        if self.is_detached() || self.aborted.get().is_some() {
            return Err(ForceRefusal::NotServing);
        }
        let Some(control) = self.abort.take() else {
            return Err(ForceRefusal::AbortUnavailable);
        };
        let written = syscalls::abort(&control);
        drop(control);
        let _ = self.aborted.set(written);
        if written == AbortWrite::Written {
            // Wake receive waiters and give every parked request the turn in
            // which it observes the stop. A lane cannot be stale while this
            // session owns it.
            let _ = self.queue.stop_service();
        }
        Ok(written)
    }
    /// After a `Written` abort: the same local drain as a normal unmount,
    /// then exactly one plain detach, then the lane and directory. The detach
    /// is attempted only once the local drain held, and never again: `EBUSY`
    /// or any other errno stops at `Detach` with the connection aborted and
    /// still mounted. Called again, it observes the same predicate; a
    /// recorded failed detach is returned as it was, with no syscall.
    pub fn force_drain(mut self) -> Result<Box<Drained>, Box<Undrained>> {
        let attempted = match self.forced_detach {
            DetachAttempt::Failed(_) => return Err(self.undrained(DrainStage::Detach, None)),
            DetachAttempt::Detached => true,
            DetachAttempt::NotAttempted => false,
        };
        if self.aborted.get() != Some(&AbortWrite::Written) || self.is_detached() != attempted {
            return Err(self.undrained(DrainStage::Detach, None));
        }
        let deadline = Instant::now() + self.drain_wait;
        let work = match self.quiesce(deadline) {
            Ok(work) => work,
            Err(stop) => return Err(self.stopped(stop)),
        };
        if !attempted {
            match syscalls::detach(&self.directory) {
                Ok(()) => {
                    self.forced_detach = DetachAttempt::Detached;
                    self.detached.store(true, Ordering::Release);
                }
                Err(errno) => {
                    self.forced_detach = DetachAttempt::Failed(errno);
                    return Err(self.undrained(DrainStage::Detach, None));
                }
            }
        }
        self.release(work)
    }
}
