//! Explicit session ownership and bounded callback drain.
use layerfs_workspace::Workspace;
use std::{fmt, io, time::Instant};

#[cfg(target_os = "linux")]
use {
    crate::adapter::Adapter,
    crate::range_ioctl::Stages,
    fuser::{Config, MountOption, Session, SessionACL, SessionUnmounter},
    layerfs_workspace::{
        CoherenceStatus, MountLease, MutationReceipt, WorkspaceAccess, MAX_READ_BYTES,
    },
    std::{
        ffi::OsStr,
        io::Read,
        os::unix::ffi::OsStrExt,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, Mutex,
        },
        thread::{self, JoinHandle},
        time::Duration,
    },
};

#[derive(Debug)]
pub enum MountError {
    Unsupported,
    Deadline,
    Io(io::Error),
    Workspace(layerfs_workspace::WorkspaceError),
    CleanupFailed,
}

impl fmt::Display for MountError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}
impl std::error::Error for MountError {}
impl From<io::Error> for MountError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}
impl From<layerfs_workspace::WorkspaceError> for MountError {
    fn from(error: layerfs_workspace::WorkspaceError) -> Self {
        Self::Workspace(error)
    }
}

/// The boundary at which a mount attempt failed. Cleanup is a separate operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MountPhase {
    Admission,
    Session,
    Binding,
    Worker,
    Deadline,
}

/// Preserves the original mount error and every admitted attempt's owner.
/// No mount failure automatically attempts cleanup. The caller must retain the
/// returned handle and explicitly unmount it before claiming resource release.
pub struct MountFailure {
    pub phase: MountPhase,
    pub cause: MountError,
    pub retained: Option<MountHandle>,
}

impl fmt::Debug for MountFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MountFailure")
            .field("phase", &self.phase)
            .field("cause", &self.cause)
            .field("retained", &self.retained.is_some())
            .finish()
    }
}
impl fmt::Display for MountFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "mount {:?}: {}", self.phase, self.cause)
    }
}
impl std::error::Error for MountFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}
impl MountFailure {
    #[cfg(target_os = "linux")]
    fn with_owner(
        mut self: Box<Self>,
        phase: MountPhase,
        cause: MountError,
        mut owner: MountHandle,
    ) -> Box<Self> {
        owner.stop_admission();
        self.phase = phase;
        self.cause = cause;
        self.retained = Some(owner);
        self
    }
}

/// Owns the mount until explicit successful unmount. A caller must retain this
/// value across failure. Dropping it does not clear Workspace mount admission;
/// a still-running session retains its Workspace and remains charged.
pub struct MountHandle {
    #[cfg(target_os = "linux")]
    workspace: Workspace,
    #[cfg(target_os = "linux")]
    lease: MountLease,
    #[cfg(target_os = "linux")]
    stopping: Arc<AtomicBool>,
    #[cfg(target_os = "linux")]
    stages: Arc<Stages>,
    #[cfg(target_os = "linux")]
    stage_sweeper: Option<JoinHandle<()>>,
    #[cfg(target_os = "linux")]
    unmounter: Option<SessionUnmounter>,
    #[cfg(target_os = "linux")]
    worker: Option<JoinHandle<io::Result<()>>>,
    #[cfg(target_os = "linux")]
    pending: Option<Arc<Mutex<Option<Session<Adapter>>>>>,
    #[cfg(target_os = "linux")]
    cleanup_failed: bool,
    #[cfg(target_os = "linux")]
    finished: bool,
}

/// Mounts the cached, read-only Linux projection. Local SDK edits complete
/// through the bound checked invalidator. No userspace content cache is added.
pub fn mount(workspace: &Workspace, deadline: Instant) -> Result<MountHandle, Box<MountFailure>> {
    mount_profile(workspace, deadline, false)
}

/// Mounts existing-file writes on a LocalEdit Workspace. Every regular open uses
/// direct I/O; shared writable mappings, writeback and sync are unsupported.
/// Size-only SETATTR supplies truncate/extend after kernel OPEN. Kernel syscalls
/// have deadline observation points, not preemption.
pub fn mount_writable(
    workspace: &Workspace,
    deadline: Instant,
) -> Result<MountHandle, Box<MountFailure>> {
    mount_profile(workspace, deadline, true)
}

