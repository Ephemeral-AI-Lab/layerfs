//! Kernel argument checks and single-use replies; no filesystem algorithms.
use crate::replies::{attributes, errno, inode, kind, serial};
use fuser::*;
use layerfs_workspace::{
    filesystem::projection_counters::ProjectionOp, FileAccess, FileCreateOptions, FileOpenOptions,
    ProjectionReplyPermit, ReferenceScope, Workspace, MAX_DIRECTORY_ENTRIES, MAX_READ_BYTES,
};
use std::{
    ffi::OsStr,
    io,
    os::unix::ffi::OsStrExt,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime},
};

pub(crate) const CALLBACK_BUDGET: Duration = Duration::from_secs(10);
const TTL: Duration = Duration::ZERO;
// Linux do_open_execat carries __FMODE_EXEC in file flags through FUSE_OPEN.
const KERNEL_FMODE_EXEC: i32 = 1 << 5;

// FUSE forwards kernel UAPI flags. glibc's 64-bit O_LARGEFILE is zero even
// though the kernel sets this bit in every ordinary file description.
#[cfg(target_arch = "aarch64")]
const KERNEL_O_LARGEFILE: i32 = 0o400000;
#[cfg(target_arch = "x86_64")]
const KERNEL_O_LARGEFILE: i32 = 0o100000;
#[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
const KERNEL_O_LARGEFILE: i32 = 0;

pub(crate) struct Adapter {
    pub(crate) workspace: Workspace,
    pub(crate) stopping: Arc<AtomicBool>,
    pub(crate) writable: bool,
    pub(crate) stages: Arc<crate::range_ioctl::Stages>,
}

impl Adapter {
    pub(crate) fn root(&self) -> u64 {
        self.workspace.root().serial
    }
    pub(crate) fn guard(&self, req: &Request) -> Result<(), Errno> {
        if self.stopping.load(Ordering::Acquire) {
            return Err(Errno::ENODEV);
        }
        if req.uid() != self.workspace.root().uid {
            return Err(Errno::EACCES);
        }
        Ok(())
    }
    pub(crate) fn handle(&self, ino: INodeNo, fh: FileHandle) -> Result<(), Errno> {
        let attrs = self.workspace.handle_attributes(fh.0).map_err(errno)?;
        if attrs.serial != serial(ino, self.root()) {
            return Err(Errno::EBADF);
        }
        Ok(())
    }
    fn observe(&self, req: &Request, op: ProjectionOp) -> Result<ProjectionReplyPermit, Errno> {
        // Callbacks retain this guard in their outer scope through reply emission.
        self.guard(req)?;
        self.workspace.record_projection_call(op);
        self.workspace
            .begin_projection_reply(Instant::now() + CALLBACK_BUDGET)
            .map_err(errno)
    }
    fn readonly(&self, req: &Request) -> Errno {
        self.guard(req).err().unwrap_or(if self.writable {
            Errno::EOPNOTSUPP
        } else {
            Errno::EROFS
        })
    }
}

fn flags(value: OpenFlags, directory: bool, writable: bool) -> Result<FileOpenOptions, Errno> {
    let mutations = libc::O_ACCMODE | libc::O_TRUNC | libc::O_APPEND | libc::O_CREAT;
    if (!writable || directory) && value.0 & mutations != 0 {
        return Err(Errno::EROFS);
    }
    let allowed = libc::O_CLOEXEC
        | KERNEL_O_LARGEFILE
        | libc::O_NOFOLLOW
        | libc::O_DIRECTORY
        | libc::O_NOCTTY
        | libc::O_NONBLOCK
        | libc::O_NOATIME
        | if directory { 0 } else { KERNEL_FMODE_EXEC }
        | if writable && !directory {
            libc::O_ACCMODE | libc::O_APPEND
        } else {
            0
        };
    if value.0 & !allowed != 0 {
        return Err(Errno::EOPNOTSUPP);
    }
    let access = match value.0 & libc::O_ACCMODE {
        libc::O_RDONLY => FileAccess::ReadOnly,
        libc::O_WRONLY => FileAccess::WriteOnly,
        libc::O_RDWR => FileAccess::ReadWrite,
        _ => return Err(Errno::EINVAL),
    };
    Ok(FileOpenOptions {
        access,
        append: value.0 & libc::O_APPEND != 0,
        truncate: false,
    })
}

