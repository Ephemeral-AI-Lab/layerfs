//! Exact per-connection custody: mount, receive loops, their owner and receipts.
use crate::{
    mount::{AbortWrite, MountEntry, Negotiation},
    ports::{FailedDemands, Fence},
    request::{Accounting, OpcodeWork},
    DispatchError, MountQueue, MountWork,
};
use fuser::{SessionMonitor, SessionOutcome, SessionPhase, SessionSnapshot};
use layerfs_overlay::NativeMount;
use nix::errno::Errno;
use std::{
    any::Any,
    fmt,
    fs::File,
    io,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock,
    },
    thread::JoinHandle,
    time::Duration,
};

/// One receive loop per connection. The kernel hands a request to the loop
/// that has waited longest, so a second loop made a serial caller alternate
/// between two threads; every owner job is serialized on one connection
/// anyway, and provider I/O leaves the loop for a worker.
pub const RECEIVE_LOOPS: usize = crate::RECEIVE_SLOTS;

#[derive(Clone, Debug)]
pub struct SessionConfig {
    /// A new directory created by this attach; an existing path is refused.
    pub directory: PathBuf,
    /// Daemon identity recorded as the mount owner, never the command identity.
    pub owner_uid: u32,
    pub owner_gid: u32,
    /// Observation deadline for all-loop serving. Expiry cancels nothing.
    pub ready_wait: Duration,
    /// Observation deadline for loop joins and request drain.
    pub drain_wait: Duration,
}

