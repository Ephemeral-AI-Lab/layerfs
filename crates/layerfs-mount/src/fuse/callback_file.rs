macro_rules! file_callbacks {
    () => {
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
            match result {
                Ok(size) => reply.written(size as u32),
                Err(error) => reply.error(error),
            }
            drop(reservation);
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
            match self.lock().and_then(|mut workspace| {
                workspace.release(MountedHandleId(handle.0)).map_err(errno)
            }) {
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
            if let Err(error) = self.budget.pause_and_wait() {
                reply.error(errno(error));
                return;
            }
            let result = self
                .lock()
                .and_then(|mut workspace| workspace.fsync().map_err(errno));
            self.budget.resume();
            match result {
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
    };
}
