macro_rules! directory_callbacks {
    () => {
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
            match self.lock().and_then(|mut workspace| {
                workspace.release(MountedHandleId(handle.0)).map_err(errno)
            }) {
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
            if let Err(error) = self.budget.pause_and_wait() {
                reply.error(errno(error));
                return;
            }
            let result = self
                .lock()
                .and_then(|mut workspace| workspace.fsyncdir().map_err(errno));
            self.budget.resume();
            match result {
                Ok(_) => reply.ok(),
                Err(error) => reply.error(error),
            }
        }
    };
}