/// One mounted connection. Dropping it performs no detach, abort, join or
/// revocation: every terminal step is an explicit call with a retained result.
pub struct NativeSession {
    pub(super) mount: NativeMount,
    pub(super) directory: PathBuf,
    pub(super) queue: MountQueue,
    /// The lane's fence, kept so its count outlives the lane's release.
    pub(super) fence: Fence,
    pub(super) accounting: Arc<Accounting>,
    pub(super) negotiation: Option<Negotiation>,
    pub(super) entry: Option<MountEntry>,
    /// Taken by the one abort write; never reopened.
    pub(super) abort: Option<File>,
    /// That write's original result, readable by observers.
    pub(super) aborted: Arc<OnceLock<AbortWrite>>,
    /// The one plain detach of forced teardown.
    pub(super) forced_detach: DetachAttempt,
    pub(super) monitor: Option<SessionMonitor>,
    pub(super) owner: Option<JoinHandle<SessionOutcome>>,
    pub(super) watcher: Option<JoinHandle<()>>,
    pub(super) outcome: Option<SessionOutcome>,
    pub(super) detached: Arc<AtomicBool>,
    pub(super) drain_wait: Duration,
}
/// Maintained observations only: no syscall, SQL job or filesystem request.
#[derive(Clone, Copy, Debug)]
pub struct SessionFacts {
    pub mount: NativeMount,
    pub loops: Option<SessionSnapshot>,
    /// Every configured loop entered and none has exited.
    pub serving: bool,
    pub detached: bool,
    pub work: Result<MountWork, DispatchError>,
    pub opcodes: OpcodeWork,
    /// A connection-specific abort control was bound at attach.
    pub abort_bound: bool,
    /// The one abort write of forced teardown, once it was made.
    pub aborted: Option<AbortWrite>,
}
/// Forced teardown refused before any effect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ForceRefusal {
    /// No abort control was bound to this connection at attach.
    AbortUnavailable,
    /// Already detached, or its one abort write was already made.
    NotServing,
}
/// The one plain detach attempt of forced teardown; none follows it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DetachAttempt {
    NotAttempted,
    Detached,
    /// The original errno. `EBUSY` here is aborted and still mounted, never
    /// the reversible answer of a normal probe.
    Failed(Errno),
}
/// The effects forced teardown had on this connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Forced {
    pub abort: AbortWrite,
    pub detach: DetachAttempt,
    /// Admitted requests ended by one terminal reply; see `Fence`.
    pub terminal_replies: u64,
}
impl NativeSession {
    pub const fn mount(&self) -> NativeMount {
        self.mount
    }
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    /// Recorded inside the completed handshake; absent only on a failed attach.
    pub fn negotiation(&self) -> Option<&Negotiation> {
        self.negotiation.as_ref()
    }
    pub fn entry(&self) -> Option<&MountEntry> {
        self.entry.as_ref()
    }
    pub fn facts(&self) -> SessionFacts {
        self.observer().facts()
    }
    /// An observation handle for status while a terminal operation owns the
    /// session. It carries no detach, abort, join or release authority.
    pub fn observer(&self) -> SessionObserver {
        SessionObserver {
            mount: self.mount,
            queue: self.queue.clone(),
            accounting: self.accounting.clone(),
            monitor: self.monitor.clone(),
            detached: self.detached.clone(),
            abort_bound: self.abort.is_some() || self.aborted.get().is_some(),
            aborted: self.aborted.clone(),
        }
    }
    pub(super) fn is_detached(&self) -> bool {
        self.detached.load(Ordering::Acquire)
    }
    /// Present once the abort write was made, whatever it returned.
    pub(super) fn forced(&self) -> Option<Forced> {
        self.aborted.get().map(|abort| Forced {
            abort: *abort,
            detach: self.forced_detach,
            terminal_replies: self.fence.terminal_replies(),
        })
    }
}
/// Maintained observations of one connection, independent of who owns it.
#[derive(Clone)]
pub struct SessionObserver {
    mount: NativeMount,
    queue: MountQueue,
    accounting: Arc<Accounting>,
    monitor: Option<SessionMonitor>,
    detached: Arc<AtomicBool>,
    abort_bound: bool,
    aborted: Arc<OnceLock<AbortWrite>>,
}
impl SessionObserver {
    pub fn facts(&self) -> SessionFacts {
        let loops = self.monitor.as_ref().map(SessionMonitor::snapshot);
        let detached = self.detached.load(Ordering::Acquire);
        SessionFacts {
            mount: self.mount,
            loops,
            serving: !detached
                && loops.is_some_and(|loops| {
                    loops.phase == SessionPhase::Serving && loops.entered == RECEIVE_LOOPS
                }),
            detached,
            work: self.queue.work(),
            opcodes: self.accounting.observe(),
            abort_bound: self.abort_bound,
            aborted: self.aborted.get().copied(),
        }
    }
}
impl fmt::Debug for SessionObserver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SessionObserver")
            .field("facts", &self.facts())
            .finish()
    }
}
impl fmt::Debug for NativeSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeSession")
            .field("mount", &self.mount)
            .field("directory", &self.directory)
            .field("facts", &self.facts())
            .field("owner_joined", &self.owner.is_none())
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachPhase {
    Directory,
    Device,
    Filesystem,
    Mount,
    Handshake,
    Spawn,
    Serving,
    Receipt,
}
/// What remains of a failed attach after its one owned disposal attempt.
#[derive(Debug)]
pub enum AttachRemainder {
    /// No mount, loop, request, lane or directory of this attempt remains.
    Disposed(Option<Box<Drained>>),
    /// A directory removal or lane release before any mount effect failed.
    Unmounted {
        directory: Option<io::Error>,
        lane: Option<DispatchError>,
    },
    /// Detach, join or request drain is not established; all handles are kept.
    Retained(Box<Undrained>),
}
/// Original failing boundary; a later attach is a new operation, never a replay.
#[derive(Debug)]
pub struct AttachFailure {
    pub phase: AttachPhase,
    pub cause: io::Error,
    /// True once the mount call returned success.
    pub mounted: bool,
    pub remainder: AttachRemainder,
}
impl fmt::Display for AttachFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "native attach {:?}: {}", self.phase, self.cause)
    }
}
impl std::error::Error for AttachFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DrainStage {
    /// Detach is not known; drain was refused before any wait.
    Detach,
    /// Not every created receive loop is known joined.
    Join,
    /// The session owner or admission watcher did not return normally.
    Owner,
    /// Received, admitted or retained request ownership remains.
    Requests,
    /// The dispatcher refused to release this mount's fixed lane.
    Lane,
}
/// The connection stays owned with its exact stopping stage; a later drain call
/// observes the same predicate again and replays no syscall.
pub struct Undrained {
    pub session: NativeSession,
    pub stage: DrainStage,
    pub loops: Option<SessionSnapshot>,
    pub work: Option<MountWork>,
    pub lane: Option<DispatchError>,
    /// Original panic payload of the owner or watcher thread.
    pub panic: Option<Box<dyn Any + Send>>,
    /// Present once forced teardown made its abort write.
    pub forced: Option<Forced>,
    /// The mount's failed base demands as recorded at the stop.
    pub failed_demands: FailedDemands,
}
impl fmt::Debug for Undrained {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Undrained")
            .field("stage", &self.stage)
            .field("loops", &self.loops)
            .field("work", &self.work)
            .field("lane", &self.lane)
            .field("panicked", &self.panic.is_some())
            .field("forced", &self.forced)
            .field("failed_demands", &self.failed_demands)
            .field("session", &self.session)
            .finish()
    }
}
/// Connection-drained receipt: detached, every created loop joined, no request
/// or receive unit left and the lane released. It is necessary for terminal
/// unmount and never sufficient: engine, Store and control owners are separate.
#[derive(Debug)]
pub struct Drained {
    pub mount: NativeMount,
    pub directory: PathBuf,
    /// Original loop/cleanup outcomes; absent when no loop was ever created.
    pub outcome: Option<SessionOutcome>,
    pub work: MountWork,
    pub opcodes: OpcodeWork,
    /// The one removal attempt of the now unmounted directory.
    pub removed: io::Result<()>,
    /// Present when forced teardown drained this connection.
    pub forced: Option<Forced>,
    /// Requests of this mount that ended on a failed base demand: the count
    /// with the first and the most recent original cause.
    pub failed_demands: FailedDemands,
}
impl Drained {
    /// Joined is disposal; clean additionally requires successful loops.
    pub fn clean(&self) -> bool {
        self.removed.is_ok() && self.outcome.as_ref().is_none_or(SessionOutcome::is_clean)
    }
}