impl Filesystem for Adapter {
    fn init(&mut self, _: &Request, config: &mut KernelConfig) -> io::Result<()> {
        config
            .set_max_write(MAX_READ_BYTES as u32)
            .map_err(|_| io::ErrorKind::Unsupported)?;
        config
            .set_max_readahead(MAX_READ_BYTES as u32)
            .map_err(|_| io::ErrorKind::Unsupported)?;
        config
            .set_max_background(1)
            .map_err(|_| io::ErrorKind::Unsupported)?;
        config
            .set_congestion_threshold(1)
            .map_err(|_| io::ErrorKind::Unsupported)?;
        // fuser defaults add only ASYNC_READ, BIG_WRITES and supported MAX_PAGES.
        // No writeback, readdirplus, stateless-open or symlink-cache capability.
        // No ATOMIC_O_TRUNC: kernel OPEN precedes separate size SETATTR.
        Ok(())
    }

    fn destroy(&mut self) {
        self.stopping.store(true, Ordering::Release);
        self.stages.clear();
    }

    fn lookup(&self, req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        let permit = self.observe(req, ProjectionOp::Lookup);
        let result = permit.as_ref().map_err(|error| *error).and_then(|_| {
            self.workspace
                .lookup(
                    serial(parent, self.root()),
                    name.as_bytes(),
                    ReferenceScope::Projection,
                    Instant::now() + CALLBACK_BUDGET,
                )
                .map_err(errno)
                .and_then(|value| {
                    attributes(value, self.root()).inspect_err(|_| {
                        self.workspace
                            .forget(value.serial, 1, ReferenceScope::Projection);
                    })
                })
        });
        match result {
            Ok(value) => reply.entry(&TTL, &value, Generation(0)),
            Err(error) => reply.error(error),
        }
    }

    fn forget(&self, _: &Request, ino: INodeNo, count: u64) {
        self.workspace
            .forget(serial(ino, self.root()), count, ReferenceScope::Projection);
    }

    fn getattr(&self, req: &Request, ino: INodeNo, fh: Option<FileHandle>, reply: ReplyAttr) {
        let permit = self.observe(req, ProjectionOp::Getattr);
        let result = permit.as_ref().map_err(|error| *error).and_then(|_| {
            let value = match fh {
                Some(handle) => {
                    self.handle(ino, handle)?;
                    self.workspace.handle_attributes(handle.0)
                }
                None => self.workspace.getattr(serial(ino, self.root())),
            };
            value
                .map_err(errno)
                .and_then(|value| attributes(value, self.root()))
        });
        match result {
            Ok(value) => reply.attr(&TTL, &value),
            Err(error) => reply.error(error),
        }
    }

    fn access(&self, req: &Request, ino: INodeNo, mask: AccessFlags, reply: ReplyEmpty) {
        let permit = self.observe(req, ProjectionOp::Other);
        let result = permit.as_ref().map_err(|error| *error).and_then(|_| {
            if !self.writable && mask.bits() & 2 != 0 {
                return Err(Errno::EROFS);
            }
            self.workspace
                .access(
                    serial(ino, self.root()),
                    req.uid(),
                    req.gid(),
                    mask.bits() as u8,
                )
                .map_err(errno)
        });
        match result {
            Ok(()) => reply.ok(),
            Err(error) => reply.error(error),
        }
    }