fn mount_profile(
    workspace: &Workspace,
    deadline: Instant,
    writable: bool,
) -> Result<MountHandle, Box<MountFailure>> {
    // Reserve failure storage before native entry; an entered failure only moves
    // the existing owner into this box, never allocates a second result.
    let failure = Box::new(MountFailure {
        phase: MountPhase::Admission,
        cause: MountError::Unsupported,
        retained: None,
    });
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (workspace, deadline, writable);
        Err(failure)
    }
    #[cfg(target_os = "linux")]
    {
        let mut failure = failure;
        let admission = (|| -> Result<MountLease, MountError> {
            if Instant::now() >= deadline {
                return Err(MountError::Deadline);
            }
            if writable && workspace.access_mode() != WorkspaceAccess::LocalEdit {
                return Err(MountError::Workspace(
                    layerfs_workspace::WorkspaceError::ReadOnly,
                ));
            }
            privileged_owner(workspace)?;
            crate::replies::attributes(workspace.root(), workspace.root().serial)
                .map_err(|error| MountError::Io(io::Error::from_raw_os_error(error.code())))?;
            if Instant::now() >= deadline {
                return Err(MountError::Deadline);
            }
            Ok(workspace.reserve_mount()?)
        })();
        let lease = match admission {
            Ok(lease) => lease,
            Err(cause) => {
                failure.cause = cause;
                return Err(failure);
            }
        };
        // Only this cell crosses the outer thread spawn. If spawn fails, dropping
        // its closure cannot drop the Session still owned by the parent's Arc.
        let pending = Arc::new(Mutex::new(None));
        let stages = Arc::new(Stages::default());
        let mut handle = MountHandle {
            workspace: workspace.clone(),
            lease,
            stopping: Arc::new(AtomicBool::new(false)),
            stages: Arc::clone(&stages),
            stage_sweeper: None,
            unmounter: None,
            worker: None,
            pending: Some(Arc::clone(&pending)),
            cleanup_failed: false,
            finished: false,
        };
        let sweeper = match thread::Builder::new()
            .name("layerfs-ioctl-stages".into())
            .spawn({
                let stopping = Arc::clone(&handle.stopping);
                let stages = Arc::clone(&stages);
                move || {
                    while !stopping.load(Ordering::Acquire) {
                        stages.sweep();
                        thread::park_timeout(Duration::from_millis(100));
                    }
                    stages.clear();
                }
            }) {
            Ok(worker) => worker,
            Err(error) => return Err(failure.with_owner(MountPhase::Worker, error.into(), handle)),
        };
        handle.stage_sweeper = Some(sweeper);
        let adapter = Adapter {
            workspace: workspace.clone(),
            stopping: Arc::clone(&handle.stopping),
            writable,
            stages,
        };
        let mut config = Config::default();
        config.acl = SessionACL::Owner;
        config.n_threads = Some(2);
        config.clone_fd = false;
        config.mount_options = vec![
            if writable {
                MountOption::RW
            } else {
                MountOption::RO
            },
            MountOption::NoSuid,
            MountOption::NoDev,
            MountOption::DefaultPermissions,
            MountOption::Exec,
            MountOption::NoAtime,
            MountOption::FSName("layerfs".into()),
            MountOption::Subtype("layerfs".into()),
            MountOption::CUSTOM(format!("max_read={MAX_READ_BYTES}")),
        ];
        if Instant::now() >= deadline {
            return Err(failure.with_owner(MountPhase::Deadline, MountError::Deadline, handle));
        }
        let mut session = match Session::new(adapter, workspace.mount_path(), &config) {
            Ok(session) => session,
            Err(error) => {
                // The provider may have attempted cleanup internally. Keep our
                // lease until a later explicit unmount establishes absence.
                return Err(failure.with_owner(MountPhase::Session, error.into(), handle));
            }
        };
        handle.unmounter = Some(session.unmount_callable());
        let notifier = session.notifier();
        // This mutex protects only ownership transfer, never filesystem state.
        // Recover its value on poison so the native owner cannot be discarded.
        *pending.lock().unwrap_or_else(|error| error.into_inner()) = Some(session);
        if Instant::now() >= deadline {
            return Err(failure.with_owner(MountPhase::Deadline, MountError::Deadline, handle));
        }
        let root = workspace.root().serial;
        let invalidation = Arc::new(
            move |receipt: MutationReceipt, entry: Option<(u64, &[u8])>, deadline: Instant| {
                if let Some((parent, name)) = entry {
                    let parent = crate::replies::inode(parent, root);
                    notifier.inval_inode(parent, -1, 0)?;
                    if Instant::now() >= deadline {
                        return Err(io::ErrorKind::TimedOut.into());
                    }
                    notifier.inval_entry(parent, OsStr::from_bytes(name))
                } else {
                    notifier.inval_inode(crate::replies::inode(receipt.inode, root), 0, 0)
                }
            },
        );
        if let Err(error) = handle.lease.bind_invalidation(invalidation) {
            return Err(failure.with_owner(MountPhase::Binding, error.into(), handle));
        }
        if Instant::now() >= deadline {
            return Err(failure.with_owner(MountPhase::Deadline, MountError::Deadline, handle));
        }
        let worker = match thread::Builder::new()
            .name("layerfs-mount".into())
            .spawn(move || {
                let session = pending
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .take();
                drop(pending);
                session
                    .ok_or_else(|| io::Error::other("missing mount session"))?
                    .run()
            }) {
            Ok(worker) => worker,
            Err(error) => {
                return Err(failure.with_owner(MountPhase::Worker, error.into(), handle));
            }
        };
        handle.worker = Some(worker);
        drop(handle.pending.take());
        if Instant::now() >= deadline {
            return Err(failure.with_owner(MountPhase::Deadline, MountError::Deadline, handle));
        }
        Ok(handle)
    }
}

