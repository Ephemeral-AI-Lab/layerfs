impl MountedWorkspace {
    pub fn getattr(&mut self, node: MountedNodeId) -> Result<MountedAttr, MountedError> {
        self.require_live_or_incomplete_read()?;
        self.counters.getattr += 1;
        self.nodes
            .get(&node)
            .filter(|node| !node.deleted || node.open_refs != 0)
            .map(|value| value.attr(node))
            .ok_or(MountedError::NotFound)
    }

    pub fn lookup_child(
        &mut self,
        parent: MountedNodeId,
        name: &[u8],
    ) -> Result<MountedAttr, MountedError> {
        self.require_live_or_incomplete_read()?;
        let name = CanonicalName::from_bytes(name).map_err(|_| MountedError::InvalidName)?;
        let node = self
            .find_child(parent, &name)?
            .ok_or(MountedError::NotFound)?;
        let entry = self.nodes.get_mut(&node).ok_or(MountedError::Corrupt)?;
        entry.lookup_refs = entry.lookup_refs.saturating_add(1);
        self.lookup_refs = self.lookup_refs.saturating_add(1);
        self.counters.lookups += 1;
        Ok(entry.attr(node))
    }

    pub fn forget(&mut self, node: MountedNodeId, count: u64) {
        if node == ROOT_NODE {
            return;
        }
        if let Some(entry) = self.nodes.get_mut(&node) {
            let forgotten = entry.lookup_refs.min(count);
            entry.lookup_refs -= forgotten;
            self.lookup_refs = self.lookup_refs.saturating_sub(forgotten);
        }
        self.reclaim_node(node);
    }

    pub fn open_file(
        &mut self,
        node: MountedNodeId,
        truncate: bool,
    ) -> Result<MountedHandleId, MountedError> {
        self.require_live()?;
        self.preflight_handle(false)?;
        if self.nodes.get(&node).ok_or(MountedError::NotFound)?.kind != MountedFileType::RegularFile
        {
            return Err(MountedError::IsDirectory);
        }
        if truncate {
            self.truncate(node, 0)?;
        }
        let handle = self.allocate_handle()?;
        self.handles.insert(handle, Handle::File(node));
        self.nodes
            .get_mut(&node)
            .ok_or(MountedError::Corrupt)?
            .open_refs += 1;
        self.counters.opens += 1;
        self.observe_resources()?;
        Ok(handle)
    }

    pub fn open_directory(&mut self, node: MountedNodeId) -> Result<MountedHandleId, MountedError> {
        self.require_live_or_incomplete_read()?;
        self.preflight_handle(true)?;
        if self.nodes.get(&node).ok_or(MountedError::NotFound)?.kind != MountedFileType::Directory {
            return Err(MountedError::NotDirectory);
        }
        let handle = self.allocate_handle()?;
        self.handles.insert(
            handle,
            Handle::Directory(Box::new(DirectoryHandle {
                committed: DirectoryCursor::new(node),
                pending: None,
            })),
        );
        self.directory_cursors += 1;
        self.nodes
            .get_mut(&node)
            .ok_or(MountedError::Corrupt)?
            .open_refs += 1;
        self.counters.opens += 1;
        self.observe_resources()?;
        Ok(handle)
    }

    pub fn read(
        &mut self,
        node: MountedNodeId,
        handle: MountedHandleId,
        offset: u64,
        length: usize,
    ) -> Result<Vec<u8>, MountedError> {
        self.require_live_or_incomplete_read()?;
        if length > MAX_REQUEST_BYTES {
            return Err(MountedError::ResourceExhausted);
        }
        self.require_file_handle(node, handle)?;
        let (base, base_visible_len, mut plan, logical_len) =
            match &self.nodes.get(&node).ok_or(MountedError::NotFound)?.content {
                NodeContent::File {
                    base,
                    base_visible_len,
                    plan,
                    logical_len,
                    ..
                } => (*base, *base_visible_len, plan.clone(), *logical_len),
                _ => return Err(MountedError::IsDirectory),
            };
        if plan.is_none() {
            if let Some(base) = base {
                let mut counters = RopeCounters::default();
                let loaded = Arc::new(read_plan(&self.working, base, &mut counters)?);
                let entry = self.nodes.get_mut(&node).ok_or(MountedError::Corrupt)?;
                let NodeContent::File { plan: cache, .. } = &mut entry.content else {
                    return Err(MountedError::IsDirectory);
                };
                *cache = Some(loaded.clone());
                plan = Some(loaded);
            }
        }
        if offset >= logical_len || length == 0 {
            return Ok(Vec::new());
        }
        let end = offset
            .checked_add(length as u64)
            .ok_or(MountedError::InvalidRange)?
            .min(logical_len);
        let mut output =
            vec![0_u8; usize::try_from(end - offset).map_err(|_| MountedError::InvalidRange)?];
        if let Some(base) = base {
            let base_len = plan.as_ref().map_or(0, |plan| plan.logical_len());
            let persisted_end = end.min(base_len).min(base_visible_len);
            if offset < persisted_end {
                let mut sink = Cursor::new(
                    &mut output[..usize::try_from(persisted_end - offset)
                        .map_err(|_| MountedError::InvalidRange)?],
                );
                let _rope = if let Some(plan) = plan {
                    read_range_with_plan(&self.working, &plan, offset..persisted_end, &mut sink)?
                } else {
                    let mut counters = RopeCounters::default();
                    let plan = read_plan(&self.working, base, &mut counters)?;
                    let read = read_range_with_plan(
                        &self.working,
                        &plan,
                        offset..persisted_end,
                        &mut sink,
                    )?;
                    merge_rope(&mut counters, read)?;
                    counters
                };
            }
        }
        let (nodes, spool) = (&self.nodes, &mut self.spool);
        let ranges = match &nodes.get(&node).ok_or(MountedError::NotFound)?.content {
            NodeContent::File { ranges, .. } => ranges,
            _ => return Err(MountedError::IsDirectory),
        };
        for (start, range) in ranges.range(..end) {
            if range.end <= offset {
                continue;
            }
            let overlap_start = (*start).max(offset);
            let overlap_end = range.end.min(end);
            let destination =
                usize::try_from(overlap_start - offset).map_err(|_| MountedError::InvalidRange)?;
            let count = usize::try_from(overlap_end - overlap_start)
                .map_err(|_| MountedError::InvalidRange)?;
            let source = range
                .spool_offset
                .checked_add(overlap_start - *start)
                .ok_or(MountedError::InvalidRange)?;
            spool.read_exact_at(source, &mut output[destination..destination + count])?;
        }
        self.counters.reads += 1;
        self.observe_request(length)?;
        Ok(output)
    }

    pub fn write(
        &mut self,
        node: MountedNodeId,
        handle: MountedHandleId,
        offset: u64,
        bytes: &[u8],
    ) -> Result<usize, MountedError> {
        self.require_live()?;
        if bytes.len() > MAX_REQUEST_BYTES {
            return Err(MountedError::ResourceExhausted);
        }
        self.require_file_handle(node, handle)?;
        if bytes.is_empty() {
            return Ok(0);
        }
        let end = offset
            .checked_add(bytes.len() as u64)
            .ok_or(MountedError::InvalidRange)?;
        let entry = self.nodes.get(&node).ok_or(MountedError::NotFound)?;
        let deleted = entry.deleted;
        let old_len = match &entry.content {
            NodeContent::File { logical_len, .. } => *logical_len,
            _ => return Err(MountedError::IsDirectory),
        };
        let new_len = old_len.max(end);
        let projected_logical = if deleted {
            if new_len > MAX_LOGICAL_FILE_BYTES {
                return Err(MountedError::NoSpace);
            }
            None
        } else {
            self.preflight_dirty(&[node], 0)?;
            Some(self.preflight_logical_file(old_len, new_len)?)
        };
        if self.live_ranges.saturating_add(2) > MAX_DIRTY_RANGES {
            return Err(MountedError::ResourceExhausted);
        }
        if self
            .spool
            .live
            .checked_add(bytes.len() as u64)
            .is_none_or(|value| value > MAX_LIVE_SPOOL_BYTES)
        {
            return Err(MountedError::NoSpace);
        }
        if let Err(error) = self.compact_spool_if_needed(bytes.len() as u64) {
            self.lifecycle = MountedLifecycle::Incomplete;
            return Err(error);
        }
        let spool_offset = self.spool.next_offset(bytes.len())?;
        let timestamp = now_timestamp()?;
        let actual = match self.spool.append(bytes) {
            Ok(actual) => actual,
            Err(error) => {
                self.lifecycle = MountedLifecycle::Incomplete;
                return Err(error);
            }
        };
        if actual != spool_offset {
            self.lifecycle = MountedLifecycle::Incomplete;
            return Err(MountedError::Indeterminate);
        }
        self.counters.spool_physical_high_water_bytes = self
            .counters
            .spool_physical_high_water_bytes
            .max(self.spool.physical());
        let (old_count, new_count, removed, preserved) = {
            let entry = self.nodes.get_mut(&node).ok_or(MountedError::NotFound)?;
            let NodeContent::File {
                logical_len,
                ranges,
                plan,
                ..
            } = &mut entry.content
            else {
                self.lifecycle = MountedLifecycle::Incomplete;
                return Err(MountedError::IsDirectory);
            };
            let old_count = ranges.len();
            let (removed, preserved) = match install_dirty_range(ranges, offset, end, spool_offset)
            {
                Ok(effect) => effect,
                Err(error) => {
                    self.lifecycle = MountedLifecycle::Incomplete;
                    return Err(error);
                }
            };
            *logical_len = new_len;
            *plan = None;
            if !entry.deleted {
                entry.dirty_content = true;
                entry.dirty_metadata = true;
            }
            entry.mtime_seconds = timestamp.0;
            entry.mtime_nanoseconds = timestamp.1;
            (old_count, ranges.len(), removed, preserved)
        };
        self.live_ranges = self.live_ranges - old_count + new_count;
        self.spool.live = match self
            .spool
            .live
            .checked_sub(removed)
            .and_then(|value| value.checked_add(preserved))
            .and_then(|value| value.checked_add(bytes.len() as u64))
        {
            Some(live) => live,
            None => {
                self.lifecycle = MountedLifecycle::Incomplete;
                return Err(MountedError::Indeterminate);
            }
        };
        if let Some(projected) = projected_logical {
            self.logical_workspace_bytes = projected;
        }
        self.sync_node_state(node);
        self.counters.writes += 1;
        if let Err(error) = self.compact_spool_if_needed(0) {
            self.lifecycle = MountedLifecycle::Incomplete;
            return Err(error);
        }
        self.observe_request(bytes.len())?;
        self.observe_resources()?;
        Ok(bytes.len())
    }

    pub fn flush(&mut self, handle: MountedHandleId) -> Result<(), MountedError> {
        self.require_handle(handle)?;
        self.counters.flushes += 1;
        Ok(())
    }

    pub fn release(&mut self, handle: MountedHandleId) -> Result<(), MountedError> {
        let handle = self
            .handles
            .remove(&handle)
            .ok_or(MountedError::InvalidHandle)?;
        let node = match handle {
            Handle::File(node) => node,
            Handle::Directory(directory) => {
                self.directory_cursors = self.directory_cursors.saturating_sub(1);
                directory.committed.node
            }
        };
        let entry = self.nodes.get_mut(&node).ok_or(MountedError::Corrupt)?;
        entry.open_refs = entry.open_refs.saturating_sub(1);
        self.counters.releases += 1;
        if let Err(error) = self.finalize_deleted_pending(node) {
            self.lifecycle = MountedLifecycle::Incomplete;
            return Err(error);
        }
        self.reclaim_node(node);
        self.observe_resources()?;
        Ok(())
    }
}