    fn open(&self, req: &Request, ino: INodeNo, requested: OpenFlags, reply: ReplyOpen) {
        let permit = self.observe(req, ProjectionOp::Open);
        let result = permit
            .as_ref()
            .map_err(|error| *error)
            .and_then(|_| flags(requested, false, self.writable))
            .and_then(|options| {
                if requested.0 & KERNEL_FMODE_EXEC != 0 {
                    self.workspace
                        .access(serial(ino, self.root()), req.uid(), req.gid(), 1)
                        .map_err(errno)?;
                }
                self.workspace
                    .open_file(
                        serial(ino, self.root()),
                        options,
                        ReferenceScope::Projection,
                        Instant::now() + CALLBACK_BUDGET,
                    )
                    .map_err(errno)
            });
        match result {
            Ok(handle) => reply.opened(
                FileHandle(handle),
                if self.writable {
                    FopenFlags::FOPEN_DIRECT_IO
                } else {
                    FopenFlags::empty()
                },
            ),
            Err(error) => reply.error(error),
        }
    }

    fn read(
        &self,
        req: &Request,
        ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        size: u32,
        requested: OpenFlags,
        _: Option<LockOwner>,
        reply: ReplyData,
    ) {
        let permit = self.observe(req, ProjectionOp::Read);
        let result = permit
            .as_ref()
            .map_err(|error| *error)
            .and_then(|_| flags(requested, false, self.writable).map(|_| ()))
            .and_then(|()| self.handle(ino, fh))
            .and_then(|()| {
                self.workspace
                    .read(
                        fh.0,
                        offset,
                        size as usize,
                        Instant::now() + CALLBACK_BUDGET,
                    )
                    .map_err(errno)
            });
        match result {
            Ok(bytes) => reply.data(bytes.as_ref()),
            Err(error) => reply.error(error),
        }
    }

    fn readlink(&self, req: &Request, ino: INodeNo, reply: ReplyData) {
        let permit = self.observe(req, ProjectionOp::Other);
        let result = permit.as_ref().map_err(|error| *error).and_then(|_| {
            self.workspace
                .readlink(serial(ino, self.root()), Instant::now() + CALLBACK_BUDGET)
                .map_err(errno)
        });
        match result {
            // Keep the Linux projection within its PATH_MAX convention, even
            // when native SDK/C1 state contains a valid 4096-byte target.
            Ok(bytes) if bytes.as_ref().len() >= libc::PATH_MAX as usize => {
                reply.error(Errno::ENAMETOOLONG)
            }
            Ok(bytes) => reply.data(bytes.as_ref()),
            Err(error) => reply.error(error),
        }
    }

    fn flush(&self, _: &Request, ino: INodeNo, fh: FileHandle, _: LockOwner, reply: ReplyEmpty) {
        let result = self
            .handle(ino, fh)
            .and_then(|()| self.workspace.flush(fh.0).map_err(errno));
        match result {
            Ok(()) => reply.ok(),
            Err(error) => reply.error(error),
        }
    }

    fn release(
        &self,
        _: &Request,
        ino: INodeNo,
        fh: FileHandle,
        _: OpenFlags,
        _: Option<LockOwner>,
        _: bool,
        reply: ReplyEmpty,
    ) {
        let result = self.handle(ino, fh).and_then(|()| {
            self.stages.release(fh.0);
            self.workspace.release(fh.0).map_err(errno)
        });
        match result {
            Ok(()) => reply.ok(),
            Err(error) => reply.error(error),
        }
    }

    fn opendir(&self, req: &Request, ino: INodeNo, requested: OpenFlags, reply: ReplyOpen) {
        let permit = self.observe(req, ProjectionOp::Other);
        let result = permit
            .as_ref()
            .map_err(|error| *error)
            .and_then(|_| flags(requested, true, self.writable).map(|_| ()))
            .and_then(|()| {
                self.workspace
                    .opendir(serial(ino, self.root()), ReferenceScope::Projection)
                    .map_err(errno)
            });
        match result {
            Ok(handle) => reply.opened(FileHandle(handle), FopenFlags::empty()),
            Err(error) => reply.error(error),
        }
    }

