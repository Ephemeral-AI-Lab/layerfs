//! One namespace Sandbox: its directory, the init stage that owns its
//! namespaces, its cgroup and its exact attempts. Nothing is retried and no
//! Drop cleans up; a Drop only reaps an init process this process started
//! that has already exited, so it never stays a zombie of this process.
use super::{cgroup, host, paths::Paths, rootfs::ImageRoot};
use crate::backend::native::LocalProcess;
use crate::provider::{LocalRecord, NamespaceRecord, SandboxRecord};
use crate::{ContainerId, RuntimeError, WireFailure};
use std::{
    fs::{self, OpenOptions},
    io::{self, Read, Seek, SeekFrom},
    net::{Ipv4Addr, SocketAddr},
    os::unix::fs::OpenOptionsExt,
    path::Path,
    process::Child,
    sync::Mutex,
    time::{Duration, Instant},
};

/// The recorded process facts of a Sandbox, as a live process to check.
pub(super) fn process(record: &LocalRecord) -> LocalProcess {
    LocalProcess {
        pid: record.pid,
        start: record.start_time,
        boot: record.boot,
    }
}

/// The facts a record carries into a reopen: the init stage it saw, the port
/// it listened on, its labels and the image its root is an overlay of.
pub(crate) struct Recorded {
    pub(crate) init: LocalProcess,
    pub(crate) port: u16,
    pub(crate) labels: Vec<(String, String)>,
    pub(crate) image: ImageRoot,
}

/// Pause between two looks at a killed init process.
const PAUSE: Duration = Duration::from_millis(20);
/// Wait for the group's processes to be gone after `cgroup.kill`.
const KILL_WAIT: Duration = Duration::from_secs(5);

/// One acknowledged namespace Sandbox: the init stage of its namespaces, its
/// listener and the cgroup that owns everything it started.
#[derive(Debug)]
pub struct Sandbox {
    pub(super) id: ContainerId,
    pub(super) paths: Paths,
    pub(super) cgroup: std::path::PathBuf,
    pub(super) labels: Vec<(String, String)>,
    /// The helper executable this backend's init and exec stages run.
    pub(super) helper: std::path::PathBuf,
    /// The unpacked image its root is an overlay of, and its digest.
    pub(super) image: ImageRoot,
    /// The init stage this process started, kept so its exit is reaped here.
    /// `None` after a reopen.
    pub(super) child: Mutex<Option<Child>>,
    pub(super) init: Option<LocalProcess>,
    pub(super) port: u16,
    pub(super) stop_attempted: bool,
    pub(super) stopped: bool,
    pub(super) delete_attempted: bool,
    pub(super) deleted: bool,
}

impl Sandbox {
    /// The owner a start returns once its init stage is spawned and its
    /// cgroup exists; the listener port follows.
    pub(super) fn unstarted(
        id: ContainerId,
        paths: Paths,
        cgroup: std::path::PathBuf,
        labels: Vec<(String, String)>,
        helper: std::path::PathBuf,
        image: ImageRoot,
    ) -> Self {
        Self {
            id,
            paths,
            cgroup,
            labels,
            helper,
            image,
            child: Mutex::new(None),
            init: None,
            port: 0,
            stop_attempted: false,
            stopped: false,
            delete_attempted: false,
            deleted: false,
        }
    }

    /// Reopens a recorded Sandbox whose init stage still runs: the same
    /// process (id, start time and boot), in its own cgroup, and its loopback
    /// port. It starts nothing.
    pub(crate) fn reopen(
        paths: Paths,
        cgroup: std::path::PathBuf,
        id: ContainerId,
        helper: std::path::PathBuf,
        recorded: Recorded,
    ) -> Result<Self, Box<WireFailure>> {
        let Recorded {
            init,
            port,
            labels,
            image,
        } = recorded;
        let mut owner = Self::unstarted(id, paths, cgroup, labels, helper, image);
        owner.init = Some(init);
        owner.port = port;
        if !owner.paths.local.directory.is_absolute() || port == 0 {
            return Err(owner.invalid("saved namespaces Sandbox"));
        }
        if !init.alive() || !host::in_cgroup(init.pid, &owner.cgroup) {
            let mut failure = owner.invalid(crate::types::STOPPED);
            failure.observed_container = Some(owner.id);
            return Err(failure);
        }
        owner.endpoint()?;
        Ok(owner)
    }

    /// A recorded Sandbox whose init stage is gone
    /// ([`WireFailure::container_stopped`]): only its one Delete is admitted.
    pub(crate) fn stopped(
        paths: Paths,
        cgroup: std::path::PathBuf,
        id: ContainerId,
        helper: std::path::PathBuf,
        recorded: Recorded,
    ) -> Self {
        let mut owner = Self::unstarted(id, paths, cgroup, recorded.labels, helper, recorded.image);
        owner.init = Some(recorded.init);
        owner.port = recorded.port;
        owner.stop_attempted = true;
        owner.stopped = true;
        owner
    }

    /// Its identity: random at create, recorded, never an Engine's.
    pub fn identity(&self) -> ContainerId {
        self.id
    }

