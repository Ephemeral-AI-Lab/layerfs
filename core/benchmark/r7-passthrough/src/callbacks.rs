use crate::state::{
    attr, checked, cstring, error, owned_fd, reopen, Passthrough, Result, TTL, WINDOW,
};
use fuser::*;
use std::{
    ffi::{CStr, OsStr},
    io,
    os::{
        fd::{AsRawFd, IntoRawFd},
        unix::{
            ffi::OsStrExt,
            fs::{FileExt, MetadataExt},
        },
    },
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn empty(reply: ReplyEmpty, result: Result<()>) {
    match result {
        Ok(()) => reply.ok(),
        Err(e) => reply.error(e),
    }
}
fn entry(reply: ReplyEntry, result: Result<FileAttr>) {
    match result {
        Ok(a) => reply.entry(&TTL, &a, Generation(0)),
        Err(e) => reply.error(e),
    }
}
fn timespec(time: Option<TimeOrNow>) -> Result<libc::timespec> {
    match time {
        None => Ok(libc::timespec {
            tv_sec: 0,
            tv_nsec: libc::UTIME_OMIT,
        }),
        Some(TimeOrNow::Now) => Ok(libc::timespec {
            tv_sec: 0,
            tv_nsec: libc::UTIME_NOW,
        }),
        Some(TimeOrNow::SpecificTime(t)) => {
            let (sec, ns) = match t.duration_since(UNIX_EPOCH) {
                Ok(d) => (
                    i64::try_from(d.as_secs()).map_err(|_| Errno::EOVERFLOW)?,
                    d.subsec_nanos(),
                ),
                Err(e) => {
                    let d = e.duration();
                    let s = i64::try_from(d.as_secs()).map_err(|_| Errno::EOVERFLOW)?;
                    if d.subsec_nanos() == 0 {
                        (-s, 0)
                    } else {
                        (-s - 1, 1_000_000_000 - d.subsec_nanos())
                    }
                }
            };
            Ok(libc::timespec {
                tv_sec: sec,
                tv_nsec: ns.into(),
            })
        }
    }
}
impl Filesystem for Passthrough {
    fn init(&mut self, _: &Request, config: &mut KernelConfig) -> io::Result<()> {
        let offered = config.capabilities();
        let required = InitFlags::FUSE_ASYNC_READ | InitFlags::FUSE_BIG_WRITES;
        if !offered.contains(required) {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "native capabilities absent",
            ));
        }
        let refused = |_| {
            io::Error::new(
                io::ErrorKind::Unsupported,
                "kernel refused exact R7 profile",
            )
        };
        config.set_max_write(WINDOW as u32).map_err(refused)?;
        config.set_max_readahead(WINDOW as u32).map_err(refused)?;
        config.set_max_background(1).map_err(|_| refused(0))?;
        config.set_congestion_threshold(1).map_err(|_| refused(0))?;
        config
            .set_time_granularity(Duration::from_nanos(1))
            .map_err(|_| refused(0))?;
        let selected = required | (offered & InitFlags::FUSE_MAX_PAGES);
        let abi = config.kernel_abi();
        println!("{{\"event\":\"negotiated\",\"abi\":[{},{}],\"offered\":{},\"selected\":{},\"max_write\":{},\"max_readahead\":{},\"max_background\":1,\"congestion_threshold\":1,\"configured_receive_loops\":2,\"serving_established\":false,\"ttl_seconds\":60,\"writeback\":false,\"default_permissions\":true}}", abi.0, abi.1, offered.bits(), selected.bits(), WINDOW, WINDOW);
        Ok(())
    }
    fn destroy(&mut self) {
        let state = self.state.lock().unwrap();
        println!("{{\"event\":\"drain\",\"counter_domain\":\"implemented filesystem callbacks\",\"batch_forget_kernel_requests\":null,\"default_callback_kernel_requests\":null,\"opcodes\":{:?},\"held_nodes\":{},\"held_handles\":{}}}",
            state.opcodes, state.nodes.len(), state.handles.len());
    }
    fn lookup(&self, _: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        self.count(1);
        entry(reply, self.lookup_native(parent, name));
    }
    fn forget(&self, _: &Request, ino: INodeNo, n: u64) {
        self.count(2);
        self.forget_native(ino, n);
    }
    // fuser 0.18's ForgetOne is not public. Its ordinary default performs the
    // exact individual forget callbacks. Batch kernel-request count remains
    // unavailable; callback counts cannot establish that opcode's cardinality.
    fn getattr(&self, _: &Request, ino: INodeNo, fh: Option<FileHandle>, reply: ReplyAttr) {
        self.count(3);
        let result = (|| {
            let f = match fh {
                Some(h) => self.handle(h.0)?,
                None => self.node(ino)?,
            };
            attr(&f.metadata().map_err(error)?, ino)
        })();
        match result {
            Ok(a) => reply.attr(&TTL, &a),
            Err(e) => reply.error(e),
        }
    }
    fn setattr(
        &self,
        _: &Request,
        ino: INodeNo,
        mode: Option<u32>,
        uid: Option<u32>,
        gid: Option<u32>,
        size: Option<u64>,
        atime: Option<TimeOrNow>,
        mtime: Option<TimeOrNow>,
        _: Option<SystemTime>,
        fh: Option<FileHandle>,
        _: Option<SystemTime>,
        _: Option<SystemTime>,
        _: Option<SystemTime>,
        flags: Option<BsdFileFlags>,
        reply: ReplyAttr,
    ) {
        self.count(4);
        let result = (|| {
            if flags.is_some() {
                return Err(Errno::ENOTSUP);
            }
            let node = self.node(ino)?;
            let file = match fh {
                Some(h) => self.handle(h.0)?,
                None => std::sync::Arc::new(reopen(
                    &node,
                    if size.is_some() {
                        libc::O_WRONLY
                    } else {
                        libc::O_RDONLY | libc::O_NONBLOCK
                    },
                )?),
            };
            let fd = file.as_raw_fd();
            if let Some(mode) = mode {
                checked(unsafe { libc::fchmod(fd, mode) })?;
            }
            if uid.is_some() || gid.is_some() {
                checked(unsafe {
                    libc::fchown(fd, uid.unwrap_or(u32::MAX), gid.unwrap_or(u32::MAX))
                })?;
            }
            if let Some(size) = size {
                checked(unsafe {
                    libc::ftruncate(fd, size.try_into().map_err(|_| Errno::EOVERFLOW)?)
                })?;
            }
            if atime.is_some() || mtime.is_some() {
                let times = [timespec(atime)?, timespec(mtime)?];
                checked(unsafe { libc::futimens(fd, times.as_ptr()) })?;
            }
            attr(&node.metadata().map_err(error)?, ino)
        })();
        match result {
            Ok(a) => reply.attr(&TTL, &a),
            Err(e) => reply.error(e),
        }
    }
    fn readlink(&self, _: &Request, ino: INodeNo, reply: ReplyData) {
        self.count(5);
        let result = (|| {
            let file = self.node(ino)?;
            let mut bytes = vec![0u8; 4096];
            let size = unsafe {
                libc::readlinkat(
                    file.as_raw_fd(),
                    c"".as_ptr(),
                    bytes.as_mut_ptr().cast(),
                    bytes.len(),
                )
            };
            if size < 0 {
                return Err(error(io::Error::last_os_error()));
            }
            if size as usize == bytes.len() {
                return Err(Errno::ENAMETOOLONG);
            }
            bytes.truncate(size as usize);
            Ok(bytes)
        })();
        match result {
            Ok(bytes) => reply.data(&bytes),
            Err(e) => reply.error(e),
        }
    }
    fn mknod(
        &self,
        req: &Request,
        parent: INodeNo,
        name: &OsStr,
        mode: u32,
        umask: u32,
        rdev: u32,
        reply: ReplyEntry,
    ) {
        self.count(8);
        let result = (|| {
            let p = self.node(parent)?;
            let n = cstring(name)?;
            checked(unsafe {
                libc::mknodat(p.as_raw_fd(), n.as_ptr(), mode & !umask, rdev.into())
            })?;
            self.creation_owner(req, parent, name)?;
            self.lookup_native(parent, name)
        })();
        entry(reply, result);
    }
    fn mkdir(
        &self,
        req: &Request,
        parent: INodeNo,
        name: &OsStr,
        mode: u32,
        umask: u32,
        reply: ReplyEntry,
    ) {
        self.count(9);
        let result = (|| {
            let p = self.node(parent)?;
            let n = cstring(name)?;
            checked(unsafe { libc::mkdirat(p.as_raw_fd(), n.as_ptr(), mode & !umask) })?;
            self.creation_owner(req, parent, name)?;
            self.lookup_native(parent, name)
        })();
        entry(reply, result);
    }
    fn unlink(&self, _: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        self.count(10);
        empty(reply, self.unlink_native(parent, name, 0));
    }
    fn rmdir(&self, _: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        self.count(11);
        empty(reply, self.unlink_native(parent, name, libc::AT_REMOVEDIR));
    }
    fn symlink(
        &self,
        req: &Request,
        parent: INodeNo,
        name: &OsStr,
        target: &Path,
        reply: ReplyEntry,
    ) {
        self.count(6);
        let result = (|| {
            let p = self.node(parent)?;
            let n = cstring(name)?;
            let t = cstring(target.as_os_str())?;
            checked(unsafe { libc::symlinkat(t.as_ptr(), p.as_raw_fd(), n.as_ptr()) })?;
            self.creation_owner(req, parent, name)?;
            self.lookup_native(parent, name)
        })();
        entry(reply, result);
    }
    fn rename(
        &self,
        _: &Request,
        parent: INodeNo,
        name: &OsStr,
        newparent: INodeNo,
        newname: &OsStr,
        flags: RenameFlags,
        reply: ReplyEmpty,
    ) {
        self.count(12);
        let result = (|| {
            let p = self.node(parent)?;
            let q = self.node(newparent)?;
            let n = cstring(name)?;
            let m = cstring(newname)?;
            checked(unsafe {
                libc::renameat2(
                    p.as_raw_fd(),
                    n.as_ptr(),
                    q.as_raw_fd(),
                    m.as_ptr(),
                    flags.bits(),
                )
            })
        })();
        empty(reply, result);
    }
    fn link(&self, _: &Request, ino: INodeNo, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        self.count(13);
        let result = (|| {
            let file = self.node(ino)?;
            let p = self.node(parent)?;
            let source =
                std::ffi::CString::new(format!("/proc/self/fd/{}", file.as_raw_fd())).unwrap();
            let n = cstring(name)?;
            checked(unsafe {
                libc::linkat(
                    libc::AT_FDCWD,
                    source.as_ptr(),
                    p.as_raw_fd(),
                    n.as_ptr(),
                    libc::AT_SYMLINK_FOLLOW,
                )
            })?;
            self.lookup_native(parent, name)
        })();
        entry(reply, result);
    }
    fn open(&self, _: &Request, ino: INodeNo, flags: OpenFlags, reply: ReplyOpen) {
        self.count(14);
        let result = (|| {
            let node = self.node(ino)?;
            let file = reopen(&node, flags.0 & !libc::O_APPEND)?;
            self.put_handle(file)
        })();
        match result {
            Ok(h) => reply.opened(FileHandle(h), FopenFlags::FOPEN_KEEP_CACHE),
            Err(e) => reply.error(e),
        }
    }
    fn create(
        &self,
        req: &Request,
        parent: INodeNo,
        name: &OsStr,
        mode: u32,
        umask: u32,
        flags: i32,
        reply: ReplyCreate,
    ) {
        self.count(35);
        let result = (|| {
            let p = self.node(parent)?;
            let n = cstring(name)?;
            let file = owned_fd(unsafe {
                libc::openat(
                    p.as_raw_fd(),
                    n.as_ptr(),
                    (flags & !libc::O_APPEND) | libc::O_CREAT | libc::O_CLOEXEC,
                    mode & !umask,
                )
            })?;
            self.creation_owner(req, parent, name)?;
            let a = self.lookup_native(parent, name)?;
            let h = self.put_handle(file)?;
            Ok((a, h))
        })();
        match result {
            Ok((a, h)) => reply.created(
                &TTL,
                &a,
                Generation(0),
                FileHandle(h),
                FopenFlags::FOPEN_KEEP_CACHE,
            ),
            Err(e) => reply.error(e),
        }
    }
    fn read(
        &self,
        _: &Request,
        _: INodeNo,
        fh: FileHandle,
        offset: u64,
        size: u32,
        _: OpenFlags,
        _: Option<LockOwner>,
        reply: ReplyData,
    ) {
        self.count(15);
        let result = (|| {
            if size as usize > WINDOW {
                return Err(Errno::EINVAL);
            }
            let file = self.handle(fh.0)?;
            let mut bytes = vec![0; size as usize];
            let n = file.read_at(&mut bytes, offset).map_err(error)?;
            if n < bytes.len()
                && offset.saturating_add(n as u64) < file.metadata().map_err(error)?.len()
            {
                return Err(Errno::EIO);
            }
            bytes.truncate(n);
            Ok(bytes)
        })();
        match result {
            Ok(bytes) => reply.data(&bytes),
            Err(e) => reply.error(e),
        }
    }
    fn write(
        &self,
        _: &Request,
        _: INodeNo,
        fh: FileHandle,
        offset: u64,
        bytes: &[u8],
        _: WriteFlags,
        _: OpenFlags,
        _: Option<LockOwner>,
        reply: ReplyWrite,
    ) {
        self.count(16);
        let result = (|| {
            if bytes.len() > WINDOW {
                return Err(Errno::EINVAL);
            }
            let written = self.handle(fh.0)?.write_at(bytes, offset).map_err(error)?;
            if written != bytes.len() {
                return Err(Errno::EIO);
            }
            Ok(written)
        })();
        match result {
            Ok(n) => reply.written(n as u32),
            Err(e) => reply.error(e),
        }
    }
    fn flush(&self, _: &Request, _: INodeNo, fh: FileHandle, _: LockOwner, reply: ReplyEmpty) {
        self.count(25);
        empty(reply, self.handle(fh.0).map(|_| ()));
    }
    fn release(
        &self,
        _: &Request,
        _: INodeNo,
        fh: FileHandle,
        _: OpenFlags,
        _: Option<LockOwner>,
        _: bool,
        reply: ReplyEmpty,
    ) {
        self.count(18);
        empty(reply, self.release_handle(fh.0));
    }
    fn opendir(&self, _: &Request, ino: INodeNo, _: OpenFlags, reply: ReplyOpen) {
        self.count(27);
        let result = (|| {
            let node = self.node(ino)?;
            self.put_handle(reopen(&node, libc::O_RDONLY | libc::O_DIRECTORY)?)
        })();
        match result {
            Ok(h) => reply.opened(FileHandle(h), FopenFlags::empty()),
            Err(e) => reply.error(e),
        }
    }
    fn readdir(
        &self,
        _: &Request,
        ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        mut reply: ReplyDirectory,
    ) {
        self.count(28);
        let result = (|| {
            let file = self.handle(fh.0)?;
            let root_inode = self
                .state
                .lock()
                .unwrap()
                .nodes
                .get(&INodeNo::ROOT.0)
                .ok_or(Errno::ESTALE)?
                .key
                .1;
            // Reopen gives an independent file position; bounded libc DIR buffer.
            let copy = reopen(&file, libc::O_RDONLY | libc::O_DIRECTORY)?;
            let raw = copy.into_raw_fd();
            let dir = unsafe { libc::fdopendir(raw) };
            if dir.is_null() {
                let e = error(io::Error::last_os_error());
                unsafe {
                    libc::close(raw);
                }
                return Err(e);
            }
            let result = (|| {
                let cookie = i64::try_from(offset).map_err(|_| Errno::EOVERFLOW)?;
                unsafe {
                    libc::seekdir(dir, cookie);
                }
                loop {
                    unsafe {
                        *libc::__errno_location() = 0;
                    }
                    let next = unsafe { libc::readdir(dir) };
                    if next.is_null() {
                        let e = unsafe { *libc::__errno_location() };
                        return if e == 0 {
                            Ok(())
                        } else {
                            Err(Errno::from_i32(e))
                        };
                    }
                    let item = unsafe { &*next };
                    let name = unsafe { CStr::from_ptr(item.d_name.as_ptr()) };
                    let kind = match item.d_type {
                        libc::DT_DIR => FileType::Directory,
                        libc::DT_LNK => FileType::Symlink,
                        libc::DT_FIFO => FileType::NamedPipe,
                        libc::DT_SOCK => FileType::Socket,
                        libc::DT_CHR => FileType::CharDevice,
                        libc::DT_BLK => FileType::BlockDevice,
                        _ => FileType::RegularFile,
                    };
                    let id = if name.to_bytes() == b"."
                        || (ino == INodeNo::ROOT && name.to_bytes() == b"..")
                    {
                        ino.0
                    } else if item.d_ino == root_inode {
                        INodeNo::ROOT.0
                    } else {
                        item.d_ino.checked_add(1).ok_or(Errno::EOVERFLOW)?
                    };
                    if reply.add(
                        INodeNo(id),
                        item.d_off.try_into().map_err(|_| Errno::EOVERFLOW)?,
                        kind,
                        OsStr::from_bytes(name.to_bytes()),
                    ) {
                        return Ok(());
                    }
                }
            })();
            checked(unsafe { libc::closedir(dir) })?;
            result
        })();
        match result {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(e),
        }
    }
    fn releasedir(&self, _: &Request, _: INodeNo, fh: FileHandle, _: OpenFlags, reply: ReplyEmpty) {
        self.count(29);
        empty(reply, self.release_handle(fh.0));
    }
    fn statfs(&self, _: &Request, ino: INodeNo, reply: ReplyStatfs) {
        self.count(17);
        let result = (|| {
            let file = self.node(ino)?;
            let mut info = std::mem::MaybeUninit::<libc::statvfs>::uninit();
            checked(unsafe { libc::fstatvfs(file.as_raw_fd(), info.as_mut_ptr()) })?;
            Ok(unsafe { info.assume_init() })
        })();
        match result {
            Ok(s) => reply.statfs(
                s.f_blocks,
                s.f_bfree,
                s.f_bavail,
                s.f_files,
                s.f_ffree,
                s.f_bsize as u32,
                s.f_namemax as u32,
                s.f_frsize as u32,
            ),
            Err(e) => reply.error(e),
        }
    }
    fn getxattr(&self, _: &Request, _: INodeNo, _: &OsStr, _: u32, reply: ReplyXattr) {
        self.count(22);
        reply.error(Errno::ENOSYS);
    }
    fn listxattr(&self, _: &Request, _: INodeNo, _: u32, reply: ReplyXattr) {
        self.count(23);
        reply.error(Errno::ENOSYS);
    }
    fn setxattr(
        &self,
        _: &Request,
        _: INodeNo,
        _: &OsStr,
        _: &[u8],
        _: i32,
        _: u32,
        reply: ReplyEmpty,
    ) {
        self.count(21);
        reply.error(Errno::ENOSYS);
    }
    fn removexattr(&self, _: &Request, _: INodeNo, _: &OsStr, reply: ReplyEmpty) {
        self.count(24);
        reply.error(Errno::ENOSYS);
    }
    fn fsync(&self, _: &Request, _: INodeNo, _: FileHandle, _: bool, reply: ReplyEmpty) {
        self.count(20);
        reply.ok();
    }
    fn fsyncdir(&self, _: &Request, _: INodeNo, _: FileHandle, _: bool, reply: ReplyEmpty) {
        self.count(30);
        reply.ok();
    }
    fn readdirplus(
        &self,
        _: &Request,
        _: INodeNo,
        _: FileHandle,
        _: u64,
        reply: ReplyDirectoryPlus,
    ) {
        self.count(44);
        reply.error(Errno::ENOSYS);
    }
}
impl Passthrough {
    fn creation_owner(&self, req: &Request, parent: INodeNo, name: &OsStr) -> Result<()> {
        let parent = self.node(parent)?;
        let pmeta = parent.metadata().map_err(error)?;
        let group = if pmeta.mode() & libc::S_ISGID != 0 {
            pmeta.gid()
        } else {
            req.gid()
        };
        let name = cstring(name)?;
        let file = owned_fd(unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        })?;
        let meta = file.metadata().map_err(error)?;
        if meta.uid() == req.uid() && meta.gid() == group {
            return Ok(());
        }
        checked(unsafe {
            libc::fchownat(
                file.as_raw_fd(),
                c"".as_ptr(),
                req.uid(),
                group,
                libc::AT_EMPTY_PATH | libc::AT_SYMLINK_NOFOLLOW,
            )
        })
    }
    fn unlink_native(&self, parent: INodeNo, name: &OsStr, flags: i32) -> Result<()> {
        let p = self.node(parent)?;
        let n = cstring(name)?;
        checked(unsafe { libc::unlinkat(p.as_raw_fd(), n.as_ptr(), flags) })
    }
}