    fn readdir(
        &self,
        req: &Request,
        ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        mut reply: ReplyDirectory,
    ) {
        let permit = self.observe(req, ProjectionOp::Readdir);
        let result = permit
            .as_ref()
            .map_err(|error| *error)
            .and_then(|_| self.handle(ino, fh))
            .and_then(|()| {
                self.workspace
                    .readdir(
                        fh.0,
                        offset,
                        MAX_DIRECTORY_ENTRIES,
                        Instant::now() + CALLBACK_BUDGET,
                    )
                    .map_err(errno)
            });
        match result {
            Ok(page) => {
                for entry in page.entries() {
                    if reply.add(
                        inode(entry.serial, self.root()),
                        entry.cookie,
                        kind(entry.kind),
                        OsStr::from_bytes(&entry.name),
                    ) {
                        break;
                    }
                }
                reply.ok();
            }
            Err(error) => reply.error(error),
        }
    }

    fn releasedir(
        &self,
        _: &Request,
        ino: INodeNo,
        fh: FileHandle,
        _: OpenFlags,
        reply: ReplyEmpty,
    ) {
        let result = self
            .handle(ino, fh)
            .and_then(|()| self.workspace.releasedir(fh.0).map_err(errno));
        match result {
            Ok(()) => reply.ok(),
            Err(error) => reply.error(error),
        }
    }

    fn statfs(&self, req: &Request, _: INodeNo, reply: ReplyStatfs) {
        match self.guard(req) {
            Ok(()) => reply.statfs(0, 0, 0, 0, 0, 4096, 255, 4096),
            Err(error) => reply.error(error),
        }
    }

    fn fsync(&self, _: &Request, _: INodeNo, _: FileHandle, _: bool, reply: ReplyEmpty) {
        reply.error(Errno::EOPNOTSUPP);
    }
    fn fsyncdir(&self, _: &Request, _: INodeNo, _: FileHandle, _: bool, reply: ReplyEmpty) {
        reply.error(Errno::EOPNOTSUPP);
    }
    fn readdirplus(
        &self,
        _: &Request,
        _: INodeNo,
        _: FileHandle,
        _: u64,
        reply: ReplyDirectoryPlus,
    ) {
        reply.error(Errno::EOPNOTSUPP);
    }
    fn getxattr(&self, _: &Request, _: INodeNo, _: &OsStr, _: u32, reply: ReplyXattr) {
        reply.error(Errno::EOPNOTSUPP);
    }
    fn listxattr(&self, _: &Request, _: INodeNo, _: u32, reply: ReplyXattr) {
        reply.error(Errno::EOPNOTSUPP);
    }

