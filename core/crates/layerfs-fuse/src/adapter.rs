//! Kernel argument checks and single-use replies; no filesystem algorithms.
use crate::replies::{attributes, errno, inode, kind, serial};
use fuser::*;
use layerfs_workspace::{
    FileAccess, FileOpenOptions, ProjectionReplyPermit, ReferenceScope, Workspace,
    MAX_DIRECTORY_ENTRIES, MAX_READ_BYTES,
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
}

impl Adapter {
    fn root(&self) -> u64 {
        self.workspace.root().serial
    }
    fn guard(&self, req: &Request) -> Result<(), Errno> {
        if self.stopping.load(Ordering::Acquire) {
            return Err(Errno::ENODEV);
        }
        if req.uid() != self.workspace.root().uid {
            return Err(Errno::EACCES);
        }
        Ok(())
    }
    fn handle(&self, ino: INodeNo, fh: FileHandle) -> Result<(), Errno> {
        let attrs = self.workspace.handle_attributes(fh.0).map_err(errno)?;
        if attrs.serial != serial(ino, self.root()) {
            return Err(Errno::EBADF);
        }
        Ok(())
    }
    fn observe(&self, req: &Request) -> Result<ProjectionReplyPermit, Errno> {
        // Callbacks retain this guard in their outer scope through reply emission.
        self.guard(req)?;
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
        Ok(())
    }

    fn destroy(&mut self) {
        self.stopping.store(true, Ordering::Release);
    }

    fn lookup(&self, req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        let permit = self.observe(req);
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
        let permit = self.observe(req);
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
        let permit = self.observe(req);
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
        let permit = self.observe(req);
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
        let permit = self.observe(req);
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
        let permit = self.observe(req);
        let result = permit.as_ref().map_err(|error| *error).and_then(|_| {
            self.workspace
                .readlink(serial(ino, self.root()), Instant::now() + CALLBACK_BUDGET)
                .map_err(errno)
        });
        match result {
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
        let result = self
            .handle(ino, fh)
            .and_then(|()| self.workspace.release(fh.0).map_err(errno));
        match result {
            Ok(()) => reply.ok(),
            Err(error) => reply.error(error),
        }
    }

    fn opendir(&self, req: &Request, ino: INodeNo, requested: OpenFlags, reply: ReplyOpen) {
        let permit = self.observe(req);
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
        let permit = self.observe(req);
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
        _: INodeNo,
        _: Option<u32>,
        _: Option<u32>,
        _: Option<u32>,
        _: Option<u64>,
        _: Option<TimeOrNow>,
        _: Option<TimeOrNow>,
        _: Option<SystemTime>,
        _: Option<FileHandle>,
        _: Option<SystemTime>,
        _: Option<SystemTime>,
        _: Option<SystemTime>,
        _: Option<BsdFileFlags>,
        reply: ReplyAttr,
    ) {
        reply.error(self.readonly(req));
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
                .begin_projection_write(deadline)
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

    fn mknod(
        &self,
        req: &Request,
        _: INodeNo,
        _: &OsStr,
        _: u32,
        _: u32,
        _: u32,
        reply: ReplyEntry,
    ) {
        reply.error(self.readonly(req));
    }
    fn mkdir(&self, req: &Request, _: INodeNo, _: &OsStr, _: u32, _: u32, reply: ReplyEntry) {
        reply.error(self.readonly(req));
    }
    fn unlink(&self, req: &Request, _: INodeNo, _: &OsStr, reply: ReplyEmpty) {
        reply.error(self.readonly(req));
    }
    fn rmdir(&self, req: &Request, _: INodeNo, _: &OsStr, reply: ReplyEmpty) {
        reply.error(self.readonly(req));
    }
    fn symlink(&self, req: &Request, _: INodeNo, _: &OsStr, _: &Path, reply: ReplyEntry) {
        reply.error(self.readonly(req));
    }
    fn rename(
        &self,
        req: &Request,
        _: INodeNo,
        _: &OsStr,
        _: INodeNo,
        _: &OsStr,
        _: RenameFlags,
        reply: ReplyEmpty,
    ) {
        reply.error(self.readonly(req));
    }
    fn link(&self, req: &Request, _: INodeNo, _: INodeNo, _: &OsStr, reply: ReplyEntry) {
        reply.error(self.readonly(req));
    }
    fn create(
        &self,
        req: &Request,
        _: INodeNo,
        _: &OsStr,
        _: u32,
        _: u32,
        _: i32,
        reply: ReplyCreate,
    ) {
        reply.error(self.readonly(req));
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