impl MountHandle {
    #[cfg(target_os = "linux")]
    fn stop_admission(&mut self) {
        self.stopping.store(true, Ordering::Release);
        self.stages.clear();
        if let Some(worker) = &self.stage_sweeper {
            worker.thread().unpark();
        }
        self.lease.stop_admission();
    }

    /// Stop admission, drain accepted operations and projection handles, detach once,
    /// then join. Local semantic handles may remain owned across unmount. A
    /// timeout keeps the worker/lease in this handle; failed detach is terminal
    /// and retains ownership rather than guessing that the mount disappeared.
    pub fn unmount(&mut self, deadline: Instant) -> Result<(), MountError> {
        #[cfg(not(target_os = "linux"))]
        {
            let _ = deadline;
            Err(MountError::Unsupported)
        }
        #[cfg(target_os = "linux")]
        {
            if self.finished {
                return Ok(());
            }
            if self.cleanup_failed {
                return Err(MountError::CleanupFailed);
            }
            if Instant::now() >= deadline {
                return Err(MountError::Deadline);
            }
            self.stop_admission();
            loop {
                let status = self.workspace.status()?;
                if status.active_operations == 0
                    && status.projection_handles == 0
                    && status.projection_replies == 0
                    && !matches!(status.coherence, Some(CoherenceStatus::Pending { .. }))
                {
                    break;
                }
                // A completed backend read does not close its caller's FD.
                // Keep release/flush callbacks live until the projection drains.
                pause(deadline)?;
            }
            if let Some(mut unmounter) = self.unmounter.take() {
                if Instant::now() >= deadline {
                    self.unmounter = Some(unmounter);
                    return Err(MountError::Deadline);
                }
                if let Err(error) = unmounter.unmount() {
                    self.cleanup_failed = true;
                    return Err(MountError::Io(error));
                }
            }
            if let Some(pending) = self.pending.take() {
                let session = pending
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .take();
                // The actual unmounter has consumed the provider mount. Release
                // the unstarted Session and its FD outside the transfer mutex.
                drop(session);
                drop(pending);
            }
            if let Some(worker) = &self.worker {
                while !worker.is_finished() {
                    pause(deadline)?;
                }
            }
            if let Some(worker) = self.worker.take() {
                match worker.join() {
                    Ok(Ok(())) => (),
                    Ok(Err(error)) => {
                        self.cleanup_failed = true;
                        return Err(MountError::Io(error));
                    }
                    Err(_) => {
                        self.cleanup_failed = true;
                        return Err(MountError::CleanupFailed);
                    }
                }
            }
            if let Some(worker) = self.stage_sweeper.take() {
                worker.join().map_err(|_| MountError::CleanupFailed)?;
            }
            if Instant::now() >= deadline {
                return Err(MountError::Deadline);
            }
            if mounted(self.workspace.mount_path())? {
                self.cleanup_failed = true;
                return Err(MountError::CleanupFailed);
            }
            if Instant::now() >= deadline {
                return Err(MountError::Deadline);
            }
            self.lease.finish()?;
            self.finished = true;
            Ok(())
        }
    }
}

#[cfg(target_os = "linux")]
fn pause(deadline: Instant) -> Result<(), MountError> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .ok_or(MountError::Deadline)?;
    thread::sleep(remaining.min(Duration::from_millis(1)));
    Ok(())
}

#[cfg(target_os = "linux")]
fn privileged_owner(workspace: &Workspace) -> Result<(), MountError> {
    if !cfg!(any(target_arch = "aarch64", target_arch = "x86_64")) {
        return Err(MountError::Unsupported);
    }
    let mut status = String::new();
    std::fs::File::open("/proc/self/status")?
        .take(65537)
        .read_to_string(&mut status)?;
    if status.len() > 65536 {
        return Err(MountError::Unsupported);
    }
    let uid = status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u32>().ok());
    let capabilities = status
        .lines()
        .find_map(|line| line.strip_prefix("CapEff:"))
        .and_then(|value| u64::from_str_radix(value.trim(), 16).ok());
    if uid != Some(0)
        || workspace.root().uid != 0
        || capabilities.is_none_or(|value| value & (1 << 21) == 0)
    {
        return Err(MountError::Unsupported);
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn mounted(path: &std::path::Path) -> Result<bool, MountError> {
    use std::os::unix::ffi::OsStrExt;
    let mut expected = Vec::new();
    for byte in path.as_os_str().as_bytes() {
        match byte {
            b' ' | b'\t' | b'\n' | b'\\' => expected.extend(format!("\\{byte:03o}").as_bytes()),
            _ => expected.push(*byte),
        }
    }
    let mut data = Vec::new();
    std::fs::File::open("/proc/self/mountinfo")?
        .take(1_048_577)
        .read_to_end(&mut data)?;
    if data.len() > 1_048_576 {
        return Err(MountError::CleanupFailed);
    }
    Ok(data
        .split(|byte| *byte == b'\n')
        .any(|line| line.split(|byte| *byte == b' ').nth(4) == Some(expected.as_slice())))
}