    fn setattr(
        &self,
        req: &Request,
        ino: INodeNo,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        atime: Option<TimeOrNow>,
        mtime: Option<TimeOrNow>,
        ctime: Option<SystemTime>,
        fh: Option<FileHandle>,
        crtime: Option<SystemTime>,
        chgtime: Option<SystemTime>,
        bkuptime: Option<SystemTime>,
        flags: Option<BsdFileFlags>,
        reply: ReplyAttr,
    ) {
        let deadline = Instant::now() + CALLBACK_BUDGET;
        // The callback class is counted as observed, before any refusal, so the
        // count describes what the kernel asked for rather than what succeeded.
        self.workspace.record_projection_call(ProjectionOp::Setattr);
        let request = self.guard(req).and_then(|()| {
            if !self.writable {
                return Err(Errno::EROFS);
            }
            // No atime setter, no ownership change and no platform flag has a
            // persisted representation. UTIME_OMIT selects no change.
            if uid.is_some()
                || gid.is_some()
                || ctime.is_some()
                || crtime.is_some()
                || chgtime.is_some()
                || bkuptime.is_some()
                || flags.is_some()
            {
                return Err(Errno::EOPNOTSUPP);
            }
            // `None` is an omitted field (the kernel clears FATTR_*_OMIT).
            // A selected atime is refused unless it names the same instant the
            // selected mtime does, because only one portable timestamp exists.
            let mtime = match mtime {
                None => None,
                Some(TimeOrNow::Now) => Some(SystemTime::now()),
                Some(TimeOrNow::SpecificTime(value)) => Some(value),
            };
            let atime = match atime {
                None => None,
                Some(TimeOrNow::Now) => Some(SystemTime::now()),
                Some(TimeOrNow::SpecificTime(value)) => Some(value),
            };
            if atime.is_some() && atime != mtime {
                return Err(Errno::EOPNOTSUPP);
            }
            let mtime = match mtime.or(atime) {
                Some(value) => {
                    let seconds = value
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .map_err(|_| Errno::EINVAL)?;
                    Some((
                        i64::try_from(seconds.as_secs()).map_err(|_| Errno::EOVERFLOW)?,
                        seconds.subsec_nanos(),
                    ))
                }
                None => None,
            };
            if size.is_none() && mode.is_none() && mtime.is_none() {
                return Err(Errno::EOPNOTSUPP);
            }
            Ok(layerfs_workspace::PortableAttributes {
                size,
                mode: mode.map(|value| value & 0o7777),
                mtime,
            })
        });
        let mut permit = request.and_then(|request| {
            self.workspace
                .begin_projection_mutation(deadline)
                .map(|permit| (permit, request))
                .map_err(errno)
        });
        let result = permit
            .as_mut()
            .map_err(|error| *error)
            .and_then(|(permit, request)| {
                if let Some(handle) = fh {
                    self.handle(ino, handle)?;
                }
                let serial = serial(ino, self.root());
                // Check unchanged kernel representation fields before publication.
                attributes(self.workspace.getattr(serial).map_err(errno)?, self.root())?;
                let value = permit
                    .set_attributes(serial, *request, deadline)
                    .map_err(errno)?;
                attributes(value, self.root())
            });
        // Size SETATTR's kernel caller owns cache invalidation after releasing
        // NOWRITE. No userspace notifier or invented post-kernel fence runs here.
        match result {
            Ok(value) => reply.attr(&TTL, &value),
            Err(error) => reply.error(error),
        }
    }

    fn write(
        &self,
        req: &Request,
        ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        data: &[u8],
        write_flags: WriteFlags,
        requested: OpenFlags,
        _: Option<LockOwner>,
        reply: ReplyWrite,
    ) {
        let deadline = Instant::now() + CALLBACK_BUDGET;
        self.workspace.record_projection_call(ProjectionOp::Write);
        let mut permit = self.guard(req).and_then(|()| {
            if !self.writable {
                return Err(Errno::EROFS);
            }
            if data.len() > MAX_READ_BYTES {
                return Err(Errno::E2BIG);
            }
            if write_flags.intersects(WriteFlags::FUSE_WRITE_CACHE)
                || write_flags.bits()
                    & !(WriteFlags::FUSE_WRITE_LOCKOWNER | WriteFlags::FUSE_WRITE_KILL_SUIDGID)
                        .bits()
                    != 0
            {
                return Err(Errno::EOPNOTSUPP);
            }
            let options = flags(requested, false, true)?;
            self.handle(ino, fh)?;
            self.workspace
                .begin_projection_mutation(deadline)
                .map(|permit| (permit, options.append))
                .map_err(errno)
        });
        let result = permit
            .as_mut()
            .map_err(|error| *error)
            .and_then(|(permit, append)| {
                let payload = self
                    .workspace
                    .own_payload(data.len() as u64, &mut &data[..], deadline)
                    .map_err(errno)?;
                permit
                    .write_file(fh.0, offset, &payload, *append, deadline)
                    .map_err(errno)
            });
        // The origin permit stays alive through the send attempt. fuser does not
        // expose checked reply delivery or a later kernel-completion acknowledgement.
        match result {
            Ok(receipt) => reply.written(receipt.accepted_bytes as u32),
            Err(error) => reply.error(error),
        }
    }

