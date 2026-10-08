//! Explicit receiver startup, serving observation and complete join ownership.

use std::fmt;
use std::io;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex};

use super::{FilesystemHolder, Session, SessionEventLoop};
use crate::Filesystem;

/// Monotonic phase of one original session run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionPhase {
    /// The runner exists but has not entered its one run attempt.
    Prepared,
    /// Threads are being created and rendezvousing before receiving requests.
    Starting,
    /// Every configured loop entered and the startup rendezvous was released.
    Serving,
    /// Startup failed or at least one receiver exited; other joins may remain.
    Stopping,
    /// Every created receiver was joined and filesystem cleanup was attempted.
    /// This is a disposal fact, not a successful-outcome verdict.
    Joined,
}

/// Fixed-size observation; detailed original outcomes belong to SessionOutcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionSnapshot {
    /// Current phase; Serving is revoked on the first receiver exit.
    pub phase: SessionPhase,
    /// Monotonic revision for event waits, within this session only.
    pub revision: u64,
    /// Configured receiver count.
    pub configured: usize,
    /// Successful spawn calls acknowledged by the session owner.
    pub created: usize,
    /// Loops that reached the rendezvous after allocating their receive buffer.
    pub entered: usize,
    /// Loops whose receive/dispatch invocation exited or unwound.
    pub exited: usize,
    /// Created threads whose original join results were consumed.
    pub joined: usize,
}

#[derive(Debug)]
struct State {
    snapshot: SessionSnapshot,
    spawning_done: bool,
}

#[derive(Debug)]
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
}

impl Shared {
    fn changed(&self, state: &mut State) {
        // At most a fixed number of transitions per configured receiver.
        state.snapshot.revision = state.snapshot.revision.saturating_add(1);
        self.changed.notify_all();
    }

    fn serving(&self, state: &mut State) {
        let s = &mut state.snapshot;
        if s.phase == SessionPhase::Starting
            && state.spawning_done
            && s.created == s.configured
            && s.entered == s.configured
        {
            s.phase = SessionPhase::Serving;
        }
        self.changed(state);
    }

    fn stopping(&self) {
        let mut state = self.state.lock();
        state.snapshot.phase = SessionPhase::Stopping;
        self.changed(&mut state);
    }
}

/// Waitable observations of the same original run; no syscall is replayed.
#[derive(Clone, Debug)]
pub struct SessionMonitor(Arc<Shared>);

impl SessionMonitor {
    /// Observe current lifecycle state without changing it.
    pub fn snapshot(&self) -> SessionSnapshot {
        self.0.state.lock().snapshot
    }

    /// Wait for a different revision or the caller's observation deadline.
    /// A timeout only returns the current observation; it never cancels or
    /// disposes the runner and is not a Ready or joined receipt.
    pub fn wait_for_change(&self, revision: u64, timeout: Duration) -> SessionSnapshot {
        let start = Instant::now();
        let mut state = self.0.state.lock();
        while state.snapshot.revision == revision {
            let remaining = timeout.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                break;
            }
            self.0.changed.wait_for(&mut state, remaining);
        }
        state.snapshot
    }

    /// Wait for all-loop serving or a terminal startup/exit observation.
    /// Prepared/Starting on return means the observation deadline expired.
    pub fn wait_ready(&self, timeout: Duration) -> SessionSnapshot {
        let start = Instant::now();
        let mut state = self.0.state.lock();
        while matches!(
            state.snapshot.phase,
            SessionPhase::Prepared | SessionPhase::Starting
        ) {
            let remaining = timeout.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                break;
            }
            self.0.changed.wait_for(&mut state, remaining);
        }
        state.snapshot
    }
}

/// One original receiver join result, including its unmodified panic payload.
pub struct ReceiverOutcome {
    /// Index assigned before this receiver's single spawn attempt.
    pub index: usize,
    /// The actual join result and, when returned, actual receive-loop result.
    pub result: thread::Result<io::Result<()>>,
}

impl fmt::Debug for ReceiverOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("ReceiverOutcome");
        s.field("index", &self.index);
        match &self.result {
            Ok(result) => s.field("result", result),
            Err(_) => s.field("result", &"original panic payload retained"),
        };
        s.finish()
    }
}

/// Complete original outcomes after every created loop has been joined.
pub struct SessionOutcome {
    /// Original configuration, descriptor preparation or spawn error, if any.
    pub startup_error: Option<io::Error>,
    /// Every created receiver, in creation order; no join is skipped on failure.
    pub receivers: Vec<ReceiverOutcome>,
    /// Original filesystem destroy result or panic; called only after joins.
    pub cleanup: thread::Result<io::Result<()>>,
}

impl fmt::Debug for SessionOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("SessionOutcome");
        s.field("startup_error", &self.startup_error)
            .field("receivers", &self.receivers);
        match &self.cleanup {
            Ok(result) => s.field("cleanup", result),
            Err(_) => s.field("cleanup", &"original panic payload retained"),
        };
        s.finish()
    }
}

impl SessionOutcome {
    /// Whether startup, every receiver and filesystem cleanup all succeeded.
    pub fn is_clean(&self) -> bool {
        self.startup_error.is_none()
            && self
                .receivers
                .iter()
                .all(|r| matches!(r.result, Ok(Ok(()))))
            && matches!(self.cleanup, Ok(Ok(())))
    }
}

