use crate::adapter::{
    empty_reply, errno, file_type, LayerFs, O_ACCMODE, O_RDWR, O_TRUNC, O_WRONLY, TTL,
};
use crate::Kind;
use fuser::{
    AccessFlags, BsdFileFlags, FileHandle, Filesystem, FopenFlags, Generation, INodeNo, InitFlags,
    KernelConfig, LockOwner, OpenFlags, RenameFlags, ReplyAttr, ReplyCreate, ReplyData,
    ReplyDirectory, ReplyDirectoryPlus, ReplyEmpty, ReplyEntry, ReplyOpen, ReplyStatfs, ReplyWrite,
    Request, TimeOrNow, WriteFlags,
};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

impl Filesystem for LayerFs {
    fn init(&mut self, _request: &Request, config: &mut KernelConfig) -> std::io::Result<()> {
        let max_write = config
            .set_max_write(1024 * 1024)
            .map(|_| 1024 * 1024)
            .unwrap_or_else(|limit| {
                let _ = config.set_max_write(limit);
                limit
            });
        self.port.note_fuse_max_write(max_write);
        let max_readahead = config
            .set_max_readahead(1024 * 1024)
            .map(|_| 1024 * 1024)
            .unwrap_or_else(|limit| {
                let _ = config.set_max_readahead(limit);
                limit
            });
        let wanted = InitFlags::FUSE_ASYNC_READ
            | InitFlags::FUSE_BIG_WRITES
            | InitFlags::FUSE_PARALLEL_DIROPS
            | InitFlags::FUSE_DO_READDIRPLUS
            | InitFlags::FUSE_READDIRPLUS_AUTO
            | InitFlags::FUSE_MAX_PAGES;
        let capabilities = config.capabilities() & wanted;
        let _ = config.add_capabilities(capabilities);
        #[cfg(target_os = "linux")]
        let capability_bits = capabilities.bits();
        #[cfg(not(target_os = "linux"))]
        let capability_bits = u64::from(capabilities.bits());
        self.port
            .note_fuse_read_config(max_readahead, capability_bits);
        let _ = config.set_max_background(64);
        let _ = config.set_congestion_threshold(48);
        config
            .set_time_granularity(Duration::from_nanos(1))
            .map(|_| ())
            .map_err(|_| std::io::Error::other("time granularity"))
    }

    fn lookup(&self, _request: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        self.port
            .note_kernel_operation(crate::KernelOperation::Lookup);
        let result = self.node(parent).and_then(|parent| {
            self.port
                .lookup(parent, name.as_bytes())
                .map_err(errno)
                .and_then(|attr| self.attr(attr))
        });
        match result {
            Ok(attr) => reply.entry(&TTL, &attr, Generation(0)),
            Err(error) => reply.error(error),
        }
    }

