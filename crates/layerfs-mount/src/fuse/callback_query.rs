macro_rules! query_callbacks {
    () => {
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
    };
}
