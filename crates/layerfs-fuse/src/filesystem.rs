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
        self.stateless_open = self.port.supports_kernel_lifetime()
            && config
                .capabilities()
                .contains(InitFlags::FUSE_NO_OPEN_SUPPORT);
        let mut wanted = InitFlags::FUSE_ASYNC_READ
            | InitFlags::FUSE_BIG_WRITES
            | InitFlags::FUSE_PARALLEL_DIROPS
            | InitFlags::FUSE_DO_READDIRPLUS
            | InitFlags::FUSE_READDIRPLUS_AUTO
            | InitFlags::FUSE_MAX_PAGES;
        if self.stateless_open {
            wanted |= InitFlags::FUSE_NO_OPEN_SUPPORT;
        }
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

    fn destroy(&mut self) {
        if self.stateless_open {
            let _ = self.port.kernel_detach();
        }
    }

    fn forget(&self, _request: &Request, ino: INodeNo, nlookup: u64) {
        if self.stateless_open {
            if let Ok(node) = self.node(ino) {
                let _ = self.port.kernel_forget(node, nlookup);
            }
        }
    }

    fn lookup(&self, _request: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        self.port
            .note_kernel_operation(crate::KernelOperation::Lookup);
        let _callback = match self
            .port
            .admit_callback(crate::KernelOperation::Lookup, 0, false)
        {
            Ok(guard) => guard,
            Err(error) => {
                reply.error(errno(error));
                return;
            }
        };
        let name = name.to_owned();
        let this = self.clone();
        self.dispatch(async move {
            let mut _callback = _callback;
            _callback._gate = match this
                .port
                .callback_gate(crate::KernelOperation::Lookup, false)
                .await
            {
                Ok(gate) => gate,
                Err(error) => {
                    reply.error(errno(error));
                    return;
                }
            };

            let result = async {
                let parent = this.node(parent)?;
                this.entry_async(parent, name.as_bytes(), crate::KernelEntry::Lookup)
                    .await
            }
            .await;
            match result {
                Ok((attr, references)) => {
                    reply.entry(&TTL, &attr, Generation(0));
                    references.submitted();
                }
                Err(error) => reply.error(error),
            }
        });
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
        let _callback = match self
            .port
            .admit_callback(crate::KernelOperation::Getattr, 0, false)
        {
            Ok(guard) => guard,
            Err(error) => {
                reply.error(errno(error));
                return;
            }
        };

        let this = self.clone();
        self.dispatch(async move {
            let mut _callback = _callback;
            _callback._gate = match this
                .port
                .callback_gate(crate::KernelOperation::Getattr, false)
                .await
            {
                Ok(gate) => gate,
                Err(error) => {
                    reply.error(errno(error));
                    return;
                }
            };

            let result = async {
                let node = this.node(ino)?;
                this.port
                    .attr_async(node)
                    .await
                    .map_err(errno)
                    .and_then(|attr| this.attr(attr))
            }
            .await;
            match result {
                Ok(attr) => reply.attr(&TTL, &attr),
                Err(error) => reply.error(error),
            }
        });
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
        let writeback = _request.pid() == 0;
        let _callback =
            match self
                .port
                .admit_callback(crate::KernelOperation::Setattr, 0, writeback)
            {
                Ok(guard) => guard,
                Err(error) => {
                    reply.error(errno(error));
                    return;
                }
            };

        let this = self.clone();
        self.dispatch(async move {
            let mut _callback = _callback;
            _callback._gate = match this
                .port
                .callback_gate(crate::KernelOperation::Setattr, writeback)
                .await
            {
                Ok(gate) => gate,
                Err(error) => {
                    reply.error(errno(error));
                    return;
                }
            };

            if uid.is_some_and(|value| value != this.uid)
                || gid.is_some_and(|value| value != this.gid)
                || flags.is_some()
            {
                reply.error(fuser::Errno::EOPNOTSUPP);
                return;
            }
            let result = async {
                let node = this.node(ino)?;
                // #144 R1a: each applied mutation reports the exact installed
                // attr, so the kernel reply is built from the last mutation
                // instead of a second `Op::Attr` round trip. A SETATTR that
                // mutates nothing (mode/uid/gid/flags unchanged, no size or
                // mtime) still reads the attr once.
                let mut installed = None;
                {
                    if let Some(size) = size {
                        installed =
                            Some(this.port.truncate_async(node, size).await.map_err(errno)?);
                    }
                    if let Some(mode) = mode {
                        installed = Some(this.port.chmod_async(node, mode).await.map_err(errno)?);
                    }
                    if let Some(value) = mtime {
                        let value = match value {
                            TimeOrNow::SpecificTime(value) => value,
                            TimeOrNow::Now => SystemTime::now(),
                        };
                        let value = value
                            .duration_since(UNIX_EPOCH)
                            .map_err(|_| fuser::Errno::EINVAL)?;
                        installed = Some(
                            this.port
                                .set_mtime_async(node, value.as_secs() as i64, value.subsec_nanos())
                                .await
                                .map_err(errno)?,
                        );
                    }
                    let attr = match installed {
                        Some(attr) => attr,
                        None => this.port.attr_async(node).await.map_err(errno)?,
                    };
                    this.attr(attr)
                }
            }
            .await;
            match result {
                Ok(attr) => reply.attr(&TTL, &attr),
                Err(error) => reply.error(error),
            }
        });
    }

    fn readlink(&self, _request: &Request, ino: INodeNo, reply: ReplyData) {
        self.port
            .note_kernel_operation(crate::KernelOperation::Readlink);
        let _callback = match self
            .port
            .admit_callback(crate::KernelOperation::Readlink, 0, false)
        {
            Ok(guard) => guard,
            Err(error) => {
                reply.error(errno(error));
                return;
            }
        };

        let this = self.clone();
        self.dispatch(async move {
            let mut _callback = _callback;
            _callback._gate = match this
                .port
                .callback_gate(crate::KernelOperation::Readlink, false)
                .await
            {
                Ok(gate) => gate,
                Err(error) => {
                    reply.error(errno(error));
                    return;
                }
            };

            match async {
                let node = this.node(ino)?;
                this.port.readlink_async(node).await.map_err(errno)
            }
            .await
            {
                Ok(target) => reply.data(&target),
                Err(error) => reply.error(error),
            }
        });
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
        let _callback = match self
            .port
            .admit_callback(crate::KernelOperation::Mknod, 0, false)
        {
            Ok(guard) => guard,
            Err(error) => {
                reply.error(errno(error));
                return;
            }
        };
        let name = name.to_owned();
        let this = self.clone();
        self.dispatch(async move {
            let mut _callback = _callback;
            _callback._gate = match this
                .port
                .callback_gate(crate::KernelOperation::Mknod, false)
                .await
            {
                Ok(gate) => gate,
                Err(error) => {
                    reply.error(errno(error));
                    return;
                }
            };

            if rdev != 0 || mode & 0o170000 != 0o100000 {
                reply.error(fuser::Errno::EOPNOTSUPP);
                return;
            }
            let result = async {
                let parent = this.node(parent)?;
                this.entry_async(
                    parent,
                    name.as_bytes(),
                    crate::KernelEntry::Create {
                        mode: mode & !umask,
                    },
                )
                .await
            }
            .await;
            match result {
                Ok((attr, references)) => {
                    reply.entry(&TTL, &attr, Generation(0));
                    references.submitted();
                }
                Err(error) => reply.error(error),
            }
        });
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
        let _callback = match self
            .port
            .admit_callback(crate::KernelOperation::Mkdir, 0, false)
        {
            Ok(guard) => guard,
            Err(error) => {
                reply.error(errno(error));
                return;
            }
        };
        let name = name.to_owned();
        let this = self.clone();
        self.dispatch(async move {
            let mut _callback = _callback;
            _callback._gate = match this
                .port
                .callback_gate(crate::KernelOperation::Mkdir, false)
                .await
            {
                Ok(gate) => gate,
                Err(error) => {
                    reply.error(errno(error));
                    return;
                }
            };

            let result = async {
                let parent = this.node(parent)?;
                this.entry_async(
                    parent,
                    name.as_bytes(),
                    crate::KernelEntry::Mkdir {
                        mode: mode & !umask,
                    },
                )
                .await
            }
            .await;
            match result {
                Ok((attr, references)) => {
                    reply.entry(&TTL, &attr, Generation(0));
                    references.submitted();
                }
                Err(error) => reply.error(error),
            }
        });
    }

    fn unlink(&self, _request: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        self.port
            .note_kernel_operation(crate::KernelOperation::Unlink);
        let _callback = match self
            .port
            .admit_callback(crate::KernelOperation::Unlink, 0, false)
        {
            Ok(guard) => guard,
            Err(error) => {
                reply.error(errno(error));
                return;
            }
        };
        let name = name.to_owned();
        let this = self.clone();
        self.dispatch(async move {
            let mut _callback = _callback;
            _callback._gate = match this
                .port
                .callback_gate(crate::KernelOperation::Unlink, false)
                .await
            {
                Ok(gate) => gate,
                Err(error) => {
                    reply.error(errno(error));
                    return;
                }
            };

            empty_reply(
                async {
                    let parent = this.node(parent)?;
                    {
                        this.port
                            .unlink_async(parent, name.as_bytes(), false)
                            .await
                            .map_err(errno)
                    }
                }
                .await,
                reply,
            );
        });
    }

    fn rmdir(&self, _request: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        self.port
            .note_kernel_operation(crate::KernelOperation::Rmdir);
        let _callback = match self
            .port
            .admit_callback(crate::KernelOperation::Rmdir, 0, false)
        {
            Ok(guard) => guard,
            Err(error) => {
                reply.error(errno(error));
                return;
            }
        };
        let name = name.to_owned();
        let this = self.clone();
        self.dispatch(async move {
            let mut _callback = _callback;
            _callback._gate = match this
                .port
                .callback_gate(crate::KernelOperation::Rmdir, false)
                .await
            {
                Ok(gate) => gate,
                Err(error) => {
                    reply.error(errno(error));
                    return;
                }
            };

            empty_reply(
                async {
                    let parent = this.node(parent)?;
                    {
                        this.port
                            .unlink_async(parent, name.as_bytes(), true)
                            .await
                            .map_err(errno)
                    }
                }
                .await,
                reply,
            );
        });
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
        let _callback = match self
            .port
            .admit_callback(crate::KernelOperation::Symlink, 0, false)
        {
            Ok(guard) => guard,
            Err(error) => {
                reply.error(errno(error));
                return;
            }
        };
        let name = name.to_owned();
        let target = target.to_owned();
        let this = self.clone();
        self.dispatch(async move {
            let mut _callback = _callback;
            _callback._gate = match this
                .port
                .callback_gate(crate::KernelOperation::Symlink, false)
                .await
            {
                Ok(gate) => gate,
                Err(error) => {
                    reply.error(errno(error));
                    return;
                }
            };

            let result = async {
                let parent = this.node(parent)?;
                this.entry_async(
                    parent,
                    name.as_bytes(),
                    crate::KernelEntry::Symlink {
                        target: target.as_os_str().as_bytes().to_vec(),
                    },
                )
                .await
            }
            .await;
            match result {
                Ok((attr, references)) => {
                    reply.entry(&TTL, &attr, Generation(0));
                    references.submitted();
                }
                Err(error) => reply.error(error),
            }
        });
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
        let _callback = match self
            .port
            .admit_callback(crate::KernelOperation::Rename, 0, false)
        {
            Ok(guard) => guard,
            Err(error) => {
                reply.error(errno(error));
                return;
            }
        };
        let name = name.to_owned();
        let new_name = new_name.to_owned();
        let this = self.clone();
        self.dispatch(async move {
            let mut _callback = _callback;
            _callback._gate = match this
                .port
                .callback_gate(crate::KernelOperation::Rename, false)
                .await
            {
                Ok(gate) => gate,
                Err(error) => {
                    reply.error(errno(error));
                    return;
                }
            };

            if flags.intersects(RenameFlags::RENAME_EXCHANGE | RenameFlags::RENAME_WHITEOUT) {
                reply.error(fuser::Errno::EOPNOTSUPP);
                return;
            }
            empty_reply(
                async {
                    let parent = this.node(parent)?;
                    {
                        let target = this.node(new_parent)?;
                        this.port
                            .rename_async(
                                parent,
                                name.as_bytes(),
                                target,
                                new_name.as_bytes(),
                                flags.contains(RenameFlags::RENAME_NOREPLACE),
                            )
                            .await
                            .map_err(errno)
                    }
                }
                .await,
                reply,
            );
        });
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
        let _callback = match self
            .port
            .admit_callback(crate::KernelOperation::Link, 0, false)
        {
            Ok(guard) => guard,
            Err(error) => {
                reply.error(errno(error));
                return;
            }
        };
        let new_name = new_name.to_owned();
        let this = self.clone();
        self.dispatch(async move {
            let mut _callback = _callback;
            _callback._gate = match this
                .port
                .callback_gate(crate::KernelOperation::Link, false)
                .await
            {
                Ok(gate) => gate,
                Err(error) => {
                    reply.error(errno(error));
                    return;
                }
            };

            let result = async {
                let parent = this.node(new_parent)?;
                let node = this.node(ino)?;
                this.entry_async(
                    parent,
                    new_name.as_bytes(),
                    crate::KernelEntry::Link { node },
                )
                .await
            }
            .await;
            match result {
                Ok((attr, references)) => {
                    reply.entry(&TTL, &attr, Generation(0));
                    references.submitted();
                }
                Err(error) => reply.error(error),
            }
        });
    }

    fn open(&self, _request: &Request, ino: INodeNo, flags: OpenFlags, reply: ReplyOpen) {
        self.port
            .note_kernel_operation(crate::KernelOperation::Open);
        let _callback = match self
            .port
            .admit_callback(crate::KernelOperation::Open, 0, false)
        {
            Ok(guard) => guard,
            Err(error) => {
                reply.error(errno(error));
                return;
            }
        };

        if self.stateless_open {
            let this = self.clone();
            self.dispatch(async move {
                let _callback = _callback;
                let writable = matches!(flags.0 & O_ACCMODE, O_WRONLY | O_RDWR);
                let result = async {
                    let node = this.node(ino)?;
                    this.port
                        .prepare_kernel_open(node, writable)
                        .await
                        .map_err(errno)
                }
                .await;
                match result {
                    Ok(()) => {
                        let mut opened = FopenFlags::FOPEN_KEEP_CACHE;
                        if !writable {
                            opened |= FopenFlags::FOPEN_NOFLUSH;
                        }
                        reply.opened(FileHandle(0), opened);
                    }
                    Err(error) => reply.error(error),
                }
            });
            return;
        }

        let this = self.clone();
        self.dispatch(async move {
            let mut _callback = _callback;
            _callback._gate = match this
                .port
                .callback_gate(crate::KernelOperation::Open, false)
                .await
            {
                Ok(gate) => gate,
                Err(error) => {
                    reply.error(errno(error));
                    return;
                }
            };

            match async {
                let node = this.node(ino)?;
                {
                    let writable = matches!(flags.0 & O_ACCMODE, O_WRONLY | O_RDWR);
                    this.open_handle_async(node, flags.0 & O_TRUNC != 0, writable)
                        .await
                        .map(|handle| (handle, node))
                }
            }
            .await
            {
                Ok((handle, node)) => {
                    this.port.note_cached_open(node);
                    // Read-only close has no write error to deliver; RELEASE still
                    // drops its pin and writable descriptors retain FLUSH.
                    let mut opened = FopenFlags::FOPEN_KEEP_CACHE;
                    if flags.0 & O_ACCMODE == 0 {
                        opened |= FopenFlags::FOPEN_NOFLUSH;
                    }
                    reply.opened(FileHandle(handle), opened)
                }
                Err(error) => reply.error(error),
            }
        });
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
        self.port
            .note_kernel_operation(crate::KernelOperation::Read);
        let _callback =
            match self
                .port
                .admit_callback(crate::KernelOperation::Read, size as usize, false)
            {
                Ok(guard) => guard,
                Err(error) => {
                    reply.error(errno(error));
                    return;
                }
            };
        match self.file_node(ino, handle) {
            Ok(node) => self.port.submit_read(
                node,
                offset,
                size as usize,
                crate::ReadReply {
                    reply,
                    _guard: _callback,
                    maximum: size as usize,
                },
            ),
            Err(error) => reply.error(error),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn write(
        &self,
        _request: &Request,
        ino: INodeNo,
        handle: FileHandle,
        offset: u64,
        data: &[u8],
        write_flags: WriteFlags,
        _flags: OpenFlags,
        _lock_owner: Option<LockOwner>,
        reply: ReplyWrite,
    ) {
        self.port
            .note_kernel_operation(crate::KernelOperation::Write);
        let _callback = match self.port.admit_callback(
            crate::KernelOperation::Write,
            data.len(),
            write_flags.contains(WriteFlags::FUSE_WRITE_CACHE),
        ) {
            Ok(guard) => guard,
            Err(error) => {
                reply.error(errno(error));
                return;
            }
        };
        match self.file_node(ino, handle) {
            Ok(node) => self.port.submit_write(
                node,
                offset,
                data,
                write_flags.contains(WriteFlags::FUSE_WRITE_CACHE),
                crate::WriteReply {
                    reply,
                    _guard: _callback,
                    maximum: data.len(),
                },
            ),
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
        let _callback = match self
            .port
            .admit_callback(crate::KernelOperation::Flush, 0, false)
        {
            Ok(guard) => guard,
            Err(error) => {
                reply.error(errno(error));
                return;
            }
        };

        let this = self.clone();
        self.dispatch(async move {
            let mut _callback = _callback;
            _callback._gate = match this
                .port
                .callback_gate(crate::KernelOperation::Flush, false)
                .await
            {
                Ok(gate) => gate,
                Err(error) => {
                    reply.error(errno(error));
                    return;
                }
            };

            reply.ok();
        });
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
        // CREATE can release before the kernel has seen its first ENOSYS OPEN.
        if self.stateless_open {
            reply.ok();
            return;
        }
        let _callback = match self
            .port
            .admit_callback(crate::KernelOperation::Release, 0, false)
        {
            Ok(guard) => guard,
            Err(error) => {
                reply.error(errno(error));
                return;
            }
        };

        let this = self.clone();
        self.dispatch(async move {
            let mut _callback = _callback;
            _callback._gate = match this
                .port
                .callback_gate(crate::KernelOperation::Release, false)
                .await
            {
                Ok(gate) => gate,
                Err(error) => {
                    reply.error(errno(error));
                    return;
                }
            };

            let result = async {
                let handle = this.handles.remove(handle.0).ok_or(fuser::Errno::EBADF)?;
                this.port
                    .unpin_async(handle.node, handle.writable)
                    .await
                    .map_err(errno)
            }
            .await;
            empty_reply(result, reply);
        });
    }

    fn fsync(
        &self,
        _request: &Request,
        ino: INodeNo,
        handle: FileHandle,
        _datasync: bool,
        reply: ReplyEmpty,
    ) {
        self.port
            .note_kernel_operation(crate::KernelOperation::Fsync);
        let _callback = match self
            .port
            .admit_callback(crate::KernelOperation::Fsync, 0, false)
        {
            Ok(guard) => guard,
            Err(error) => {
                reply.error(errno(error));
                return;
            }
        };

        let this = self.clone();
        self.dispatch(async move {
            let mut _callback = _callback;
            _callback._gate = match this
                .port
                .callback_gate(crate::KernelOperation::Fsync, false)
                .await
            {
                Ok(gate) => gate,
                Err(error) => {
                    reply.error(errno(error));
                    return;
                }
            };

            empty_reply(
                async {
                    let node = this.file_node(ino, handle)?;
                    this.port.fsync_async(Some(node)).await.map_err(errno)
                }
                .await,
                reply,
            );
        });
    }

    fn opendir(&self, _request: &Request, ino: INodeNo, _flags: OpenFlags, reply: ReplyOpen) {
        self.port
            .note_kernel_operation(crate::KernelOperation::Opendir);
        let _callback = match self
            .port
            .admit_callback(crate::KernelOperation::Opendir, 0, false)
        {
            Ok(guard) => guard,
            Err(error) => {
                reply.error(errno(error));
                return;
            }
        };

        let this = self.clone();
        self.dispatch(async move {
            let mut _callback = _callback;
            _callback._gate = match this
                .port
                .callback_gate(crate::KernelOperation::Opendir, false)
                .await
            {
                Ok(gate) => gate,
                Err(error) => {
                    reply.error(errno(error));
                    return;
                }
            };

            match async {
                let node = this.node(ino)?;
                if this.port.attr_async(node).await.map_err(errno)?.kind != Kind::Directory {
                    return Err(fuser::Errno::ENOTDIR);
                }
                this.port.pin_directory_async(node).await.map_err(errno)?;
                Ok(this.handles.insert(node, false))
            }
            .await
            {
                Ok(handle) => reply.opened(FileHandle(handle), FopenFlags::empty()),
                Err(error) => reply.error(error),
            }
        });
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
        let _callback = match self
            .port
            .admit_callback(crate::KernelOperation::Readdir, 0, false)
        {
            Ok(guard) => guard,
            Err(error) => {
                reply.error(errno(error));
                return;
            }
        };

        let this = self.clone();
        self.dispatch(async move {
            let mut _callback = _callback;
            _callback._gate = match this
                .port
                .callback_gate(crate::KernelOperation::Readdir, false)
                .await
            {
                Ok(gate) => gate,
                Err(error) => {
                    reply.error(errno(error));
                    return;
                }
            };

            let result = async {
                let node = this.handle(handle)?;
                {
                    this.port
                        .directory_page_async(node, offset)
                        .await
                        .map_err(errno)
                }
            }
            .await;
            match result {
                Ok(entries) => {
                    let mut returned_entries = 0;
                    for (cookie, attr, name) in entries {
                        let ino = this.inodes.kernel(attr.node);
                        if reply.add(
                            INodeNo(ino),
                            cookie,
                            file_type(attr.kind),
                            OsStr::from_bytes(&name),
                        ) {
                            break;
                        }
                        returned_entries += 1;
                    }
                    this.port.note_readdir_page(offset, returned_entries);
                    reply.ok();
                }
                Err(error) => reply.error(error),
            }
        });
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
        let _callback =
            match self
                .port
                .admit_callback(crate::KernelOperation::Readdirplus, 0, false)
            {
                Ok(guard) => guard,
                Err(error) => {
                    reply.error(errno(error));
                    return;
                }
            };

        let this = self.clone();
        self.dispatch(async move {
            let mut _callback = _callback;
            _callback._gate = match this
                .port
                .callback_gate(crate::KernelOperation::Readdirplus, false)
                .await
            {
                Ok(gate) => gate,
                Err(error) => {
                    reply.error(errno(error));
                    return;
                }
            };

            let result = async {
                let node = this.handle(handle)?;
                if this.stateless_open {
                    this.port
                        .kernel_directory_page_async(node, offset)
                        .await
                        .map_err(errno)
                } else {
                    this.port
                        .directory_page_async(node, offset)
                        .await
                        .map_err(errno)
                        .map(|entries| (entries, crate::KernelReferences::default()))
                }
            }
            .await;
            match result {
                Ok((entries, mut references)) => {
                    let mut returned_entries = 0;
                    let mut retained_entries = 0;
                    for (cookie, attr, name) in entries {
                        let attr = match this.attr(attr) {
                            Ok(attr) => attr,
                            Err(error) => {
                                reply.error(error);
                                return;
                            }
                        };
                        if reply.add(
                            attr.ino,
                            cookie,
                            OsStr::from_bytes(&name),
                            &TTL,
                            &attr,
                            Generation(0),
                        ) {
                            break;
                        }
                        returned_entries += 1;
                        if name != b"." && name != b".." {
                            retained_entries += 1;
                        }
                    }
                    if this.stateless_open {
                        if let Err(error) = references.release_unemitted(retained_entries) {
                            reply.error(errno(error));
                            return;
                        }
                    }
                    this.port.note_readdir_page(offset, returned_entries);
                    reply.ok();
                    references.submitted();
                }
                Err(error) => reply.error(error),
            }
        });
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
        let _callback = match self
            .port
            .admit_callback(crate::KernelOperation::Releasedir, 0, false)
        {
            Ok(guard) => guard,
            Err(error) => {
                reply.error(errno(error));
                return;
            }
        };

        let this = self.clone();
        self.dispatch(async move {
            let mut _callback = _callback;
            _callback._gate = match this
                .port
                .callback_gate(crate::KernelOperation::Releasedir, false)
                .await
            {
                Ok(gate) => gate,
                Err(error) => {
                    reply.error(errno(error));
                    return;
                }
            };

            match this.handles.remove(handle.0) {
                Some(handle) => match this.port.unpin_directory_async(handle.node).await {
                    Ok(()) => reply.ok(),
                    Err(error) => reply.error(errno(error)),
                },
                None => reply.error(fuser::Errno::EBADF),
            }
        });
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
        let _callback = match self
            .port
            .admit_callback(crate::KernelOperation::Fsyncdir, 0, false)
        {
            Ok(guard) => guard,
            Err(error) => {
                reply.error(errno(error));
                return;
            }
        };

        let this = self.clone();
        self.dispatch(async move {
            let mut _callback = _callback;
            _callback._gate = match this
                .port
                .callback_gate(crate::KernelOperation::Fsyncdir, false)
                .await
            {
                Ok(gate) => gate,
                Err(error) => {
                    reply.error(errno(error));
                    return;
                }
            };

            empty_reply(this.port.fsync_async(None).await.map_err(errno), reply);
        });
    }

    fn statfs(&self, _request: &Request, _ino: INodeNo, reply: ReplyStatfs) {
        self.port
            .note_kernel_operation(crate::KernelOperation::Statfs);
        let _callback = match self
            .port
            .admit_callback(crate::KernelOperation::Statfs, 0, false)
        {
            Ok(guard) => guard,
            Err(error) => {
                reply.error(errno(error));
                return;
            }
        };

        let this = self.clone();
        self.dispatch(async move {
            let mut _callback = _callback;
            _callback._gate = match this
                .port
                .callback_gate(crate::KernelOperation::Statfs, false)
                .await
            {
                Ok(gate) => gate,
                Err(error) => {
                    reply.error(errno(error));
                    return;
                }
            };

            reply.statfs(1 << 30, 1 << 29, 1 << 29, 1 << 30, 1 << 29, 4096, 255, 4096);
        });
    }

    fn access(&self, _request: &Request, ino: INodeNo, _mask: AccessFlags, reply: ReplyEmpty) {
        self.port
            .note_kernel_operation(crate::KernelOperation::Access);
        let _callback = match self
            .port
            .admit_callback(crate::KernelOperation::Access, 0, false)
        {
            Ok(guard) => guard,
            Err(error) => {
                reply.error(errno(error));
                return;
            }
        };

        let this = self.clone();
        self.dispatch(async move {
            let mut _callback = _callback;
            _callback._gate = match this
                .port
                .callback_gate(crate::KernelOperation::Access, false)
                .await
            {
                Ok(gate) => gate,
                Err(error) => {
                    reply.error(errno(error));
                    return;
                }
            };

            match async {
                let node = this.node(ino)?;
                this.port.attr_async(node).await.map_err(errno)
            }
            .await
            {
                Ok(_) => reply.ok(),
                Err(error) => reply.error(error),
            }
        });
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
        let _callback = match self
            .port
            .admit_callback(crate::KernelOperation::Create, 0, false)
        {
            Ok(guard) => guard,
            Err(error) => {
                reply.error(errno(error));
                return;
            }
        };
        let name = name.to_owned();
        let this = self.clone();
        self.dispatch(async move {
            let mut _callback = _callback;
            _callback._gate = match this
                .port
                .callback_gate(crate::KernelOperation::Create, false)
                .await
            {
                Ok(gate) => gate,
                Err(error) => {
                    reply.error(errno(error));
                    return;
                }
            };

            let result = async {
                let parent = this.node(parent)?;
                if this.stateless_open {
                    let (attr, references) = this
                        .entry_async(
                            parent,
                            name.as_bytes(),
                            crate::KernelEntry::Create {
                                mode: mode & !umask,
                            },
                        )
                        .await?;
                    Ok((attr, 0, references))
                } else {
                    let attr = this
                        .port
                        .create_file_open_async(parent, name.as_bytes(), mode & !umask)
                        .await
                        .map_err(errno)?;
                    let handle = this.handles.insert(attr.node, true);
                    Ok((this.attr(attr)?, handle, crate::KernelReferences::default()))
                }
            }
            .await;
            match result {
                // Created files bypass the kernel page cache so large sequential writes stay
                // memory-bounded. These handles are coherent but not mmapable; a later open uses
                // FOPEN_KEEP_CACHE and supports mmap after the create handle closes.
                Ok((attr, handle, references)) => {
                    reply.created(
                        &TTL,
                        &attr,
                        Generation(0),
                        FileHandle(handle),
                        FopenFlags::FOPEN_DIRECT_IO,
                    );
                    references.submitted();
                }
                Err(error) => reply.error(error),
            }
        });
    }
}
