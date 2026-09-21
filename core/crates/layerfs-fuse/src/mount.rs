//! Explicit session ownership and bounded callback drain.
use layerfs_workspace::Workspace;
use std::{fmt, io, time::Instant};

#[cfg(target_os = "linux")]
use {
    crate::adapter::{Adapter, CALLBACK_BUDGET},
    fuser::{Config, MountOption, Session, SessionACL, SessionUnmounter},
    layerfs_workspace::{
        CoherenceStatus, MountLease, MutationReceipt, WorkspaceAccess, MAX_READ_BYTES,
    },
    std::{
        io::Read,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
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
    unmounter: Option<SessionUnmounter>,
    #[cfg(target_os = "linux")]
    worker: Option<JoinHandle<io::Result<()>>>,
    #[cfg(target_os = "linux")]
    cleanup_failed: bool,
    #[cfg(target_os = "linux")]
    finished: bool,
}

/// Mounts the cached, read-only Linux projection. Local SDK edits complete
/// through the bound checked invalidator. No userspace content cache is added.
pub fn mount(workspace: &Workspace, deadline: Instant) -> Result<MountHandle, MountError> {
    mount_profile(workspace, deadline, false)
}

/// Mounts existing-file writes on a LocalEdit Workspace. Every regular open uses
/// direct I/O; shared writable mappings, writeback, sync and size SETATTR are
/// unsupported. Kernel syscalls have deadline observation points, not preemption.
pub fn mount_writable(workspace: &Workspace, deadline: Instant) -> Result<MountHandle, MountError> {
    mount_profile(workspace, deadline, true)
}

fn mount_profile(
    workspace: &Workspace,
    deadline: Instant,
    writable: bool,
) -> Result<MountHandle, MountError> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (workspace, deadline, writable);
        Err(MountError::Unsupported)
    }
    #[cfg(target_os = "linux")]
    {
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
        let mut lease = workspace.reserve_mount()?;
        let stopping = Arc::new(AtomicBool::new(false));
        let adapter = Adapter {
            workspace: workspace.clone(),
            stopping: Arc::clone(&stopping),
            writable,
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
        let mut session = match Session::new(adapter, workspace.mount_path(), &config) {
            Ok(session) => session,
            Err(error) => {
                // fuser attempted its normal cleanup. Only a checked absence can
                // release our lease; an uncertain/failed cleanup remains counted.
                if !mounted(workspace.mount_path())? {
                    lease.finish()?;
                }
                return Err(MountError::Io(error));
            }
        };
        let notifier = session.notifier();
        let root = workspace.root().serial;
        let invalidation = Arc::new(move |receipt: MutationReceipt, _: Instant| {
            notifier.inval_inode(crate::replies::inode(receipt.inode, root), 0, 0)
        });
        if let Err(error) = lease.bind_invalidation(invalidation) {
            drop(session);
            if !mounted(workspace.mount_path())? {
                lease.finish()?;
            }
            return Err(MountError::Workspace(error));
        }
        let unmounter = session.unmount_callable();
        let worker = match thread::Builder::new()
            .name("layerfs-mount".into())
            .spawn(move || session.run())
        {
            Ok(worker) => worker,
            Err(error) => {
                if !mounted(workspace.mount_path())? {
                    lease.finish()?;
                }
                return Err(MountError::Io(error));
            }
        };
        let mut handle = MountHandle {
            workspace: workspace.clone(),
            lease,
            stopping,
            unmounter: Some(unmounter),
            worker: Some(worker),
            cleanup_failed: false,
            finished: false,
        };
        if Instant::now() >= deadline {
            // No caller receives a successful mount after its attach deadline.
            // Cleanup has its declared callback-drain allowance, not a new mount.
            handle.unmount(Instant::now() + CALLBACK_BUDGET)?;
            return Err(MountError::Deadline);
        }
        Ok(handle)
    }
}

impl MountHandle {
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
            self.stopping.store(true, Ordering::Release);
            self.lease.stop_admission();
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
            if mounted(self.workspace.mount_path())? {
                self.cleanup_failed = true;
                return Err(MountError::CleanupFailed);
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
