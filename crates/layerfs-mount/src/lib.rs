#![cfg(target_os = "linux")]

mod driver;
pub mod workspace;

pub use driver::MountDriver;

use crate::workspace::{
    ByteBudget, MountedAttr, MountedError, MountedFileType, MountedHandleId, MountedNodeId,
    MountedWorkspace, MAX_REQUEST_BYTES, ROOT_NODE,
};
use fuser::{
    AccessFlags, BsdFileFlags, FileAttr, FileHandle, FileType, Filesystem, FopenFlags, Generation,
    INodeNo, KernelConfig, LockOwner, Notifier, OpenFlags, RenameFlags, ReplyAttr, ReplyCreate,
    ReplyData, ReplyDirectory, ReplyEmpty, ReplyEntry, ReplyOpen, ReplyStatfs, ReplyWrite, Request,
    TimeOrNow, WriteFlags,
};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const TTL: Duration = Duration::from_secs(1);
const O_TRUNC: i32 = 0o1000;
const O_DSYNC: i32 = 0o10000;
const O_SYNC: i32 = 0o4010000;
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
    workspace: Arc<Mutex<MountedWorkspace>>,
    budget: Arc<ByteBudget>,
    checkpoint: Mutex<()>,
    counters: Arc<Mutex<FuseCounters>>,
    notifier: Arc<OnceLock<Notifier>>,
    session_end: SessionEndNotifier,
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
            checkpoint: Mutex::new(()),
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

    fn callback(&self) -> CallbackTimer<'_> {
        CallbackTimer {
            counters: &self.counters,
            started: Instant::now(),
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, MountedWorkspace>, fuser::Errno> {
        let started = Instant::now();
        let result = self.workspace.lock().map_err(|_| fuser::Errno::EIO);
        if let Ok(mut counters) = self.counters.lock() {
            counters.mount_lock_wait_ns = counters
                .mount_lock_wait_ns
                .saturating_add(started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64);
        }
        result
    }

    fn count(&self, update: impl FnOnce(&mut FuseCounters)) {
        if let Ok(mut counters) = self.counters.lock() {
            update(&mut counters);
        }
    }

    fn attr(&self, value: MountedAttr) -> FileAttr {
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

    fn reject_sync_flags(flags: i32) -> Result<(), fuser::Errno> {
        if flags & (O_SYNC | O_DSYNC) != 0 {
            Err(fuser::Errno::EOPNOTSUPP)
        } else {
            Ok(())
        }
    }

    fn checkpoint_if_needed(&self) -> Result<(), fuser::Errno> {
        if !self.lock()?.automatic_checkpoint_needed() {
            return Ok(());
        }

        // ponytail: one mount-wide gate; shard only if checkpoint contention is measurable.
        let _checkpoint = self.checkpoint.lock().map_err(|_| fuser::Errno::EIO)?;
        if !self.lock()?.automatic_checkpoint_needed() {
            return Ok(());
        }
        let result = self.budget.pause_and_wait().map_err(errno).and_then(|()| {
            self.lock()
                .and_then(|mut workspace| workspace.checkpoint().map_err(errno))
        });
        self.budget.resume();
        result.map(|_| ())
    }

    fn finish_mutation<T>(&self, result: Result<T, fuser::Errno>) -> Result<T, fuser::Errno> {
        let value = result?;
        if self.checkpoint_if_needed().is_err() {
            if let Ok(mut workspace) = self.workspace.lock() {
                workspace.mark_incomplete();
            }
        }
        Ok(value)
    }

    fn release_handle(&self, handle: MountedHandleId) -> Result<(), fuser::Errno> {
        let result = self
            .lock()
            .and_then(|mut workspace| workspace.release(handle).map_err(errno));
        self.finish_mutation(result)
    }

    fn explicit_checkpoint(&self, directory: bool) -> Result<(), fuser::Errno> {
        let _checkpoint = self.checkpoint.lock().map_err(|_| fuser::Errno::EIO)?;
        self.budget.pause_and_wait().map_err(errno)?;
        let result = self.lock().and_then(|mut workspace| {
            if directory {
                workspace.fsyncdir()
            } else {
                workspace.fsync()
            }
            .map_err(errno)
        });
        self.budget.resume();
        result.map(|_| ())
    }
}

struct CallbackTimer<'a> {
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

impl Filesystem for LayerFuse {
    fn init(&mut self, _request: &Request, config: &mut KernelConfig) -> std::io::Result<()> {
        let _callback = self.callback();
        self.count(|counters| counters.init += 1);
        config
            .set_max_write(MAX_REQUEST_BYTES as u32)
            .map_err(|maximum| std::io::Error::other(format!("kernel max_write {maximum}")))?;
        let _ = config.set_max_readahead(MAX_REQUEST_BYTES as u32);
        config
            .set_max_background(8)
            .map_err(|minimum| std::io::Error::other(format!("kernel max_background {minimum}")))?;
        config
            .set_time_granularity(Duration::from_nanos(1))
            .map_err(|granularity| {
                std::io::Error::other(format!("kernel time granularity {granularity:?}"))
            })?;
        Ok(())
    }

    fn destroy(&mut self) {
        let _callback = self.callback();
        self.count(|counters| counters.destroy += 1);
        self.session_end.notify();
    }

    fn lookup(&self, _request: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        let _callback = self.callback();
        self.count(|counters| counters.lookup += 1);
        let result = self
            .lock()
            .and_then(|mut workspace| {
                workspace
                    .lookup_child(MountedNodeId(parent.0), name.as_bytes())
                    .map_err(errno)
            })
            .map(|attr| self.attr(attr));
        match result {
            Ok(attr) => reply.entry(&TTL, &attr, Generation(0)),
            Err(error) => reply.error(error),
        }
    }

    fn forget(&self, _request: &Request, ino: INodeNo, nlookup: u64) {
        let _callback = self.callback();
        self.count(|counters| counters.forget += 1);
        if let Ok(mut workspace) = self.workspace.lock() {
            workspace.forget(MountedNodeId(ino.0), nlookup);
        }
    }

    fn getattr(
        &self,
        _request: &Request,
        ino: INodeNo,
        _handle: Option<FileHandle>,
        reply: ReplyAttr,
    ) {
        let _callback = self.callback();
        self.count(|counters| counters.getattr += 1);
        match self
            .lock()
            .and_then(|mut workspace| workspace.getattr(MountedNodeId(ino.0)).map_err(errno))
        {
            Ok(attr) => reply.attr(&TTL, &self.attr(attr)),
            Err(error) => reply.error(error),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn setattr(
        &self,
        _request: &Request,
        ino: INodeNo,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        _atime: Option<TimeOrNow>,
        mtime: Option<TimeOrNow>,
        _ctime: Option<SystemTime>,
        _handle: Option<FileHandle>,
        _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        flags: Option<BsdFileFlags>,
        reply: ReplyAttr,
    ) {
        let _callback = self.callback();
        self.count(|counters| counters.setattr += 1);
        if uid.is_some_and(|uid| uid != self.uid)
            || gid.is_some_and(|gid| gid != self.gid)
            || flags.is_some()
        {
            reply.error(fuser::Errno::EOPNOTSUPP);
            return;
        }
        let result = self.lock().and_then(|mut workspace| {
            let node = MountedNodeId(ino.0);
            if let Some(size) = size {
                workspace.truncate(node, size).map_err(errno)?;
            }
            if let Some(mode) = mode {
                workspace.chmod(node, mode).map_err(errno)?;
            }
            if let Some(mtime) = mtime {
                let time = match mtime {
                    TimeOrNow::SpecificTime(time) => time,
                    TimeOrNow::Now => SystemTime::now(),
                };
                let (seconds, nanoseconds) = timestamp(time).ok_or(fuser::Errno::EINVAL)?;
                workspace
                    .set_mtime(node, seconds, nanoseconds)
                    .map_err(errno)?;
            }
            workspace.getattr(node).map_err(errno)
        });
        let result = self.finish_mutation(result);
        match result {
            Ok(attr) => reply.attr(&TTL, &self.attr(attr)),
            Err(error) => reply.error(error),
        }
    }

    fn readlink(&self, _request: &Request, ino: INodeNo, reply: ReplyData) {
        let _callback = self.callback();
        self.count(|counters| counters.readlink += 1);
        match self
            .lock()
            .and_then(|workspace| workspace.readlink(MountedNodeId(ino.0)).map_err(errno))
        {
            Ok(target) => reply.data(&target),
            Err(error) => reply.error(error),
        }
    }

    fn mknod(
        &self,
        _request: &Request,
        parent: INodeNo,
        name: &OsStr,
        mode: u32,
        umask: u32,
        rdev: u32,
        reply: ReplyEntry,
    ) {
        let _callback = self.callback();
        self.count(|counters| counters.mknod += 1);
        if rdev != 0 || mode & 0o170000 != 0o100000 {
            reply.error(fuser::Errno::EOPNOTSUPP);
            return;
        }
        let result = self.lock().and_then(|mut workspace| {
            workspace
                .mknod_file(MountedNodeId(parent.0), name.as_bytes(), mode & !umask)
                .map_err(errno)
        });
        let result = self.finish_mutation(result);
        match result {
            Ok(attr) => reply.entry(&TTL, &self.attr(attr), Generation(0)),
            Err(error) => reply.error(error),
        }
    }

    fn mkdir(
        &self,
        _request: &Request,
        parent: INodeNo,
        name: &OsStr,
        mode: u32,
        umask: u32,
        reply: ReplyEntry,
    ) {
        let _callback = self.callback();
        self.count(|counters| counters.mkdir += 1);
        let result = self.lock().and_then(|mut workspace| {
            workspace
                .mkdir(MountedNodeId(parent.0), name.as_bytes(), mode & !umask)
                .map_err(errno)
        });
        let result = self.finish_mutation(result);
        match result {
            Ok(attr) => reply.entry(&TTL, &self.attr(attr), Generation(0)),
            Err(error) => reply.error(error),
        }
    }

    fn unlink(&self, _request: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        let _callback = self.callback();
        self.count(|counters| counters.unlink += 1);
        let result = self.lock().and_then(|mut workspace| {
            workspace
                .unlink(MountedNodeId(parent.0), name.as_bytes())
                .map_err(errno)
        });
        let result = self.finish_mutation(result);
        match result {
            Ok(()) => reply.ok(),
            Err(error) => reply.error(error),
        }
    }

    fn rmdir(&self, _request: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        let _callback = self.callback();
        self.count(|counters| counters.rmdir += 1);
        let result = self.lock().and_then(|mut workspace| {
            workspace
                .rmdir(MountedNodeId(parent.0), name.as_bytes())
                .map_err(errno)
        });
        let result = self.finish_mutation(result);
        match result {
            Ok(()) => reply.ok(),
            Err(error) => reply.error(error),
        }
    }

    fn symlink(
        &self,
        _request: &Request,
        parent: INodeNo,
        name: &OsStr,
        target: &Path,
        reply: ReplyEntry,
    ) {
        let _callback = self.callback();
        self.count(|counters| counters.symlink += 1);
        let result = self.lock().and_then(|mut workspace| {
            workspace
                .symlink(
                    MountedNodeId(parent.0),
                    name.as_bytes(),
                    target.as_os_str().as_bytes().to_vec(),
                )
                .map_err(errno)
        });
        let result = self.finish_mutation(result);
        match result {
            Ok(attr) => reply.entry(&TTL, &self.attr(attr), Generation(0)),
            Err(error) => reply.error(error),
        }
    }

    fn rename(
        &self,
        _request: &Request,
        parent: INodeNo,
        name: &OsStr,
        new_parent: INodeNo,
        new_name: &OsStr,
        flags: RenameFlags,
        reply: ReplyEmpty,
    ) {
        let _callback = self.callback();
        self.count(|counters| counters.rename += 1);
        if flags.intersects(RenameFlags::RENAME_EXCHANGE | RenameFlags::RENAME_WHITEOUT) {
            reply.error(fuser::Errno::EOPNOTSUPP);
            return;
        }
        let result = self.lock().and_then(|mut workspace| {
            workspace
                .rename(
                    MountedNodeId(parent.0),
                    name.as_bytes(),
                    MountedNodeId(new_parent.0),
                    new_name.as_bytes(),
                    flags.contains(RenameFlags::RENAME_NOREPLACE),
                )
                .map_err(errno)
        });
        let result = self.finish_mutation(result);
        match result {
            Ok(()) => reply.ok(),
            Err(error) => reply.error(error),
        }
    }

    fn link(
        &self,
        _request: &Request,
        ino: INodeNo,
        new_parent: INodeNo,
        new_name: &OsStr,
        reply: ReplyEntry,
    ) {
        let _callback = self.callback();
        self.count(|counters| counters.link += 1);
        let result = self.lock().and_then(|mut workspace| {
            workspace
                .link(
                    MountedNodeId(ino.0),
                    MountedNodeId(new_parent.0),
                    new_name.as_bytes(),
                )
                .map_err(errno)
        });
        let result = self.finish_mutation(result);
        match result {
            Ok(attr) => reply.entry(&TTL, &self.attr(attr), Generation(0)),
            Err(error) => reply.error(error),
        }
    }

    fn open(&self, _request: &Request, ino: INodeNo, flags: OpenFlags, reply: ReplyOpen) {
        let _callback = self.callback();
        self.count(|counters| counters.open += 1);
        if let Err(error) = Self::reject_sync_flags(flags.0) {
            reply.error(error);
            return;
        }
        let result = self.lock().and_then(|mut workspace| {
            workspace
                .open_file(MountedNodeId(ino.0), flags.0 & O_TRUNC != 0)
                .map_err(errno)
        });
        let result = if flags.0 & O_TRUNC != 0 {
            self.finish_mutation(result)
        } else {
            result
        };
        match result {
            Ok(handle) => reply.opened(FileHandle(handle.0), FopenFlags::FOPEN_KEEP_CACHE),
            Err(error) => reply.error(error),
        }
    }

    fn read(
        &self,
        _request: &Request,
        ino: INodeNo,
        handle: FileHandle,
        offset: u64,
        size: u32,
        _flags: OpenFlags,
        _lock_owner: Option<LockOwner>,
        reply: ReplyData,
    ) {
        let _callback = self.callback();
        self.count(|counters| counters.read += 1);
        let reservation = match self.budget.reserve(size as usize) {
            Ok(reservation) => reservation,
            Err(error) => {
                reply.error(errno(error));
                return;
            }
        };
        let result = self.lock().and_then(|mut workspace| {
            workspace
                .read(
                    MountedNodeId(ino.0),
                    MountedHandleId(handle.0),
                    offset,
                    size as usize,
                )
                .map_err(errno)
        });
        match result {
            Ok(data) => reply.data(&data),
            Err(error) => reply.error(error),
        }
        drop(reservation);
    }

    #[allow(clippy::too_many_arguments)]
    fn write(
        &self,
        _request: &Request,
        ino: INodeNo,
        handle: FileHandle,
        offset: u64,
        data: &[u8],
        _write_flags: WriteFlags,
        flags: OpenFlags,
        _lock_owner: Option<LockOwner>,
        reply: ReplyWrite,
    ) {
        let _callback = self.callback();
        self.count(|counters| counters.write += 1);
        if let Err(error) = Self::reject_sync_flags(flags.0) {
            reply.error(error);
            return;
        }
        let reservation = match self.budget.reserve(data.len()) {
            Ok(reservation) => reservation,
            Err(error) => {
                reply.error(errno(error));
                return;
            }
        };
        let result = self.lock().and_then(|mut workspace| {
            workspace
                .write(
                    MountedNodeId(ino.0),
                    MountedHandleId(handle.0),
                    offset,
                    data,
                )
                .map_err(errno)
        });
        drop(reservation);
        let result = self.finish_mutation(result);
        match result {
            Ok(size) => reply.written(size as u32),
            Err(error) => reply.error(error),
        }
    }

    fn flush(
        &self,
        _request: &Request,
        _ino: INodeNo,
        handle: FileHandle,
        _lock_owner: LockOwner,
        reply: ReplyEmpty,
    ) {
        let _callback = self.callback();
        self.count(|counters| counters.flush += 1);
        match self
            .lock()
            .and_then(|mut workspace| workspace.flush(MountedHandleId(handle.0)).map_err(errno))
        {
            Ok(()) => reply.ok(),
            Err(error) => reply.error(error),
        }
    }

    fn release(
        &self,
        _request: &Request,
        _ino: INodeNo,
        handle: FileHandle,
        _flags: OpenFlags,
        _lock_owner: Option<LockOwner>,
        _flush: bool,
        reply: ReplyEmpty,
    ) {
        let _callback = self.callback();
        self.count(|counters| counters.release += 1);
        match self.release_handle(MountedHandleId(handle.0)) {
            Ok(()) => reply.ok(),
            Err(error) => reply.error(error),
        }
    }

    fn fsync(
        &self,
        _request: &Request,
        _ino: INodeNo,
        _handle: FileHandle,
        _datasync: bool,
        reply: ReplyEmpty,
    ) {
        let _callback = self.callback();
        self.count(|counters| counters.fsync += 1);
        match self.explicit_checkpoint(false) {
            Ok(()) => reply.ok(),
            Err(error) => reply.error(error),
        }
    }

    fn opendir(&self, _request: &Request, ino: INodeNo, _flags: OpenFlags, reply: ReplyOpen) {
        let _callback = self.callback();
        self.count(|counters| counters.opendir += 1);
        match self.lock().and_then(|mut workspace| {
            workspace
                .open_directory(MountedNodeId(ino.0))
                .map_err(errno)
        }) {
            Ok(handle) => reply.opened(FileHandle(handle.0), FopenFlags::empty()),
            Err(error) => reply.error(error),
        }
    }

    fn readdir(
        &self,
        _request: &Request,
        _ino: INodeNo,
        handle: FileHandle,
        offset: u64,
        mut reply: ReplyDirectory,
    ) {
        let _callback = self.callback();
        self.count(|counters| counters.readdir += 1);
        let reservation = match self.budget.reserve(MAX_REQUEST_BYTES) {
            Ok(reservation) => reservation,
            Err(error) => {
                reply.error(errno(error));
                return;
            }
        };
        let result = self.lock().and_then(|mut workspace| {
            let mounted_handle = MountedHandleId(handle.0);
            let mut nodes = Vec::with_capacity(64);
            let mut committed = offset;
            for _ in 0..64 {
                let Some(entry) = workspace
                    .readdir_next(mounted_handle, committed)
                    .map_err(errno)?
                else {
                    break;
                };
                nodes.push(entry.node);
                if reply.add(
                    INodeNo(entry.node.0),
                    entry.next_offset,
                    file_type(entry.kind),
                    OsStr::from_bytes(&entry.name),
                ) {
                    workspace
                        .discard_readdir_pending(mounted_handle)
                        .map_err(errno)?;
                    break;
                }
                committed = entry.next_offset;
                workspace
                    .commit_readdir(mounted_handle, committed)
                    .map_err(errno)?;
            }
            workspace.reclaim_readdir_nodes(&nodes);
            Ok(())
        });
        match result {
            Ok(()) => reply.ok(),
            Err(error) => reply.error(error),
        }
        drop(reservation);
    }

    fn releasedir(
        &self,
        _request: &Request,
        _ino: INodeNo,
        handle: FileHandle,
        _flags: OpenFlags,
        reply: ReplyEmpty,
    ) {
        let _callback = self.callback();
        self.count(|counters| counters.releasedir += 1);
        match self.release_handle(MountedHandleId(handle.0)) {
            Ok(()) => reply.ok(),
            Err(error) => reply.error(error),
        }
    }

    fn fsyncdir(
        &self,
        _request: &Request,
        _ino: INodeNo,
        _handle: FileHandle,
        _datasync: bool,
        reply: ReplyEmpty,
    ) {
        let _callback = self.callback();
        self.count(|counters| counters.fsyncdir += 1);
        match self.explicit_checkpoint(true) {
            Ok(()) => reply.ok(),
            Err(error) => reply.error(error),
        }
    }

    fn statfs(&self, _request: &Request, _ino: INodeNo, reply: ReplyStatfs) {
        let _callback = self.callback();
        self.count(|counters| counters.statfs += 1);
        match self
            .lock()
            .and_then(|workspace| workspace.capacity().map_err(errno))
        {
            Ok(capacity) => reply.statfs(
                capacity.total_bytes / 4096,
                capacity.free_bytes / 4096,
                capacity.free_bytes / 4096,
                capacity.total_files,
                capacity.free_files,
                4096,
                255,
                4096,
            ),
            Err(error) => reply.error(error),
        }
    }

    fn access(&self, _request: &Request, ino: INodeNo, _mask: AccessFlags, reply: ReplyEmpty) {
        let _callback = self.callback();
        self.count(|counters| counters.access += 1);
        match self
            .lock()
            .and_then(|mut workspace| workspace.getattr(MountedNodeId(ino.0)).map_err(errno))
        {
            Ok(_) => reply.ok(),
            Err(error) => reply.error(error),
        }
    }

    fn create(
        &self,
        _request: &Request,
        parent: INodeNo,
        name: &OsStr,
        mode: u32,
        umask: u32,
        flags: i32,
        reply: ReplyCreate,
    ) {
        let _callback = self.callback();
        self.count(|counters| counters.create += 1);
        if let Err(error) = Self::reject_sync_flags(flags) {
            reply.error(error);
            return;
        }
        let result = self.lock().and_then(|mut workspace| {
            workspace
                .create_file(MountedNodeId(parent.0), name.as_bytes(), mode & !umask)
                .map_err(errno)
        });
        let result = self.finish_mutation(result);
        match result {
            Ok((attr, handle)) => reply.created(
                &TTL,
                &self.attr(attr),
                Generation(0),
                FileHandle(handle.0),
                FopenFlags::FOPEN_KEEP_CACHE,
            ),
            Err(error) => reply.error(error),
        }
    }
}

fn file_type(kind: MountedFileType) -> FileType {
    match kind {
        MountedFileType::RegularFile => FileType::RegularFile,
        MountedFileType::Directory => FileType::Directory,
        MountedFileType::Symlink => FileType::Symlink,
    }
}

fn errno(error: MountedError) -> fuser::Errno {
    match error {
        MountedError::NotFound => fuser::Errno::ENOENT,
        MountedError::AlreadyExists => fuser::Errno::EEXIST,
        MountedError::NotDirectory => fuser::Errno::ENOTDIR,
        MountedError::IsDirectory => fuser::Errno::EISDIR,
        MountedError::NotEmpty => fuser::Errno::ENOTEMPTY,
        MountedError::InvalidName | MountedError::InvalidRange => fuser::Errno::EINVAL,
        MountedError::PermissionDenied => fuser::Errno::EACCES,
        MountedError::ReadOnly => fuser::Errno::EROFS,
        MountedError::NoSpace => fuser::Errno::ENOSPC,
        MountedError::TooManyOpenFiles => fuser::Errno::EMFILE,
        MountedError::ResourceExhausted => fuser::Errno::ENOSPC,
        MountedError::Busy => fuser::Errno::EBUSY,
        MountedError::StaleHandle => fuser::Errno::ESTALE,
        MountedError::InvalidHandle => fuser::Errno::EBADF,
        MountedError::Conflict
        | MountedError::CommittedCleanup
        | MountedError::Corrupt
        | MountedError::Indeterminate
        | MountedError::Startup(_, _) => fuser::Errno::EIO,
        MountedError::Unsupported => fuser::Errno::EOPNOTSUPP,
        MountedError::Io(_) => fuser::Errno::EIO,
    }
}

fn system_time(seconds: i64, nanoseconds: u32) -> SystemTime {
    if seconds >= 0 {
        UNIX_EPOCH + Duration::new(seconds as u64, nanoseconds)
    } else {
        UNIX_EPOCH
            .checked_sub(Duration::new(seconds.unsigned_abs(), nanoseconds))
            .unwrap_or(UNIX_EPOCH)
    }
}

fn timestamp(time: SystemTime) -> Option<(i64, u32)> {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => Some((
            i64::try_from(duration.as_secs()).ok()?,
            duration.subsec_nanos(),
        )),
        Err(error) => Some((
            -i64::try_from(error.duration().as_secs()).ok()?,
            error.duration().subsec_nanos(),
        )),
    }
}

pub fn root_node() -> MountedNodeId {
    ROOT_NODE
}

#[cfg(any())]
mod legacy_tests {
    use super::*;
    use layerfs_storage::integrity::IntegrityMode;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fuse(label: &str) -> (LayerFuse, PathBuf) {
        let directory = std::env::temp_dir().join(format!(
            "layerfs-mount-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let workspace = MountedWorkspace::open(
            &directory.join("store.sqlite"),
            "main",
            IntegrityMode::TrustedLocalDev,
            directory.join("spool"),
            [0xa1; 32],
        )
        .unwrap();
        (LayerFuse::new(workspace, 0, 0), directory)
    }

    #[test]
    fn missing_entry_and_inode_notifier_fail_closed() {
        let (fuse, directory) = fuse("missing-notifier");
        assert_eq!(
            fuse.invalidate_entry(INodeNo::ROOT, OsStr::new("entry")),
            Err(fuser::Errno::EIO)
        );
        assert_eq!(
            fuse.invalidate_inode(INodeNo(2), 0, 1),
            Err(fuser::Errno::EIO)
        );
        assert_eq!(
            fuse.shared_workspace().lock().unwrap().lifecycle(),
            crate::workspace::MountedLifecycle::Incomplete
        );
        let counters = *fuse.shared_counters().lock().unwrap();
        assert_eq!(counters.invalidations_requested, 2);
        assert_eq!(counters.invalidations_unsupported, 2);
        drop(fuse);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn notifier_success_and_failure_counters_are_exact_and_failure_is_incomplete() {
        let (fuse, directory) = fuse("notifier-outcomes");
        fuse.count(|counters| counters.invalidations_requested += 2);
        assert_eq!(fuse.finish_invalidation(Ok(())), Ok(()));
        assert_eq!(
            fuse.finish_invalidation(Err(std::io::Error::other("injected"))),
            Err(fuser::Errno::EIO)
        );
        let counters = *fuse.shared_counters().lock().unwrap();
        assert_eq!(counters.invalidations_requested, 2);
        assert_eq!(counters.invalidations_succeeded, 1);
        assert_eq!(counters.invalidations_failed, 1);
        assert_eq!(
            fuse.shared_workspace().lock().unwrap().lifecycle(),
            crate::workspace::MountedLifecycle::Incomplete
        );
        drop(fuse);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn callback_wall_includes_workspace_lock_wait() {
        let (fuse, directory) = fuse("callback-wall");
        {
            let _callback = fuse.callback();
            let workspace = fuse.lock().unwrap();
            drop(workspace);
        }
        let counters = *fuse.shared_counters().lock().unwrap();
        assert!(counters.callback_wall_ns > 0);
        assert!(counters.mount_lock_wait_ns <= counters.callback_wall_ns);
        drop(fuse);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn destroy_only_emits_one_session_end_event() {
        let (mut fuse, directory) = fuse("destroy-event");
        let workspace = fuse.shared_workspace();
        let (sender, receiver) = std::sync::mpsc::channel();
        fuse.set_lifecycle_sender(sender).unwrap();
        {
            let mut workspace = workspace.lock().unwrap();
            let (file, handle) = workspace.create_file(ROOT_NODE, b"dirty", 0o644).unwrap();
            workspace.write(file.node, handle, 0, b"data").unwrap();
            workspace.release(handle).unwrap();
        }
        Filesystem::destroy(&mut fuse);
        Filesystem::destroy(&mut fuse);
        assert_eq!(receiver.recv().unwrap(), LayerFuseEvent::SessionEnded);
        assert!(matches!(
            receiver.try_recv(),
            Err(std::sync::mpsc::TryRecvError::Empty)
        ));
        assert_eq!(
            workspace.lock().unwrap().lifecycle(),
            crate::workspace::MountedLifecycle::Live
        );
        workspace.lock().unwrap().shutdown().unwrap();
        drop(workspace);
        drop(fuse);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