/// A single runnable Session and its observable receiver lifecycle.
///
/// The consuming run owns every created handle until its original join returns.
/// It can remain blocked while a receiver waits for kernel input: the mount owner
/// must perform its explicit detach/connection disposition. Observing Stopping or
/// joining an outer caller thread is never a replacement for these joins.
#[must_use = "retain the runner through its original run and join outcome"]
#[derive(Debug)]
pub struct SessionRunner<FS: Filesystem> {
    session: Session<FS>,
    monitor: SessionMonitor,
}

impl<FS: Filesystem> SessionRunner<FS> {
    pub(super) fn new(session: Session<FS>) -> Self {
        let monitor = SessionMonitor(Arc::new(Shared {
            state: Mutex::new(State {
                snapshot: SessionSnapshot {
                    phase: SessionPhase::Prepared,
                    revision: 0,
                    configured: session.config.n_threads.unwrap_or(1),
                    created: 0,
                    entered: 0,
                    exited: 0,
                    joined: 0,
                },
                spawning_done: false,
            }),
            changed: Condvar::new(),
        }));
        Self { session, monitor }
    }

    /// Retain a monitor before transferring the runner to its session owner.
    pub fn monitor(&self) -> SessionMonitor {
        self.monitor.clone()
    }

    /// Run once, retain every created receiver and consume every original join.
    /// No error path automatically detaches, aborts or retries the connection.
    pub fn run(self) -> SessionOutcome {
        let Self { session, monitor } = self;
        let Session {
            filesystem,
            ch,
            mount: _mount,
            allowed,
            session_owner,
            proto_version: _,
            config,
        } = session;
        let shared = &monitor.0;
        let n = config.n_threads.unwrap_or(1);
        {
            let mut state = shared.state.lock();
            state.snapshot.phase = SessionPhase::Starting;
            shared.changed(&mut state);
        }
        let mut filesystem = Arc::new(filesystem);
        let mut threads = Vec::new();
        let mut startup_error = None;
        if n == 0 || (!cfg!(target_os = "linux") && n != 1) {
            startup_error = Some(io::Error::other("unsupported receiver count"));
        } else {
            for index in 0..n {
                let channel = if config.clone_fd && index + 1 != n {
                    #[cfg(target_os = "linux")]
                    {
                        ch.clone_fd()
                    }
                    #[cfg(not(target_os = "linux"))]
                    {
                        Err(io::Error::other("clone_fd is only supported on Linux"))
                    }
                } else {
                    Ok(ch.clone())
                };
                let channel = match channel {
                    Ok(channel) => channel,
                    Err(error) => {
                        startup_error = Some(error);
                        break;
                    }
                };
                let event_loop = SessionEventLoop {
                    thread_name: format!("fuser-{index}"),
                    filesystem: filesystem.clone(),
                    ch: channel,
                    allowed,
                    session_owner,
                };
                let monitor = monitor.clone();
                match thread::Builder::new()
                    .name(event_loop.thread_name.clone())
                    .spawn(move || {
                        let guard = ReceiverGuard(monitor);
                        event_loop.event_loop_started(|| guard.enter())
                    }) {
                    Ok(thread) => {
                        threads.push((index, thread));
                        let mut state = shared.state.lock();
                        state.snapshot.created += 1;
                        shared.changed(&mut state);
                    }
                    Err(error) => {
                        startup_error = Some(error);
                        break;
                    }
                }
            }
        }
        if startup_error.is_some() {
            shared.stopping();
        } else {
            let mut state = shared.state.lock();
            state.spawning_done = true;
            shared.serving(&mut state);
        }
        let mut receivers = Vec::with_capacity(threads.len());
        for (index, thread) in threads {
            let result = thread.join();
            receivers.push(ReceiverOutcome { index, result });
            let mut state = shared.state.lock();
            state.snapshot.joined += 1;
            shared.changed(&mut state);
        }
        let cleanup = panic::catch_unwind(AssertUnwindSafe(|| {
            let holder: &mut FilesystemHolder<FS> = Arc::get_mut(&mut filesystem)
                .ok_or_else(|| io::Error::other("receiver still owns filesystem after joins"))?;
            holder.destroy();
            Ok(())
        }));
        {
            let mut state = shared.state.lock();
            state.snapshot.phase = SessionPhase::Joined;
            shared.changed(&mut state);
        }
        SessionOutcome {
            startup_error,
            receivers,
            cleanup,
        }
    }
}

struct ReceiverGuard(SessionMonitor);

impl ReceiverGuard {
    fn enter(&self) -> io::Result<()> {
        let shared = &self.0.0;
        let mut state = shared.state.lock();
        state.snapshot.entered += 1;
        shared.serving(&mut state);
        while state.snapshot.phase == SessionPhase::Starting {
            shared.changed.wait(&mut state);
        }
        if state.snapshot.phase == SessionPhase::Serving {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "session stopped before receiver service",
            ))
        }
    }
}

impl Drop for ReceiverGuard {
    fn drop(&mut self) {
        let shared = &self.0.0;
        let mut state = shared.state.lock();
        state.snapshot.exited += 1;
        state.snapshot.phase = SessionPhase::Stopping;
        shared.changed(&mut state);
    }
}
