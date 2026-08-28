impl MountedWorkspace {
    fn sync_node_state(&mut self, id: MountedNodeId) {
        let Some(node) = self.nodes.get(&id) else {
            self.dirty_nodes.remove(&id);
            self.pending_nodes.remove(&id);
            return;
        };
        if node.dirty() {
            self.dirty_nodes.insert(id);
        } else {
            self.dirty_nodes.remove(&id);
        }
        if node.pending() {
            self.pending_nodes.insert(id);
        } else {
            self.pending_nodes.remove(&id);
        }
    }

    fn preflight_dirty(
        &self,
        nodes: &[MountedNodeId],
        additional: usize,
    ) -> Result<(), MountedError> {
        let mut newly_dirty = additional;
        for (index, node) in nodes.iter().enumerate() {
            let entry = self.nodes.get(node).ok_or(MountedError::NotFound)?;
            if entry.record.is_some()
                && !entry.dirty_metadata
                && self.checkpoint_metadata_bytes(entry)? > MAX_CHECKPOINT_METADATA_BYTES
            {
                return Err(MountedError::ResourceExhausted);
            }
            if !self.dirty_nodes.contains(node) && !nodes[..index].contains(node) {
                newly_dirty = newly_dirty
                    .checked_add(1)
                    .ok_or(MountedError::ResourceExhausted)?;
            }
        }
        if self
            .dirty_nodes
            .len()
            .checked_add(newly_dirty)
            .is_none_or(|count| count > MAX_DIRTY_NODES)
        {
            return Err(MountedError::ResourceExhausted);
        }
        Ok(())
    }

    fn checkpoint_metadata_bytes(&self, node: &MountedNode) -> Result<usize, MountedError> {
        let Some(record) = node.record else {
            return Ok(0);
        };
        let mut bytes = 0_usize;
        visit_metadata_entries(&self.working, record.metadata_root, |entries| {
            for entry in entries {
                bytes = bytes.saturating_add(
                    std::mem::size_of::<MetadataEntryV1>()
                        + entry.key.domain.len()
                        + entry.key.key.len()
                        + 64,
                );
            }
            Ok(())
        })?;
        Ok(bytes)
    }

    fn preflight_handle(&self, directory: bool) -> Result<(), MountedError> {
        if self.handles.len() == MAX_HANDLES
            || directory && self.directory_cursors == MAX_DIRECTORY_CURSORS
            || self.next_handle.checked_add(1).is_none()
        {
            return Err(MountedError::TooManyOpenFiles);
        }
        Ok(())
    }

    fn preflight_logical_file(&self, old: u64, new: u64) -> Result<u64, MountedError> {
        if new > MAX_LOGICAL_FILE_BYTES {
            return Err(MountedError::NoSpace);
        }
        let projected = self
            .logical_workspace_bytes
            .checked_sub(old)
            .and_then(|value| value.checked_add(new))
            .ok_or(MountedError::Indeterminate)?;
        if projected > MAX_LOGICAL_WORKSPACE_BYTES {
            return Err(MountedError::NoSpace);
        }
        Ok(projected)
    }

    fn has_dirty_state(&self) -> bool {
        !self.dirty_nodes.is_empty()
    }

    fn observe_request(&mut self, bytes: usize) -> Result<(), MountedError> {
        self.counters.largest_request_bytes = self.counters.largest_request_bytes.max(bytes as u64);
        let (current, high) = self.budget.observation()?;
        self.counters.operation_q_current_bytes = current as u64;
        self.counters.operation_q_high_water_bytes = high as u64;
        Ok(())
    }

    fn observe_resources(&mut self) -> Result<(), MountedError> {
        self.counters.lookup_refs = self.lookup_refs;
        self.counters.lookup_refs_high_water = self
            .counters
            .lookup_refs_high_water
            .max(self.counters.lookup_refs);
        self.counters.live_nodes = self.nodes.len() as u64;
        self.counters.live_nodes_high_water = self
            .counters
            .live_nodes_high_water
            .max(self.counters.live_nodes);
        self.counters.open_handles = self.handles.len() as u64;
        self.counters.open_handles_high_water = self
            .counters
            .open_handles_high_water
            .max(self.counters.open_handles);
        self.counters.pending_nodes = self.pending_nodes.len() as u64;
        self.counters.pending_nodes_high_water = self
            .counters
            .pending_nodes_high_water
            .max(self.counters.pending_nodes);
        self.counters.dirty_nodes = self.dirty_nodes.len() as u64;
        self.counters.dirty_nodes_high_water = self
            .counters
            .dirty_nodes_high_water
            .max(self.counters.dirty_nodes);
        self.counters.dirty_ranges = self.live_ranges as u64;
        self.counters.dirty_ranges_high_water = self
            .counters
            .dirty_ranges_high_water
            .max(self.counters.dirty_ranges);
        self.counters.directory_cursors = self.directory_cursors as u64;
        self.counters.directory_changes = self.directory_changes as u64;
        self.counters.directory_changes_high_water = self
            .counters
            .directory_changes_high_water
            .max(self.counters.directory_changes);
        self.counters.inode_mappings = self.by_inode.len() as u64;
        self.counters.inode_mappings_high_water = self
            .counters
            .inode_mappings_high_water
            .max(self.counters.inode_mappings);
        self.counters.logical_workspace_bytes = self.logical_workspace_bytes;
        self.counters.logical_workspace_high_water_bytes = self
            .counters
            .logical_workspace_high_water_bytes
            .max(self.logical_workspace_bytes);
        self.counters.spool_appended_bytes = self.spool.total_appended;
        self.counters.spool_live_bytes = self.spool.live;
        self.counters.spool_live_high_water_bytes = self
            .counters
            .spool_live_high_water_bytes
            .max(self.spool.live);
        self.counters.spool_dead_bytes = self.spool.appended.saturating_sub(self.spool.live);
        self.counters.spool_physical_bytes = self.spool.physical();
        self.counters.spool_physical_high_water_bytes = self
            .counters
            .spool_physical_high_water_bytes
            .max(self.counters.spool_physical_bytes);
        self.observe_request(0)
    }

    fn require_live(&self) -> Result<(), MountedError> {
        match self.lifecycle {
            MountedLifecycle::Live => Ok(()),
            MountedLifecycle::Checkpointing | MountedLifecycle::Closing => Err(MountedError::Busy),
            MountedLifecycle::Conflict | MountedLifecycle::Incomplete => {
                Err(MountedError::Indeterminate)
            }
            MountedLifecycle::Closed => Err(MountedError::StaleHandle),
        }
    }

    fn require_live_or_incomplete_read(&self) -> Result<(), MountedError> {
        match self.lifecycle {
            MountedLifecycle::Live
            | MountedLifecycle::Conflict
            | MountedLifecycle::Incomplete
            | MountedLifecycle::Closing => Ok(()),
            MountedLifecycle::Checkpointing => Err(MountedError::Busy),
            MountedLifecycle::Closed => Err(MountedError::StaleHandle),
        }
    }
}
