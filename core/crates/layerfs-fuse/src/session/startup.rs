//! One attach attempt: direct mount, handshake and all-loop serving evidence.
use super::state::{
    AttachFailure, AttachPhase, AttachRemainder, DetachAttempt, KernelSession, SessionConfig,
    RECEIVE_LOOPS,
};
use crate::{
    attributes::Identity, mount::syscalls, ports::MountServices, request::KernelFilesystem,
    MountRequests,
};
use fuser::{Config, Session, SessionACL, SessionMonitor, SessionPhase};
use std::{fs, io, sync::Arc, thread, time::Duration};

impl KernelSession {
    /// Serving is returned only after the kernel mount exists, the handshake
    /// completed and every receive loop entered. Neither a successful spawn nor
    /// a filesystem probe substitutes for that evidence. A failure after the
    /// mount call makes exactly one detach attempt and reports what remains.
    pub fn attach(
        config: SessionConfig,
        queue: MountRequests,
        services: Arc<dyn MountServices>,
        identity: Identity,
    ) -> Result<Self, Box<AttachFailure>> {
        let unmounted = |phase, cause, directory: bool, queue: &MountRequests| {
            let directory = directory
                .then(|| fs::remove_dir(&config.directory).err())
                .flatten();
            let lane = queue.stop_receive().and_then(|()| queue.finish()).err();
            Box::new(AttachFailure {
                phase,
                cause,
                mounted: false,
                remainder: if directory.is_none() && lane.is_none() {
                    AttachRemainder::Disposed(None)
                } else {
                    AttachRemainder::Unmounted { directory, lane }
                },
            })
        };
        let filesystem = match KernelFilesystem::new(queue.clone(), services, identity) {
            Ok(filesystem) => filesystem,
            Err(error) => {
                return Err(unmounted(
                    AttachPhase::Filesystem,
                    io::Error::other(error),
                    false,
                    &queue,
                ))
            }
        };
        if let Err(cause) = syscalls::create_directory(&config.directory) {
            return Err(unmounted(AttachPhase::Directory, cause, false, &queue));
        }
        #[cfg(target_os = "linux")]
        let device = match syscalls::open_device() {
            Ok(device) => device,
            Err(cause) => return Err(unmounted(AttachPhase::Device, cause, true, &queue)),
        };
        #[cfg(target_os = "linux")]
        if let Err(errno) = syscalls::attach(
            &device,
            &config.directory,
            config.owner_uid,
            config.owner_gid,
        ) {
            return Err(unmounted(AttachPhase::Mount, errno.into(), true, &queue));
        }
        #[cfg(target_os = "macos")]
        let mount_path = match syscalls::resolve_mount_path(&config.directory) {
            Ok(path) => path,
            Err(cause) => return Err(unmounted(AttachPhase::Directory, cause, true, &queue)),
        };
        let negotiated = filesystem.negotiation();
        let mut session = Self {
            mount: queue.identity(),
            directory: config.directory.clone(),
            accounting: filesystem.accounting(),
            fence: queue.fence(),
            queue,
            negotiation: None,
            entry: None,
            abort: None,
            aborted: Arc::default(),
            forced_detach: DetachAttempt::NotAttempted,
            monitor: None,
            owner: None,
            watcher: None,
            outcome: None,
            detached: Arc::default(),
            drain_wait: config.drain_wait,
            invalidator: None,
            #[cfg(target_os = "macos")]
            channel: None,
        };
        #[cfg(target_os = "linux")]
        let kernel = {
            let mut options = Config::default();
            // The command identity differs from the mount owner; kernel permission
            // checks, not a library uid filter, decide access.
            options.acl = SessionACL::All;
            options.n_threads = Some(RECEIVE_LOOPS);
            // The session takes the only descriptor: losing it aborts the connection.
            match Session::from_fd(filesystem, device, SessionACL::All, options) {
                Ok(kernel) => kernel,
                Err(cause) => return Err(session.abandon(AttachPhase::Handshake, cause)),
            }
        };
        #[cfg(target_os = "macos")]
        let kernel = {
            let mut options = Config::default();
            options.acl = SessionACL::Owner;
            options.n_threads = Some(RECEIVE_LOOPS);
            let (fuser_fd, channel) =
                match attach_macos_channel(&mount_path, session.mount.owner_id()) {
                    Ok(pair) => pair,
                    Err(cause) => {
                        return Err(unmounted(AttachPhase::Mount, cause, true, &session.queue))
                    }
                };
            session.channel = Some(channel);
            match Session::from_fd(filesystem, fuser_fd, SessionACL::Owner, options) {
                Ok(kernel) => kernel,
                Err(cause) => {
                    if syscalls::mount_entry(&session.directory).is_ok() {
                        return Err(session.abandon(AttachPhase::Handshake, cause));
                    }
                    return Err(unmounted(AttachPhase::Mount, cause, true, &session.queue));
                }
            }
        };
        session.negotiation = negotiated.get().copied();
        session.invalidator = Some(super::Invalidator::new(kernel.notifier(), identity));
        let runner = kernel.into_runner();
        let monitor = runner.monitor();
        session.monitor = Some(monitor.clone());
        let owner_id = session.mount.owner_id();
        match thread::Builder::new()
            .name(format!("layerfs-mount-{owner_id}"))
            .spawn(move || runner.run())
        {
            Ok(owner) => session.owner = Some(owner),
            Err(cause) => {
                // The unspawned closure dropped the runner and its descriptor.
                session.monitor = None;
                return Err(session.abandon(AttachPhase::Spawn, cause));
            }
        }
        let fence = session.queue.clone();
        let watched = monitor.clone();
        match thread::Builder::new()
            .name(format!("layerfs-fence-{owner_id}"))
            .spawn(move || fence_first_exit(watched, fence))
        {
            Ok(watcher) => session.watcher = Some(watcher),
            Err(cause) => return Err(session.abandon(AttachPhase::Spawn, cause)),
        }
        let loops = monitor.wait_ready(config.ready_wait);
        if loops.phase != SessionPhase::Serving || loops.entered != RECEIVE_LOOPS {
            let cause = io::Error::other(format!("receive loops not serving: {loops:?}"));
            return Err(session.abandon(AttachPhase::Serving, cause));
        }
        if session.negotiation.is_none() {
            let cause = io::Error::other("completed handshake recorded no negotiation");
            return Err(session.abandon(AttachPhase::Receipt, cause));
        }
        #[cfg(target_os = "linux")]
        let entry = match syscalls::mount_entry(&session.directory) {
            Ok(entry) => entry,
            Err(cause) => return Err(session.abandon(AttachPhase::Receipt, cause)),
        };
        #[cfg(target_os = "macos")]
        let entry = match wait_macos_mount_entry(&session.directory, &monitor, config.ready_wait) {
            Ok(entry) => entry,
            Err(cause) => return Err(session.abandon(AttachPhase::Receipt, cause)),
        };
        match syscalls::abort_control(&entry) {
            Ok(abort) => session.abort = abort,
            Err(cause) => return Err(session.abandon(AttachPhase::Receipt, cause)),
        }
        session.entry = Some(entry);
        Ok(session)
    }
    /// The mount exists: one detach attempt, then the ordinary complete drain.
    fn abandon(mut self, phase: AttachPhase, cause: io::Error) -> Box<AttachFailure> {
        let remainder = match self.detach() {
            Ok(super::Detach::Detached) => match self.drain() {
                Ok(drained) => AttachRemainder::Disposed(Some(drained)),
                Err(undrained) => AttachRemainder::Retained(undrained),
            },
            Ok(super::Detach::Busy) | Err(_) => {
                AttachRemainder::Retained(Box::new(super::Undrained {
                    loops: self.monitor.as_ref().map(SessionMonitor::snapshot),
                    failed_demands: self.fence.failed_demands(),
                    session: self,
                    stage: super::DrainStage::Detach,
                    work: None,
                    lane: None,
                    panic: None,
                    forced: None,
                }))
            }
        };
        Box::new(AttachFailure {
            phase,
            cause,
            mounted: true,
            remainder,
        })
    }
}
/// The first receiver exit or unwind refuses later receives and wakes every
/// drain waiter. It depends on no request capacity, kernel
/// request or polling interval: only this session's own lifecycle event.
fn fence_first_exit(monitor: SessionMonitor, queue: MountRequests) {
    let mut loops = monitor.snapshot();
    while !matches!(loops.phase, SessionPhase::Stopping | SessionPhase::Joined) {
        loops = monitor.wait_for_change(loops.revision, Duration::MAX);
    }
    // A request owner already released by a completed drain has no waiter to wake.
    let _ = queue.stop_receive();
}
#[cfg(target_os = "macos")]
fn wait_macos_mount_entry(
    target: &std::path::Path,
    monitor: &SessionMonitor,
    ready_wait: Duration,
) -> io::Result<crate::mount::MountEntry> {
    let deadline = std::time::Instant::now() + ready_wait;
    loop {
        match syscalls::mount_entry(target) {
            Ok(entry) => return Ok(entry),
            Err(cause) => {
                if monitor.snapshot().phase != SessionPhase::Serving
                    || std::time::Instant::now() >= deadline
                {
                    return Err(cause);
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
}
#[cfg(target_os = "macos")]
fn attach_macos_channel(
    mount_path: &std::path::Path,
    owner_id: u64,
) -> io::Result<(
    std::os::fd::OwnedFd,
    (std::os::unix::net::UnixStream, std::os::unix::net::UnixStream),
)> {
    use nix::fcntl::{fcntl, FcntlArg, FdFlag};
    use nix::sys::socket::{setsockopt, sockopt};
    use std::io::{Read, Write};
    use std::net::Shutdown;
    use std::os::fd::{AsRawFd, OwnedFd};
    use std::os::unix::net::UnixStream;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::sync::{Condvar, Mutex};

    let mut candidates = vec![
        PathBuf::from("/Library/Application Support/fuse-t/bin/go-nfsv4"),
        PathBuf::from("/usr/local/bin/go-nfsv4"),
    ];
    if let Ok(home) = std::env::var("HOME") {
        candidates.push(PathBuf::from(home).join(".fuse-t/usr/local/bin/go-nfsv4"));
    }
    let bin = candidates
        .into_iter()
        .find(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from("go-nfsv4"));

    let (comm_parent, comm_child) = UnixStream::pair()?;
    let (mut mon_parent, mon_child) = UnixStream::pair()?;
    let (fuser_end, proxy_end) = UnixStream::pair()?;
    let buf_size = 4 * 1024 * 1024;
    for sock in [&comm_parent, &comm_child, &fuser_end, &proxy_end] {
        let _ = setsockopt(sock, sockopt::RcvBuf, &buf_size);
        let _ = setsockopt(sock, sockopt::SndBuf, &buf_size);
    }
    fcntl(&comm_child, FcntlArg::F_SETFD(FdFlag::empty())).map_err(io::Error::from)?;
    fcntl(&mon_child, FcntlArg::F_SETFD(FdFlag::empty())).map_err(io::Error::from)?;

    let mut daemon = Command::new(bin)
        .arg(format!("--volname=layerfs-{}-{owner_id}", std::process::id()))
        .arg("--namedattr=true")
        .arg("--dontbrowse=true")
        .arg("--noatime=true")
        .arg(mount_path)
        .env("_FUSE_COMMFD", comm_child.as_raw_fd().to_string())
        .env("_FUSE_MONFD", mon_child.as_raw_fd().to_string())
        .env("_FUSE_COMMVERS", "2")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    drop(comm_child);
    drop(mon_child);
    mon_parent.write_all(b"mount")?;

    let abort_comm = comm_parent.try_clone()?;
    let abort_mon = mon_parent.try_clone()?;
    let mut comm_read = comm_parent.try_clone()?;
    let mut comm_write = comm_parent;
    let mut proxy_read = proxy_end.try_clone()?;
    let mut proxy_write = proxy_end;
    let sync_pair = Arc::new((Mutex::new(false), Condvar::new()));
    let sync_req = Arc::clone(&sync_pair);

    thread::Builder::new()
        .name(format!("layerfs-fuset-req-{owner_id}"))
        .spawn(move || {
            let _mon = mon_parent;
            let mut buf = vec![0u8; 4 * 1024 * 1024];
            loop {
                if comm_read.read_exact(&mut buf[..40]).is_err() {
                    break;
                }
                let len = u32::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
                let opcode = u32::from_ne_bytes([buf[4], buf[5], buf[6], buf[7]]);
                if !(40..=buf.len()).contains(&len) {
                    break;
                }
                if len > 40 && comm_read.read_exact(&mut buf[40..len]).is_err() {
                    break;
                }
                let expects_reply = opcode != 2 && opcode != 42;
                let (lock, cvar) = &*sync_req;
                if expects_reply {
                    *lock.lock().unwrap() = true;
                }
                if proxy_write.write_all(&buf[..len]).is_err() {
                    break;
                }
                if expects_reply {
                    let mut in_flight = lock.lock().unwrap();
                    while *in_flight {
                        in_flight = cvar.wait(in_flight).unwrap();
                    }
                } else {
                    thread::sleep(Duration::from_millis(1));
                }
            }
            let _ = proxy_write.shutdown(Shutdown::Both);
            let _ = daemon.wait();
        })?;

    thread::Builder::new()
        .name(format!("layerfs-fuset-rsp-{owner_id}"))
        .spawn(move || {
            let mut buf = vec![0u8; 4 * 1024 * 1024];
            loop {
                if proxy_read.read_exact(&mut buf[..16]).is_err() {
                    break;
                }
                let len = u32::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
                let unique = u64::from_ne_bytes([
                    buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
                ]);
                if !(16..=buf.len()).contains(&len) {
                    break;
                }
                if len > 16 && proxy_read.read_exact(&mut buf[16..len]).is_err() {
                    break;
                }
                if comm_write.write_all(&buf[..len]).is_err() {
                    break;
                }
                if unique != 0 {
                    let (lock, cvar) = &*sync_pair;
                    let mut in_flight = lock.lock().unwrap();
                    *in_flight = false;
                    cvar.notify_all();
                }
            }
            let _ = comm_write.shutdown(Shutdown::Both);
        })?;

    Ok((OwnedFd::from(fuser_end), (abort_comm, abort_mon)))
}
