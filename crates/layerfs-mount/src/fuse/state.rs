use super::translate::{file_type, system_time};
use crate::workspace::{ByteBudget, MountedAttr, MountedWorkspace};
use fuser::{FileAttr, INodeNo, Notifier};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant, UNIX_EPOCH};

pub(super) const TTL: Duration = Duration::from_secs(1);
pub(super) const O_TRUNC: i32 = 0o1000;
pub(super) const O_DSYNC: i32 = 0o10000;
pub(super) const O_SYNC: i32 = 0o4010000;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayerFuseEvent {
    Signal(i32),
    SessionEnded,
}

#[derive(Clone)]
pub struct SessionEndNotifier {
    sender: Arc<OnceLock<Sender<LayerFuseEvent>>>,
    sent: Arc<AtomicBool>,
}

impl SessionEndNotifier {
    pub fn notify(&self) {
        if let Some(sender) = self.sender.get() {
            if !self.sent.swap(true, Ordering::AcqRel) {
                let _ = sender.send(LayerFuseEvent::SessionEnded);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FuseCounters {
    pub init: u64,
    pub destroy: u64,
    pub lookup: u64,
    pub forget: u64,
    pub getattr: u64,
    pub setattr: u64,
    pub readlink: u64,
    pub mknod: u64,
    pub mkdir: u64,
    pub unlink: u64,
    pub rmdir: u64,
    pub symlink: u64,
    pub rename: u64,
    pub link: u64,
    pub open: u64,
    pub read: u64,
    pub write: u64,
    pub flush: u64,
    pub release: u64,
    pub fsync: u64,
    pub opendir: u64,
    pub readdir: u64,
    pub releasedir: u64,
    pub fsyncdir: u64,
    pub statfs: u64,
    pub access: u64,
    pub create: u64,
    pub callback_wall_ns: u64,
    pub mount_lock_wait_ns: u64,
    pub invalidations_requested: u64,
    pub invalidations_succeeded: u64,
    pub invalidations_failed: u64,
    pub invalidations_unsupported: u64,
}

pub struct LayerFuse {
    pub(super) workspace: Arc<Mutex<MountedWorkspace>>,
    pub(super) budget: Arc<ByteBudget>,
    counters: Arc<Mutex<FuseCounters>>,
    notifier: Arc<OnceLock<Notifier>>,
    pub(super) session_end: SessionEndNotifier,
    uid: u32,
    gid: u32,
}

impl LayerFuse {
    pub fn new(workspace: MountedWorkspace, uid: u32, gid: u32) -> Self {
        Self::from_shared(Arc::new(Mutex::new(workspace)), uid, gid)
    }

    pub fn from_shared(workspace: Arc<Mutex<MountedWorkspace>>, uid: u32, gid: u32) -> Self {
        let budget = workspace.lock().expect("new mount workspace").byte_budget();
        Self {
            workspace,
            budget,
            counters: Arc::new(Mutex::new(FuseCounters::default())),
            notifier: Arc::new(OnceLock::new()),
            session_end: SessionEndNotifier {
                sender: Arc::new(OnceLock::new()),
                sent: Arc::new(AtomicBool::new(false)),
            },
            uid,
            gid,
        }
    }

    pub fn shared_workspace(&self) -> Arc<Mutex<MountedWorkspace>> {
        self.workspace.clone()
    }

    pub fn shared_counters(&self) -> Arc<Mutex<FuseCounters>> {
        self.counters.clone()
    }

    pub fn notifier_slot(&self) -> Arc<OnceLock<Notifier>> {
        self.notifier.clone()
    }

    pub fn byte_budget(&self) -> Arc<ByteBudget> {
        self.budget.clone()
    }

    pub fn set_lifecycle_sender(
        &self,
        sender: Sender<LayerFuseEvent>,
    ) -> Result<(), Sender<LayerFuseEvent>> {
        self.session_end.sender.set(sender)
    }

    pub fn session_end_notifier(&self) -> SessionEndNotifier {
        self.session_end.clone()
    }

    pub(super) fn callback(&self) -> CallbackTimer<'_> {
        CallbackTimer {
            counters: &self.counters,
            started: Instant::now(),
        }
    }

    pub(super) fn lock(&self) -> Result<MutexGuard<'_, MountedWorkspace>, fuser::Errno> {
        let started = Instant::now();
        let result = self.workspace.lock().map_err(|_| fuser::Errno::EIO);
        if let Ok(mut counters) = self.counters.lock() {
            counters.mount_lock_wait_ns = counters
                .mount_lock_wait_ns
                .saturating_add(started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64);
        }
        result
    }

    pub(super) fn count(&self, update: impl FnOnce(&mut FuseCounters)) {
        if let Ok(mut counters) = self.counters.lock() {
            update(&mut counters);
        }
    }

    pub(super) fn attr(&self, value: MountedAttr) -> FileAttr {
        FileAttr {
            ino: INodeNo(value.node.0),
            size: value.size,
            blocks: value.size.div_ceil(512),
            atime: UNIX_EPOCH,
            mtime: system_time(value.mtime_seconds, value.mtime_nanoseconds),
            ctime: system_time(value.mtime_seconds, value.mtime_nanoseconds),
            crtime: UNIX_EPOCH,
            kind: file_type(value.kind),
            perm: value.mode as u16,
            nlink: value.links,
            uid: self.uid,
            gid: self.gid,
            rdev: 0,
            blksize: 4096,
            flags: 0,
        }
    }

    pub(super) fn reject_sync_flags(flags: i32) -> Result<(), fuser::Errno> {
        if flags & (O_SYNC | O_DSYNC) != 0 {
            Err(fuser::Errno::EOPNOTSUPP)
        } else {
            Ok(())
        }
    }
}

pub(super) struct CallbackTimer<'a> {
    counters: &'a Mutex<FuseCounters>,
    started: Instant,
}

impl Drop for CallbackTimer<'_> {
    fn drop(&mut self) {
        if let Ok(mut counters) = self.counters.lock() {
            counters.callback_wall_ns = counters
                .callback_wall_ns
                .saturating_add(self.started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64);
        }
    }
}
