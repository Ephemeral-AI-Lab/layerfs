macro_rules! namespace_callbacks {
    () => {
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
            if uid.is_some() || gid.is_some() || flags.is_some() {
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
            match result {
                Ok(attr) => reply.entry(&TTL, &self.attr(attr), Generation(0)),
                Err(error) => reply.error(error),
            }
        }
    };
}
