//! One local Sandbox: its directory, its daemon process and its exact
//! attempts. No Drop cleanup or replay, as for a container; a Drop only
//! reaps a daemon this process started that has already exited. A stop
//! returns only once a daemon this process started is reaped, so it never
//! stays a zombie of this process.
use super::{groups, mounts, process::LocalProcess};
use crate::{ContainerId, RuntimeError, WireFailure};
use nix::sys::signal::Signal;
use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Seek, SeekFrom},
    net::{Ipv4Addr, SocketAddr},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    process::Child,
    sync::Mutex,
    time::{Duration, Instant},
};

/// The daemon's output file in a Sandbox directory, stdout and stderr together.
pub(super) const LOG: &str = "daemon.log";
/// Pause between two looks at a stopping process.
const PAUSE: Duration = Duration::from_millis(20);
/// Wait for a killed process to be gone.
const KILL_WAIT: Duration = Duration::from_secs(5);

/// One acknowledged local Sandbox directory and, once started, its daemon.
#[derive(Debug)]
pub struct Sandbox {
    pub(super) id: ContainerId,
    pub(super) directory: PathBuf,
    pub(super) labels: Vec<(String, String)>,
    pub(super) process: Option<LocalProcess>,
    pub(super) port: u16,
    /// The daemon this process started, kept so its exit is reaped here and
    /// it does not stay a zombie of this process. `None` after a reopen.
    pub(super) child: Mutex<Option<Child>>,
    pub(super) stop_attempted: bool,
    pub(super) stopped: bool,
    /// Its process groups were killed and found empty (by its stop).
    pub(super) ended: bool,
    pub(super) delete_attempted: bool,
    pub(super) deleted: bool,
}

