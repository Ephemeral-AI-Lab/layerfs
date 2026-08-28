impl MountedWorkspace {
    pub fn readdir(
        &mut self,
        handle: MountedHandleId,
        offset: u64,
        max_entries: usize,
    ) -> Result<Vec<MountedDirEntry>, MountedError> {
        let mut output = Vec::with_capacity(max_entries.min(DIRECTORY_PAGE_ENTRIES));
        let mut committed = offset;
        while output.len() < max_entries {
            let Some(entry) = self.readdir_next(handle, committed)? else {
                break;
            };
            committed = entry.next_offset;
            self.commit_readdir(handle, committed)?;
            output.push(entry);
        }
        Ok(output)
    }

    pub fn readdir_next(
        &mut self,
        handle: MountedHandleId,
        offset: u64,
    ) -> Result<Option<MountedDirEntry>, MountedError> {
        self.require_live_or_incomplete_read()?;
        let mut directory = match self.handles.remove(&handle) {
            Some(Handle::Directory(directory)) => directory,
            Some(other) => {
                self.handles.insert(handle, other);
                return Err(MountedError::InvalidHandle);
            }
            None => return Err(MountedError::InvalidHandle),
        };
        let result = (|| {
            if offset != directory.committed.cookie {
                directory.committed = DirectoryCursor::new(directory.committed.node);
                directory.pending = None;
                while directory.committed.cookie < offset {
                    if self
                        .next_directory_entry(&mut directory.committed)?
                        .is_none()
                    {
                        return Ok(None);
                    }
                }
            }
            if let Some((entry, _)) = &directory.pending {
                return Ok(Some(entry.clone()));
            }
            let mut after = directory.committed.clone();
            let entry = self.next_directory_entry(&mut after)?;
            if let Some(entry) = &entry {
                directory.pending = Some((entry.clone(), after));
            }
            Ok(entry)
        })();
        self.handles.insert(handle, Handle::Directory(directory));
        result
    }

    pub fn commit_readdir(
        &mut self,
        handle: MountedHandleId,
        next_offset: u64,
    ) -> Result<(), MountedError> {
        let Some(Handle::Directory(directory)) = self.handles.get_mut(&handle) else {
            return Err(MountedError::InvalidHandle);
        };
        let (entry, after) = directory.pending.take().ok_or(MountedError::InvalidRange)?;
        if entry.next_offset != next_offset {
            directory.pending = Some((entry, after));
            return Err(MountedError::InvalidRange);
        }
        directory.committed = after;
        Ok(())
    }

    pub fn discard_readdir_pending(&mut self, handle: MountedHandleId) -> Result<(), MountedError> {
        let Some(Handle::Directory(directory)) = self.handles.get_mut(&handle) else {
            return Err(MountedError::InvalidHandle);
        };
        directory.pending = None;
        Ok(())
    }

    pub fn reclaim_readdir_nodes(&mut self, nodes: &[MountedNodeId]) {
        for node in nodes {
            self.reclaim_node(*node);
        }
    }
}