    /// Its private directory (setup, executable, overlay, mounts, log).
    pub fn directory(&self) -> &Path {
        &self.paths.local.directory
    }

    /// Its commands' scratch directory (1777), deleted with it.
    pub fn tmp(&self) -> std::path::PathBuf {
        self.paths.local.tmp.clone()
    }

    /// Its init stage process, once started.
    pub fn process(&self) -> Option<LocalProcess> {
        self.init
    }

    /// The loopback port its daemon listens on, once observed.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The labels it was created with, kept in its record.
    pub fn labels(&self) -> &[(String, String)] {
        &self.labels
    }

    /// The image its root is an overlay of.
    pub fn image(&self) -> &ImageRoot {
        &self.image
    }

    /// The facts its record keeps, for reopening this exact Sandbox later.
    pub fn local_record(&self) -> Option<LocalRecord> {
        let init = self.process()?;
        Some(LocalRecord {
            name: self.directory().file_name()?.to_str()?.to_owned(),
            pid: init.pid,
            start_time: init.start,
            boot: init.boot,
            port: self.port(),
            labels: self.labels().to_vec(),
        })
    }

    /// Its record, naming this backend so another one never reopens it, and
    /// the image digest so a reopen finds the same root again.
    pub fn record(&self) -> Option<SandboxRecord> {
        let local = self.local_record()?;
        Some(SandboxRecord::Namespaces(NamespaceRecord {
            local,
            image: self.image.image().clone(),
        }))
    }

    /// The daemon's endpoint while its init stage runs. A Sandbox whose init
    /// stage exited (idle close, crash, kill) is the stopped observation
    /// ([`WireFailure::container_stopped`]): its Workspaces went with it.
    pub fn endpoint(&self) -> Result<SocketAddr, Box<WireFailure>> {
        let Some(init) = self.init else {
            return Err(self.invalid("running endpoint admission"));
        };
        if self.port == 0 || self.stop_attempted || self.delete_attempted {
            return Err(self.invalid("running endpoint admission"));
        }
        self.reap();
        if !init.alive() {
            let mut failure = self.invalid(crate::types::STOPPED);
            failure.observed_container = Some(self.id);
            return Err(failure);
        }
        Ok(SocketAddr::from((Ipv4Addr::LOCALHOST, self.port)))
    }

    /// Ends the Sandbox once: one write to `cgroup.kill` asks the kernel to
    /// `SIGKILL` every process of its group, descendants and commands
    /// included, and no graceful drain exists to wait for. A Sandbox whose
    /// init stage already ended is the same stopped state.
    pub fn stop(&mut self, timeout_seconds: u16) -> Result<(), Box<WireFailure>> {
        if self.stop_attempted || self.delete_attempted {
            return Err(self.invalid("original namespaces Stop admission"));
        }
        self.stop_attempted = true;
        cgroup::kill(&self.cgroup).map_err(|error| self.io(error))?;
        let wait = Duration::from_secs(u64::from(timeout_seconds)).max(KILL_WAIT);
        let deadline = Instant::now() + wait;
        loop {
            self.reap();
            if self.init.is_none_or(|init| !init.alive()) {
                match cgroup::populated(&self.cgroup) {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => break,
                    Err(error) => return Err(self.io(error)),
                }
            }
            if Instant::now() >= deadline {
                return Err(
                    self.invalid("a process of the namespaces Sandbox runs after cgroup.kill")
                );
            }
            std::thread::sleep(PAUSE);
        }
        self.stopped = true;
        Ok(())
    }

    /// Deletes its cgroup and its directory once, after a known stop: a
    /// group that still holds a process is refused, so nothing is reported
    /// gone while it runs. The private root it made is a mount of the
    /// Sandbox's own namespaces and is gone with them. The shared Store is
    /// outside the directory and is never touched.
    pub fn delete(&mut self) -> Result<(), Box<WireFailure>> {
        if self.delete_attempted || (self.init.is_some() && !self.stopped) {
            return Err(self.invalid("original namespaces Delete admission; known stop required"));
        }
        self.delete_attempted = true;
        self.reap();
        if self.init.is_some_and(LocalProcess::alive) {
            return Err(self.invalid("namespaces Sandbox init runs again; nothing was deleted"));
        }
        cgroup::remove(&self.cgroup).map_err(|error| self.io(error))?;
        match fs::remove_dir_all(self.directory()) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(self.io(error)),
        }
        // the private root holds nothing once its namespaces ended
        match fs::remove_dir(&self.paths.root) {
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
                .open(&self.paths.local.log)?;
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

    /// Nothing to name: `cgroup.kill` owns every process the Sandbox started,
    /// including one a command left in a session of its own.
    pub fn track(&self, _pid: u32, _start_time: u64) -> Result<(), Box<WireFailure>> {
        Ok(())
    }

    /// Reaps the init stage this process started, if it exited. True when no
    /// init stage this process started is left unreaped.
    pub(crate) fn reap(&self) -> bool {
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
    /// An init stage this process started and that has exited is reaped, so a
    /// Sandbox dropped on any path leaves no zombie of it. A running one is
    /// left alone: ending it is [`Sandbox::stop`]'s, once.
    fn drop(&mut self) {
        self.reap();
    }
}
