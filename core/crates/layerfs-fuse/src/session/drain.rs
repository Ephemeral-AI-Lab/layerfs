//! Reversible plain detach, then complete connection and request drain.
use super::state::{DrainStage, Drained, NativeSession, Undrained};
use crate::{mount::syscalls, DispatchError, MountWork};
use fuser::SessionPhase;
use nix::errno::Errno;
use std::{any::Any, fs, sync::atomic::Ordering, time::Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Detach {
    /// The kernel acknowledged the detach; native admission is now terminal.
    Detached,
    /// The kernel's reversible answer. Nothing changed: requests were and
    /// remain serviced normally and no terminal error was injected.
    Busy,
}
/// Where the shared join-and-quiesce half stopped, with what it observed.
pub(super) struct Stop {
    stage: DrainStage,
    panic: Option<Box<dyn Any + Send>>,
    lane: Option<DispatchError>,
}
impl NativeSession {
    /// One plain detach attempt while ordinary request service continues. Any
    /// other errno is the original unestablished outcome for the caller to keep.
    /// Refused before the syscall once forced teardown made its abort write:
    /// that path owns the connection's single detach.
    pub fn detach(&mut self) -> Result<Detach, Errno> {
        if self.is_detached() || self.aborted.get().is_some() {
            return Err(Errno::EINVAL);
        }
        match syscalls::detach(&self.directory) {
            Ok(()) => {
                self.detached.store(true, Ordering::Release);
                // Known detach is the fence: wake every borrowed admission
                // waiter. Already admitted work keeps its original disposition.
                let _ = self.queue.stop_admission();
                Ok(Detach::Detached)
            }
            Err(Errno::EBUSY) => Ok(Detach::Busy),
            Err(errno) => Err(errno),
        }
    }
    /// Establishes that every created loop is joined and no received, admitted
    /// or retained request remains, then releases the lane. Waits are bounded
    /// observations: expiry keeps the whole owner and fabricates nothing.
    pub fn drain(mut self) -> Result<Box<Drained>, Box<Undrained>> {
        if !self.is_detached() {
            return Err(self.undrained(DrainStage::Detach, None));
        }
        let deadline = Instant::now() + self.drain_wait;
        match self.quiesce(deadline) {
            Ok(work) => self.release(work),
            Err(stop) => Err(self.stopped(stop)),
        }
    }
    /// The half both terminal paths share: every created loop joined, the
    /// owner and watcher returned, and no received or admitted request left.
    /// It makes no syscall and releases nothing.
    pub(super) fn quiesce(&mut self, deadline: Instant) -> Result<MountWork, Stop> {
        let stop = |stage, panic| Stop {
            stage,
            panic,
            lane: None,
        };
        if let (Some(monitor), true) = (&self.monitor, self.owner.is_some()) {
            let mut loops = monitor.snapshot();
            while loops.phase != SessionPhase::Joined {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(stop(DrainStage::Join, None));
                }
                loops = monitor.wait_for_change(loops.revision, remaining);
            }
        }
        if let Some(owner) = self.owner.take() {
            match owner.join() {
                Ok(outcome) => self.outcome = Some(outcome),
                Err(payload) => return Err(stop(DrainStage::Owner, Some(payload))),
            }
        }
        if let Some(watcher) = self.watcher.take() {
            if let Err(payload) = watcher.join() {
                return Err(stop(DrainStage::Owner, Some(payload)));
            }
        }
        match self.queue.wait_quiescent(deadline) {
            Ok(work) if work.received == 0 && work.admitted == 0 => Ok(work),
            Ok(_) => Err(stop(DrainStage::Requests, None)),
            Err(lane) => Err(Stop {
                stage: DrainStage::Lane,
                panic: None,
                lane: Some(lane),
            }),
        }
    }
    /// The release half, only for a detached and quiesced connection: the
    /// lane's fixed slot, then the one removal of the unmounted directory.
    pub(super) fn release(self, work: MountWork) -> Result<Box<Drained>, Box<Undrained>> {
        if let Err(lane) = self.queue.finish() {
            let mut undrained = self.undrained(DrainStage::Lane, None);
            undrained.lane = Some(lane);
            return Err(undrained);
        }
        Ok(Box::new(Drained {
            mount: self.mount,
            forced: self.forced(),
            opcodes: self.accounting.observe(),
            removed: fs::remove_dir(&self.directory),
            directory: self.directory,
            outcome: self.outcome,
            work,
        }))
    }
    pub(super) fn stopped(self, stop: Stop) -> Box<Undrained> {
        let mut undrained = self.undrained(stop.stage, stop.panic);
        if stop.lane.is_some() {
            undrained.lane = stop.lane;
        }
        undrained
    }
    pub(super) fn undrained(
        self,
        stage: DrainStage,
        panic: Option<Box<dyn Any + Send>>,
    ) -> Box<Undrained> {
        let facts = self.facts();
        Box::new(Undrained {
            stage,
            loops: facts.loops,
            work: facts.work.ok(),
            lane: facts.work.err(),
            panic,
            forced: self.forced(),
            session: self,
        })
    }
}
