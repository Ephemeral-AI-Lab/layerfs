impl MountedWorkspace {
    fn load_canonical_node(
        &mut self,
        inode: InodeId,
        mut id: MountedNodeId,
    ) -> Result<MountedNodeId, MountedError> {
        if let Some(existing) = self.by_inode.get(&inode) {
            id = *existing;
            if self.nodes.contains_key(&id) {
                return Ok(id);
            }
        }
        if self.nodes.len() == MAX_MOUNTED_NODES {
            return Err(MountedError::ResourceExhausted);
        }
        if !self.by_inode.contains_key(&inode) && self.by_inode.len() == MAX_MOUNTED_NODES {
            let mut evicted = false;
            while let Some(candidate) = self.reclaimable_inode_mappings.pop_first() {
                let reclaimable = self
                    .by_inode
                    .get(&candidate)
                    .is_some_and(|node| !self.nodes.contains_key(node));
                if reclaimable {
                    self.by_inode.remove(&candidate);
                    evicted = true;
                    break;
                }
            }
            if !evicted {
                return Err(MountedError::ResourceExhausted);
            }
        }
        let mut counters = InodeTableCounters::default();
        let record_id = inode_table_lookup(
            &self.working,
            InodeTableRoot(self.namespace.inode_table_root),
            inode,
            &mut counters,
        )?
        .ok_or(MountedError::Corrupt)?;
        let record = self
            .working
            .with_authenticated_canonical(record_id, decode_inode_record)?;
        record.validate(id == ROOT_NODE)?;
        let metadata = read_portable_metadata(&self.working, record)?;
        let content = match record.kind {
            InodeKind::RegularFile => {
                let mut rope = RopeCounters::default();
                let plan = Arc::new(read_plan(
                    &self.working,
                    FileStateRoot(record.content_root),
                    &mut rope,
                )?);
                NodeContent::File {
                    base: Some(FileStateRoot(record.content_root)),
                    base_visible_len: plan.logical_len(),
                    logical_len: plan.logical_len(),
                    ranges: BTreeMap::new(),
                    plan: Some(plan),
                }
            }
            InodeKind::Directory => NodeContent::Directory {
                base: Some(DirectoryStateRoot(record.content_root)),
                changes: BTreeMap::new(),
            },
            InodeKind::Symlink => NodeContent::Symlink {
                target: self
                    .working
                    .with_authenticated_canonical(record.content_root, decode_symlink)?
                    .target,
            },
        };
        self.nodes.insert(
            id,
            MountedNode {
                canonical: Some(inode),
                record: Some(record),
                kind: record.kind.into(),
                mode: metadata.permission_mode,
                mtime_seconds: metadata.mtime_seconds,
                mtime_nanoseconds: metadata.mtime_nanoseconds,
                namespace_refs: record.namespace_ref_count,
                parent: ROOT_NODE,
                lookup_refs: 0,
                open_refs: 0,
                deleted: false,
                dirty_content: false,
                dirty_metadata: false,
                dirty_links: false,
                directory_mtime_before: None,
                content,
            },
        );
        self.by_inode.insert(inode, id);
        self.observe_resources()?;
        Ok(id)
    }

    fn ensure_canonical_node(
        &mut self,
        inode: InodeId,
        parent: MountedNodeId,
    ) -> Result<MountedNodeId, MountedError> {
        if let Some(id) = self.by_inode.get(&inode).copied() {
            if !self.nodes.contains_key(&id) {
                let id = self.load_canonical_node(inode, id)?;
                if self
                    .nodes
                    .get(&id)
                    .is_some_and(|node| node.kind == MountedFileType::Directory)
                {
                    self.nodes.get_mut(&id).ok_or(MountedError::Corrupt)?.parent = parent;
                }
                return Ok(id);
            }
            if self
                .nodes
                .get(&id)
                .is_some_and(|node| node.kind == MountedFileType::Directory)
            {
                self.nodes.get_mut(&id).ok_or(MountedError::Corrupt)?.parent = parent;
            }
            return Ok(id);
        }
        let id = self.allocate_node()?;
        let id = self.load_canonical_node(inode, id)?;
        if self
            .nodes
            .get(&id)
            .is_some_and(|node| node.kind == MountedFileType::Directory)
        {
            self.nodes.get_mut(&id).ok_or(MountedError::Corrupt)?.parent = parent;
        }
        Ok(id)
    }

    fn allocate_node(&mut self) -> Result<MountedNodeId, MountedError> {
        let id = MountedNodeId(self.next_node);
        self.next_node = self
            .next_node
            .checked_add(1)
            .ok_or(MountedError::ResourceExhausted)?;
        Ok(id)
    }

    fn allocate_handle(&mut self) -> Result<MountedHandleId, MountedError> {
        let id = MountedHandleId(self.next_handle);
        self.next_handle = self
            .next_handle
            .checked_add(1)
            .ok_or(MountedError::TooManyOpenFiles)?;
        Ok(id)
    }

    fn require_file_handle(
        &self,
        node: MountedNodeId,
        handle: MountedHandleId,
    ) -> Result<(), MountedError> {
        match self.handles.get(&handle) {
            Some(Handle::File(actual)) if *actual == node => Ok(()),
            _ => Err(MountedError::InvalidHandle),
        }
    }

    fn require_handle(&self, handle: MountedHandleId) -> Result<(), MountedError> {
        self.handles
            .contains_key(&handle)
            .then_some(())
            .ok_or(MountedError::InvalidHandle)
    }

    fn reclaim_node(&mut self, node: MountedNodeId) {
        if node == ROOT_NODE {
            return;
        }
        let reclaim = self
            .nodes
            .get(&node)
            .is_some_and(|entry| entry.lookup_refs == 0 && entry.open_refs == 0 && !entry.dirty());
        if reclaim {
            self.dirty_nodes.remove(&node);
            self.pending_nodes.remove(&node);
            if let Some(removed) = self.nodes.remove(&node) {
                if let NodeContent::Directory { changes, .. } = &removed.content {
                    self.directory_changes = self.directory_changes.saturating_sub(changes.len());
                }
                if let Some(inode) = removed.canonical {
                    if self.by_inode.get(&inode) == Some(&node) {
                        self.reclaimable_inode_mappings.insert(inode);
                    }
                }
            }
        }
    }

    fn finalize_deleted_pending(&mut self, node: MountedNodeId) -> Result<(), MountedError> {
        let should_clear = self.nodes.get(&node).is_some_and(|entry| {
            entry.deleted && entry.canonical.is_none() && entry.open_refs == 0
        });
        if !should_clear {
            return Ok(());
        }
        self.drain_node_file_ranges(node)?;
        let entry = self.nodes.get_mut(&node).ok_or(MountedError::Corrupt)?;
        entry.dirty_content = false;
        entry.dirty_metadata = false;
        entry.dirty_links = false;
        entry.directory_mtime_before = None;
        self.sync_node_state(node);
        self.normalize_spool()
    }

    fn drain_node_file_ranges(&mut self, node: MountedNodeId) -> Result<(), MountedError> {
        let (count, bytes) = match &self.nodes.get(&node).ok_or(MountedError::Corrupt)?.content {
            NodeContent::File { ranges, .. } => (
                ranges.len(),
                ranges.iter().try_fold(0_u64, |total, (start, range)| {
                    total
                        .checked_add(range.end - *start)
                        .ok_or(MountedError::Indeterminate)
                })?,
            ),
            _ => return Ok(()),
        };
        let live_ranges = self
            .live_ranges
            .checked_sub(count)
            .ok_or(MountedError::Indeterminate)?;
        let spool_live = self
            .spool
            .live
            .checked_sub(bytes)
            .ok_or(MountedError::Indeterminate)?;
        let entry = self.nodes.get_mut(&node).ok_or(MountedError::Corrupt)?;
        let NodeContent::File { ranges, plan, .. } = &mut entry.content else {
            return Err(MountedError::Indeterminate);
        };
        ranges.clear();
        *plan = None;
        self.live_ranges = live_ranges;
        self.spool.live = spool_live;
        Ok(())
    }
}