    fn getattr(
        &self,
        _request: &Request,
        ino: INodeNo,
        _handle: Option<FileHandle>,
        reply: ReplyAttr,
    ) {
        self.port
            .note_kernel_operation(crate::KernelOperation::Getattr);
        let result = self.node(ino).and_then(|node| {
            self.port
                .attr(node)
                .map_err(errno)
                .and_then(|attr| self.attr(attr))
        });
        match result {
            Ok(attr) => reply.attr(&TTL, &attr),
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
        self.port
            .note_kernel_operation(crate::KernelOperation::Setattr);
        if uid.is_some_and(|value| value != self.uid)
            || gid.is_some_and(|value| value != self.gid)
            || flags.is_some()
        {
            reply.error(fuser::Errno::EOPNOTSUPP);
            return;
        }
        let result = self.node(ino).and_then(|node| {
            if let Some(size) = size {
                self.port.truncate(node, size).map_err(errno)?;
            }
            if let Some(mode) = mode {
                self.port.chmod(node, mode).map_err(errno)?;
            }
            if let Some(value) = mtime {
                let value = match value {
                    TimeOrNow::SpecificTime(value) => value,
                    TimeOrNow::Now => SystemTime::now(),
                };
                let value = value
                    .duration_since(UNIX_EPOCH)
                    .map_err(|_| fuser::Errno::EINVAL)?;
                self.port
                    .set_mtime(node, value.as_secs() as i64, value.subsec_nanos())
                    .map_err(errno)?;
            }
            self.attr(self.port.attr(node).map_err(errno)?)
        });
        match result {
            Ok(attr) => reply.attr(&TTL, &attr),
            Err(error) => reply.error(error),
        }
    }

    fn readlink(&self, _request: &Request, ino: INodeNo, reply: ReplyData) {
        self.port
            .note_kernel_operation(crate::KernelOperation::Readlink);
        match self
            .node(ino)
            .and_then(|node| self.port.readlink(node).map_err(errno))
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
        self.port
            .note_kernel_operation(crate::KernelOperation::Mknod);
        if rdev != 0 || mode & 0o170000 != 0o100000 {
            reply.error(fuser::Errno::EOPNOTSUPP);
            return;
        }
        let result = self.node(parent).and_then(|parent| {
            let attr = self
                .port
                .create_file(parent, name.as_bytes(), mode & !umask)
                .map_err(errno)?;
            self.attr(attr)
        });
        match result {
            Ok(attr) => reply.entry(&TTL, &attr, Generation(0)),
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
        self.port
            .note_kernel_operation(crate::KernelOperation::Mkdir);
        let result = self.node(parent).and_then(|parent| {
            let attr = self
                .port
                .mkdir(parent, name.as_bytes(), mode & !umask)
                .map_err(errno)?;
            self.attr(attr)
        });
        match result {
            Ok(attr) => reply.entry(&TTL, &attr, Generation(0)),
            Err(error) => reply.error(error),
        }
    }

    fn unlink(&self, _request: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        self.port
            .note_kernel_operation(crate::KernelOperation::Unlink);
        empty_reply(
            self.node(parent).and_then(|parent| {
                self.port
                    .unlink(parent, name.as_bytes(), false)
                    .map_err(errno)
            }),
            reply,
        );
    }

    fn rmdir(&self, _request: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        self.port
            .note_kernel_operation(crate::KernelOperation::Rmdir);
        empty_reply(
            self.node(parent).and_then(|parent| {
                self.port
                    .unlink(parent, name.as_bytes(), true)
                    .map_err(errno)
            }),
            reply,
        );
    }

    fn symlink(
        &self,
        _request: &Request,
        parent: INodeNo,
        name: &OsStr,
        target: &Path,
        reply: ReplyEntry,
    ) {
        self.port
            .note_kernel_operation(crate::KernelOperation::Symlink);
        let result = self.node(parent).and_then(|parent| {
            let attr = self
                .port
                .symlink(
                    parent,
                    name.as_bytes(),
                    target.as_os_str().as_bytes().to_vec(),
                )
                .map_err(errno)?;
            self.attr(attr)
        });
        match result {
            Ok(attr) => reply.entry(&TTL, &attr, Generation(0)),
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
        self.port
            .note_kernel_operation(crate::KernelOperation::Rename);
        if flags.intersects(RenameFlags::RENAME_EXCHANGE | RenameFlags::RENAME_WHITEOUT) {
            reply.error(fuser::Errno::EOPNOTSUPP);
            return;
        }
        empty_reply(
            self.node(parent).and_then(|parent| {
                let target = self.node(new_parent)?;
                self.port
                    .rename(
                        parent,
                        name.as_bytes(),
                        target,
                        new_name.as_bytes(),
                        flags.contains(RenameFlags::RENAME_NOREPLACE),
                    )
                    .map_err(errno)
            }),
            reply,
        );
    }

    fn link(
        &self,
        _request: &Request,
        ino: INodeNo,
        new_parent: INodeNo,
        new_name: &OsStr,
        reply: ReplyEntry,
    ) {
        self.port
            .note_kernel_operation(crate::KernelOperation::Link);
        let result = self.node(ino).and_then(|node| {
            let parent = self.node(new_parent)?;
            let attr = self
                .port
                .link(node, parent, new_name.as_bytes())
                .map_err(errno)?;
            self.attr(attr)
        });
        match result {
            Ok(attr) => reply.entry(&TTL, &attr, Generation(0)),
            Err(error) => reply.error(error),
        }
    }

    fn open(&self, _request: &Request, ino: INodeNo, flags: OpenFlags, reply: ReplyOpen) {
        self.port
            .note_kernel_operation(crate::KernelOperation::Open);
        match self.node(ino).and_then(|node| {
            let writable = matches!(flags.0 & O_ACCMODE, O_WRONLY | O_RDWR);
            self.open_handle(node, flags.0 & O_TRUNC != 0, writable)
                .map(|handle| (handle, writable))
        }) {
            Ok((handle, _)) => reply.opened(FileHandle(handle), FopenFlags::FOPEN_KEEP_CACHE),
            Err(error) => reply.error(error),
        }
    }

    fn read(
        &self,
        _request: &Request,
        _ino: INodeNo,
        handle: FileHandle,
        offset: u64,
        size: u32,
        _flags: OpenFlags,
        _lock_owner: Option<LockOwner>,
        reply: ReplyData,
    ) {
        self.port
            .note_kernel_operation(crate::KernelOperation::Read);
        match self
            .handle(handle)
            .and_then(|node| self.port.read(node, offset, size as usize).map_err(errno))
        {
            Ok(bytes) => reply.data(&bytes),
            Err(error) => reply.error(error),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn write(
        &self,
        _request: &Request,
        _ino: INodeNo,
        handle: FileHandle,
        offset: u64,
        data: &[u8],
        _write_flags: WriteFlags,
        _flags: OpenFlags,
        _lock_owner: Option<LockOwner>,
        reply: ReplyWrite,
    ) {
        self.port
            .note_kernel_operation(crate::KernelOperation::Write);
        match self
            .handle(handle)
            .and_then(|node| self.port.write(node, offset, data).map_err(errno))
        {
            Ok(size) => reply.written(size as u32),
            Err(error) => reply.error(error),
        }
    }

    fn flush(
        &self,
        _request: &Request,
        _ino: INodeNo,
        _handle: FileHandle,
        _lock_owner: LockOwner,
        reply: ReplyEmpty,
    ) {
        self.port
            .note_kernel_operation(crate::KernelOperation::Flush);
        reply.ok();
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
        self.port
            .note_kernel_operation(crate::KernelOperation::Release);
        let result = self
            .handles
            .remove(handle.0)
            .ok_or(fuser::Errno::EBADF)
            .and_then(|handle| self.port.unpin(handle.node, handle.writable).map_err(errno));
        empty_reply(result, reply);
    }

    fn fsync(
        &self,
        _request: &Request,
        _ino: INodeNo,
        handle: FileHandle,
        _datasync: bool,
        reply: ReplyEmpty,
    ) {
        self.port
            .note_kernel_operation(crate::KernelOperation::Fsync);
        empty_reply(
            self.handle(handle)
                .and_then(|node| self.port.fsync(Some(node)).map_err(errno)),
            reply,
        );
    }

    fn opendir(&self, _request: &Request, ino: INodeNo, _flags: OpenFlags, reply: ReplyOpen) {
        self.port
            .note_kernel_operation(crate::KernelOperation::Opendir);
        match self.node(ino).and_then(|node| {
            if self.port.attr(node).map_err(errno)?.kind != Kind::Directory {
                return Err(fuser::Errno::ENOTDIR);
            }
            Ok(self.handles.insert(node, false))
        }) {
            Ok(handle) => reply.opened(FileHandle(handle), FopenFlags::empty()),
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
        self.port
            .note_kernel_operation(crate::KernelOperation::Readdir);
        let result = self
            .handle(handle)
            .and_then(|node| self.port.readdir(node).map_err(errno));
        match result {
            Ok(entries) => {
                let mut returned_entries = 0;
                for (index, (node, kind, name)) in
                    entries.into_iter().enumerate().skip(offset as usize)
                {
                    let ino = self.inodes.kernel(node);
                    if reply.add(
                        INodeNo(ino),
                        (index + 1) as u64,
                        file_type(kind),
                        OsStr::from_bytes(&name),
                    ) {
                        break;
                    }
                    returned_entries += 1;
                }
                self.port.note_readdir_page(offset, returned_entries);
                reply.ok();
            }
            Err(error) => reply.error(error),
        }
    }

    fn readdirplus(
        &self,
        _request: &Request,
        _ino: INodeNo,
        handle: FileHandle,
        offset: u64,
        mut reply: ReplyDirectoryPlus,
    ) {
        self.port
            .note_kernel_operation(crate::KernelOperation::Readdirplus);
        let result = self
            .handle(handle)
            .and_then(|node| self.port.readdirplus(node).map_err(errno));
        match result {
            Ok(entries) => {
                let mut returned_entries = 0;
                for (index, (attr, name)) in entries.into_iter().enumerate().skip(offset as usize) {
                    let attr = match self.attr(attr) {
                        Ok(attr) => attr,
                        Err(error) => {
                            reply.error(error);
                            return;
                        }
                    };
                    if reply.add(
                        attr.ino,
                        (index + 1) as u64,
                        OsStr::from_bytes(&name),
                        &TTL,
                        &attr,
                        Generation(0),
                    ) {
                        break;
                    }
                    returned_entries += 1;
                }
                self.port.note_readdir_page(offset, returned_entries);
                reply.ok();
            }
            Err(error) => reply.error(error),
        }
    }

    fn releasedir(
        &self,
        _request: &Request,
        _ino: INodeNo,
        handle: FileHandle,
        _flags: OpenFlags,
        reply: ReplyEmpty,
    ) {
        self.port
            .note_kernel_operation(crate::KernelOperation::Releasedir);
        if self.handles.remove(handle.0).is_some() {
            reply.ok();
        } else {
            reply.error(fuser::Errno::EBADF);
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
        self.port
            .note_kernel_operation(crate::KernelOperation::Fsyncdir);
        empty_reply(self.port.fsync(None).map_err(errno), reply);
    }

    fn statfs(&self, _request: &Request, _ino: INodeNo, reply: ReplyStatfs) {
        self.port
            .note_kernel_operation(crate::KernelOperation::Statfs);
        reply.statfs(1 << 30, 1 << 29, 1 << 29, 1 << 30, 1 << 29, 4096, 255, 4096);
    }

    fn access(&self, _request: &Request, ino: INodeNo, _mask: AccessFlags, reply: ReplyEmpty) {
        self.port
            .note_kernel_operation(crate::KernelOperation::Access);
        match self
            .node(ino)
            .and_then(|node| self.port.attr(node).map_err(errno))
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
        _flags: i32,
        reply: ReplyCreate,
    ) {
        self.port
            .note_kernel_operation(crate::KernelOperation::Create);
        let result = self.node(parent).and_then(|parent| {
            let attr = self
                .port
                .create_file_open(parent, name.as_bytes(), mode & !umask)
                .map_err(errno)?;
            let handle = self.handles.insert(attr.node, true);
            Ok((self.attr(attr)?, handle))
        });
        match result {
            // Created files bypass the kernel page cache so large sequential writes stay
            // memory-bounded. These handles are coherent but not mmapable; a later open uses
            // FOPEN_KEEP_CACHE and supports mmap after the create handle closes.
            Ok((attr, handle)) => reply.created(
                &TTL,
                &attr,
                Generation(0),
                FileHandle(handle),
                FopenFlags::FOPEN_DIRECT_IO,
            ),
            Err(error) => reply.error(error),
        }
    }
}