impl Sandbox {
    /// A directory made by a start that has not spawned its daemon yet.
    pub(super) fn unstarted(
        id: ContainerId,
        directory: PathBuf,
        labels: Vec<(String, String)>,
    ) -> Self {
        Self {
            id,
            directory,
            labels,
            process: None,
            port: 0,
            child: Mutex::new(None),
            stop_attempted: false,
            stopped: false,
            ended: false,
            delete_attempted: false,
            deleted: false,
        }
    }
    /// Reopens a recorded Sandbox whose daemon still runs: the same process
    /// (id and start time) and its loopback port. It starts nothing.
    pub fn reopen(
        directory: PathBuf,
        id: ContainerId,
        process: LocalProcess,
        port: u16,
        labels: Vec<(String, String)>,
    ) -> Result<Self, Box<WireFailure>> {
        let mut owner = Self::unstarted(id, directory, labels);
        owner.process = Some(process);
        owner.port = port;
        if !owner.directory.is_absolute() || port == 0 {
            return Err(owner.invalid("saved local Sandbox"));
        }
        owner.endpoint()?;
        Ok(owner)
    }
    /// A recorded Sandbox that [`Self::reopen`] found stopped
    /// ([`WireFailure::container_stopped`]): only its one Delete is admitted.
    pub fn stopped(
        directory: PathBuf,
        id: ContainerId,
        process: LocalProcess,
        port: u16,
        labels: Vec<(String, String)>,
    ) -> Self {
        let mut owner = Self::unstarted(id, directory, labels);
        owner.process = Some(process);
        owner.port = port;
        owner.stop_attempted = true;
        owner.stopped = true;
        owner
    }
    /// Its identity: random at create, recorded, never an Engine's.
    pub fn identity(&self) -> ContainerId {
        self.id
    }
    /// Its private directory (setup, overlay, Workspace mounts, daemon log).
    pub fn directory(&self) -> &Path {
        &self.directory
    }
    /// Its commands' scratch directory (1777), deleted with it.
    pub fn tmp(&self) -> PathBuf {
        self.directory.join("tmp")
    }
    /// Its daemon process, once started.
    pub fn process(&self) -> Option<LocalProcess> {
        self.process
    }
    /// The loopback port its daemon listens on, once observed.
    pub fn port(&self) -> u16 {
        self.port
    }
    /// The labels it was created with, kept in its record.
    pub fn labels(&self) -> &[(String, String)] {
        &self.labels
    }
    /// The daemon's endpoint while that exact process runs. A daemon that
    /// exited (idle close, crash, kill) is the stopped observation
    /// ([`WireFailure::container_stopped`]): its Workspaces went with it.
    pub fn endpoint(&self) -> Result<SocketAddr, Box<WireFailure>> {
        let Some(process) = self.process else {
            return Err(self.invalid("running endpoint admission"));
        };
        if self.port == 0 || self.stop_attempted || self.delete_attempted {
            return Err(self.invalid("running endpoint admission"));
        }
        self.reap();
        if !process.alive() {
            let mut failure = self.invalid(crate::types::STOPPED);
            failure.observed_container = Some(self.id);
            return Err(failure);
        }
        Ok(SocketAddr::from((Ipv4Addr::LOCALHOST, self.port)))
    }
    /// Stops the daemon once: TERM, up to `seconds` for it to end, then
    /// KILL (a daemon this process started has ended once reaped); then
    /// KILL to every process group of its commands and its daemon, as a
    /// container's end kills what ran in it. A daemon already gone is the
    /// same stopped state. Crash teardown, not a graceful filesystem drain.
    pub fn stop(&mut self, seconds: u16) -> Result<(), Box<WireFailure>> {
        if self.stop_attempted || self.delete_attempted {
            return Err(self.invalid("original local Stop admission"));
        }
        self.stop_attempted = true;
        if let Some(process) = self.process {
            self.end(
                process,
                Signal::SIGTERM,
                Duration::from_secs(u64::from(seconds)),
            )?;
            if !self.gone(process) {
                self.end(process, Signal::SIGKILL, KILL_WAIT)?;
            }
            if process.alive() {
                return Err(self.invalid("local daemon still runs after KILL"));
            }
        }
        self.stopped = true;
        groups::end(&self.directory, self.process, KILL_WAIT).map_err(|error| self.io(error))?;
        self.ended = true;
        Ok(())
    }
    /// Deletes its directory once, after a known stop: any process left in
    /// its groups is killed first (when its stop did not run here, as for a
    /// daemon found stopped), and any mount its daemon left is detached, so
    /// nothing writes into it or is removed through a mount. The shared
    /// Store is outside it and never touched.
    pub fn delete(&mut self) -> Result<(), Box<WireFailure>> {
        if self.delete_attempted || (self.process.is_some() && !self.stopped) {
            return Err(self.invalid("original local Delete admission; known stop required"));
        }
        self.delete_attempted = true;
        self.reap();
        if self.process.is_some_and(LocalProcess::alive) {
            return Err(self.invalid("local daemon runs again; nothing was deleted"));
        }
        if !self.ended {
            groups::end(&self.directory, self.process, KILL_WAIT)
                .map_err(|error| self.io(error))?;
            self.ended = true;
        }
        mounts::detach(&self.directory).map_err(|error| self.io(error))?;
        match fs::remove_dir_all(&self.directory) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(self.io(error)),
        }
        self.deleted = true;
        Ok(())
    }
    /// The daemon output's last `lines` lines, keeping only the final `cap`
    /// bytes. Read-only and never retried.
    pub fn log_tail(&self, lines: u16, cap: usize) -> Result<Vec<u8>, Box<WireFailure>> {
        if self.delete_attempted {
            return Err(self.invalid("log read before Delete"));
        }
        let read = || -> io::Result<Vec<u8>> {
            let mut file = OpenOptions::new()
                .read(true)
                .custom_flags(nix::fcntl::OFlag::O_NOFOLLOW.bits())
                .open(self.directory.join(LOG))?;
            let length = file.metadata()?.len();
            // a line is rarely longer than this; the window bounds the read
            let window = (cap as u64).saturating_mul(4).max(64 * 1024);
            file.seek(SeekFrom::Start(length.saturating_sub(window)))?;
            let mut bytes = Vec::new();
            file.take(window).read_to_end(&mut bytes)?;
            Ok(bytes)
        };
        let bytes = read().map_err(|error| self.io(error))?;
        let body = bytes.strip_suffix(b"\n").unwrap_or(&bytes);
        let start = (body.iter().enumerate().rev())
            .filter(|(_, b)| **b == b'\n')
            .nth(usize::from(lines).saturating_sub(1))
            .map_or(0, |(i, _)| i + 1);
        let tail = &bytes[start..];
        Ok(tail[tail.len().saturating_sub(cap)..].to_vec())
    }
    /// Names the process group `pid` leads or is about to lead: a process
    /// started at `start` by a command of this Sandbox in a session of its
    /// own (the SDK's `setsid` background process), so it ends with the
    /// Sandbox too. One already gone needs nothing.
    pub fn track(&self, pid: u32, start: u64) -> Result<(), Box<WireFailure>> {
        match LocalProcess::observe(pid) {
            Ok(process) if process.start == start && process.alive() => {
                groups::name(&self.directory, process).map_err(|error| self.io(error))
            }
            _ => Ok(()),
        }
    }
    /// Reaps the daemon this process started, if it exited. True when no
    /// daemon this process started is left unreaped.
    pub(super) fn reap(&self) -> bool {
        let mut child = self.child.lock().unwrap_or_else(|error| error.into_inner());
        let Some(started) = child.as_mut() else {
            return true;
        };
        let reaped = matches!(started.try_wait(), Ok(Some(_)));
        if reaped {
            *child = None;
        }
        reaped
    }
    /// The daemon has ended and, if this process started it, is reaped. Its
    /// main thread can end (a zombie id, so not alive) while its other
    /// threads still run and it cannot be reaped yet: not gone.
    fn gone(&self, process: LocalProcess) -> bool {
        self.reap() && !process.alive()
    }
    /// Sends `signal` once and waits up to `wait` for the process to be gone.
    /// A KILL also goes to the daemon this process started and has not
    /// reaped: its id cannot be reused until reaped, so it is still the one
    /// started, though its main thread may have ended already.
    fn end(
        &self,
        process: LocalProcess,
        signal: Signal,
        wait: Duration,
    ) -> Result<(), Box<WireFailure>> {
        process.signal(signal).map_err(|error| self.io(error))?;
        if signal == Signal::SIGKILL {
            let mut child = self.child.lock().unwrap_or_else(|error| error.into_inner());
            if let Some(started) = child.as_mut() {
                let _ = started.kill();
            }
        }
        let deadline = Instant::now() + wait;
        loop {
            if self.gone(process) || Instant::now() >= deadline {
                return Ok(());
            }
            std::thread::sleep(PAUSE);
        }
    }
    pub(super) fn invalid(&self, message: &'static str) -> Box<WireFailure> {
        self.failure(RuntimeError::Protocol(message))
    }
    pub(super) fn io(&self, error: io::Error) -> Box<WireFailure> {
        self.failure(RuntimeError::Io(error))
    }
    pub(super) fn failure(&self, cause: RuntimeError) -> Box<WireFailure> {
        Box::new(WireFailure {
            attempted: false,
            sent_bytes: 0,
            request_header_bytes: 0,
            sent_body_bytes: 0,
            pending_request: Default::default(),
            status: None,
            cause,
            fence_error: None,
            observed_exec: None,
            observed_container: None,
            requested_container: Some(self.id),
            requested_exec: None,
            error_body: None,
        })
    }
}

impl Drop for Sandbox {
    /// A daemon this process started and that has exited is reaped, so a
    /// Sandbox dropped on any path leaves no zombie of it. A running daemon
    /// is left alone: stopping it is [`Sandbox::stop`]'s, once.
    fn drop(&mut self) {
        self.reap();
    }
}