    fn ioctl(
        &self,
        req: &Request,
        ino: INodeNo,
        fh: FileHandle,
        flags: IoctlFlags,
        cmd: u32,
        input: &[u8],
        out_size: u32,
        reply: ReplyIoctl,
    ) {
        crate::range_ioctl::dispatch(self, req, ino, fh, flags, cmd, input, out_size, reply);
    }

    fn mknod(
        &self,
        req: &Request,
        parent: INodeNo,
        name: &OsStr,
        mode: u32,
        _umask: u32,
        rdev: u32,
        reply: ReplyEntry,
    ) {
        let deadline = Instant::now() + CALLBACK_BUDGET;
        self.workspace.record_projection_call(ProjectionOp::Write);
        let mut permit = self.guard(req).and_then(|()| {
            if !self.writable {
                return Err(Errno::EROFS);
            }
            // INIT does not enable DONT_MASK: Linux sends final permission bits
            // after applying umask. Special kinds have no storage contract.
            if mode & libc::S_IFMT != libc::S_IFREG || mode & !0o777 != libc::S_IFREG {
                return Err(Errno::EOPNOTSUPP);
            }
            if rdev != 0 {
                return Err(Errno::EOPNOTSUPP);
            }
            self.workspace
                .begin_projection_mutation(deadline)
                .map_err(errno)
        });
        let result = permit.as_mut().map_err(|error| *error).and_then(|permit| {
            let value = permit
                .mknod(
                    serial(parent, self.root()),
                    name.as_bytes(),
                    mode & 0o777,
                    0,
                    deadline,
                )
                .map_err(errno)?;
            attributes(value, self.root()).inspect_err(|_| {
                self.workspace
                    .forget(value.serial, 1, ReferenceScope::Projection);
            })
        });
        // The kernel installs the entry and invalidates its parent; hold the
        // permit through the reply attempt as CREATE does.
        match result {
            Ok(value) => reply.entry(&TTL, &value, Generation(0)),
            Err(error) => reply.error(error),
        }
    }
    fn mkdir(
        &self,
        req: &Request,
        parent: INodeNo,
        name: &OsStr,
        mode: u32,
        _umask: u32,
        reply: ReplyEntry,
    ) {
        let deadline = Instant::now() + CALLBACK_BUDGET;
        self.workspace.record_projection_call(ProjectionOp::Write);
        let mut permit = self.guard(req).and_then(|()| {
            if !self.writable {
                return Err(Errno::EROFS);
            }
            self.workspace
                .begin_projection_mutation(deadline)
                .map_err(errno)
        });
        let result = permit.as_mut().map_err(|error| *error).and_then(|permit| {
            // INIT does not enable DONT_MASK: Linux sends final permission/sticky
            // bits after applying umask. Native SDK callers supply their own mask.
            let value = permit
                .mkdir(
                    serial(parent, self.root()),
                    name.as_bytes(),
                    mode,
                    0,
                    deadline,
                )
                .map_err(errno)?;
            attributes(value, self.root()).inspect_err(|_| {
                self.workspace
                    .forget(value.serial, 1, ReferenceScope::Projection);
            })
        });
        // The kernel owns entry installation and parent invalidation. A reverse
        // entry notification here would wait on the parent lock held by mkdir.
        // Keep the permit through the reply attempt; delivery is not observable.
        match result {
            Ok(value) => reply.entry(&TTL, &value, Generation(0)),
            Err(error) => reply.error(error),
        }
    }
    fn unlink(&self, req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        let deadline = Instant::now() + CALLBACK_BUDGET;
        self.workspace.record_projection_call(ProjectionOp::Write);
        let mut permit = self.guard(req).and_then(|()| {
            if !self.writable {
                return Err(Errno::EROFS);
            }
            self.workspace
                .begin_projection_mutation(deadline)
                .map_err(errno)
        });
        let result = permit.as_mut().map_err(|error| *error).and_then(|permit| {
            permit
                .unlink(serial(parent, self.root()), name.as_bytes(), deadline)
                .map_err(errno)
        });
        // The kernel owns the parent lock and cache invalidation through UNLINK.
        match result {
            Ok(()) => reply.ok(),
            Err(error) => reply.error(error),
        }
    }
    fn rmdir(&self, req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        let deadline = Instant::now() + CALLBACK_BUDGET;
        self.workspace.record_projection_call(ProjectionOp::Write);
        let mut permit = self.guard(req).and_then(|()| {
            if !self.writable {
                return Err(Errno::EROFS);
            }
            self.workspace
                .begin_projection_mutation(deadline)
                .map_err(errno)
        });
        let result = permit.as_mut().map_err(|error| *error).and_then(|permit| {
            permit
                .rmdir(serial(parent, self.root()), name.as_bytes(), deadline)
                .map_err(errno)
        });
        match result {
            Ok(()) => reply.ok(),
            Err(error) => reply.error(error),
        }
    }
    fn symlink(
        &self,
        req: &Request,
        parent: INodeNo,
        name: &OsStr,
        target: &Path,
        reply: ReplyEntry,
    ) {
        let deadline = Instant::now() + CALLBACK_BUDGET;
        self.workspace.record_projection_call(ProjectionOp::Write);
        let mut permit = self.guard(req).and_then(|()| {
            if !self.writable {
                return Err(Errno::EROFS);
            }
            self.workspace
                .begin_projection_mutation(deadline)
                .map_err(errno)
        });
        let result = permit.as_mut().map_err(|error| *error).and_then(|permit| {
            let value = permit
                .symlink(
                    serial(parent, self.root()),
                    name.as_bytes(),
                    target.as_os_str().as_bytes(),
                    deadline,
                )
                .map_err(errno)?;
            attributes(value, self.root()).inspect_err(|_| {
                self.workspace
                    .forget(value.serial, 1, ReferenceScope::Projection);
            })
        });
        // The parent lock remains kernel-owned through SYMLINK. Keep the permit
        // through the reply attempt and let the kernel install/invalidate entries.
        match result {
            Ok(value) => reply.entry(&TTL, &value, Generation(0)),
            Err(error) => reply.error(error),
        }
    }
    fn rename(
        &self,
        req: &Request,
        parent: INodeNo,
        name: &OsStr,
        new_parent: INodeNo,
        new_name: &OsStr,
        flags: RenameFlags,
        reply: ReplyEmpty,
    ) {
        let deadline = Instant::now() + CALLBACK_BUDGET;
        self.workspace.record_projection_call(ProjectionOp::Rename);
        // RENAME_NOREPLACE is the only selected flag; exchange and whiteout stay
        // unsupported and are refused before any publication.
        let noreplace = match flags.bits() {
            0 => false,
            1 => true,
            _ => {
                reply.error(Errno::EINVAL);
                return;
            }
        };
        let mut permit = self.guard(req).and_then(|()| {
            if !self.writable {
                return Err(Errno::EROFS);
            }
            self.workspace
                .begin_projection_mutation(deadline)
                .map_err(errno)
        });
        let result = permit.as_mut().map_err(|error| *error).and_then(|permit| {
            permit
                .rename(
                    serial(parent, self.root()),
                    name.as_bytes(),
                    serial(new_parent, self.root()),
                    new_name.as_bytes(),
                    layerfs_workspace::RenameFlags { noreplace },
                    deadline,
                )
                .map_err(errno)
        });
        // RENAME holds both parent locks until the reply; the kernel installs
        // and invalidates both entries itself, so no reverse notification runs.
        match result {
            Ok(()) => reply.ok(),
            Err(error) => reply.error(error),
        }
    }
    fn link(
        &self,
        req: &Request,
        ino: INodeNo,
        new_parent: INodeNo,
        new_name: &OsStr,
        reply: ReplyEntry,
    ) {
        let deadline = Instant::now() + CALLBACK_BUDGET;
        self.workspace.record_projection_call(ProjectionOp::Write);
        let mut permit = self.guard(req).and_then(|()| {
            if !self.writable {
                return Err(Errno::EROFS);
            }
            self.workspace
                .begin_projection_mutation(deadline)
                .map_err(errno)
        });
        let result = permit.as_mut().map_err(|error| *error).and_then(|permit| {
            let value = permit
                .link(
                    serial(new_parent, self.root()),
                    new_name.as_bytes(),
                    serial(ino, self.root()),
                    deadline,
                )
                .map_err(errno)?;
            attributes(value, self.root()).inspect_err(|_| {
                self.workspace
                    .forget(value.serial, 1, ReferenceScope::Projection);
            })
        });
        match result {
            Ok(value) => reply.entry(&TTL, &value, Generation(0)),
            Err(error) => reply.error(error),
        }
    }
    fn create(
        &self,
        req: &Request,
        parent: INodeNo,
        name: &OsStr,
        mode: u32,
        _umask: u32,
        requested: i32,
        reply: ReplyCreate,
    ) {
        let deadline = Instant::now() + CALLBACK_BUDGET;
        self.workspace.record_projection_call(ProjectionOp::Write);
        let mut permit = self.guard(req).and_then(|()| {
            if !self.writable {
                return Err(Errno::EROFS);
            }
            // Linux CREATE includes S_IFREG and has already applied umask;
            // INIT does not negotiate DONT_MASK. Other mode bits stay refused.
            if mode & !0o777 != libc::S_IFREG {
                return Err(Errno::EINVAL);
            }
            if requested & (libc::O_DIRECTORY | KERNEL_FMODE_EXEC) != 0 {
                return Err(Errno::EOPNOTSUPP);
            }
            let mut open = flags(
                OpenFlags(requested & !(libc::O_CREAT | libc::O_EXCL | libc::O_TRUNC)),
                false,
                true,
            )?;
            open.truncate = requested & libc::O_TRUNC != 0;
            let options = FileCreateOptions {
                mode: mode & 0o777,
                umask: 0,
                exclusive: requested & libc::O_EXCL != 0,
                open,
            };
            self.workspace
                .begin_projection_mutation(deadline)
                .map(|permit| (permit, options))
                .map_err(errno)
        });
        let result = permit
            .as_mut()
            .map_err(|error| *error)
            .and_then(|(permit, options)| {
                let (value, handle) = permit
                    .create_file(
                        serial(parent, self.root()),
                        name.as_bytes(),
                        *options,
                        deadline,
                    )
                    .map_err(errno)?;
                match attributes(value, self.root()) {
                    Ok(value) => Ok((value, handle)),
                    Err(error) => {
                        let released = self.workspace.release(handle);
                        self.workspace
                            .forget(value.serial, 1, ReferenceScope::Projection);
                        released.map_err(errno)?;
                        Err(error)
                    }
                }
            });
        // The kernel installs the entry and invalidates its parent. Hold the
        // permit through the reply attempt; fuser does not report send success.
        match result {
            Ok((value, handle)) => reply.created(
                &TTL,
                &value,
                Generation(0),
                FileHandle(handle),
                FopenFlags::FOPEN_DIRECT_IO,
            ),
            Err(error) => reply.error(error),
        }
    }
    fn setxattr(
        &self,
        req: &Request,
        _: INodeNo,
        _: &OsStr,
        _: &[u8],
        _: i32,
        _: u32,
        reply: ReplyEmpty,
    ) {
        reply.error(self.readonly(req));
    }
    fn removexattr(&self, req: &Request, _: INodeNo, _: &OsStr, reply: ReplyEmpty) {
        reply.error(self.readonly(req));
    }
}
