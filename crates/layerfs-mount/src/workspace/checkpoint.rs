impl MountedWorkspace {
    pub fn fsync(&mut self) -> Result<ObjectId, MountedError> {
        self.counters.fsyncs += 1;
        self.checkpoint()
    }

    pub fn fsyncdir(&mut self) -> Result<ObjectId, MountedError> {
        self.counters.fsyncdirs += 1;
        self.checkpoint()
    }

    pub fn checkpoint(&mut self) -> Result<ObjectId, MountedError> {
        self.require_live()?;
        if !self.has_dirty_state() {
            self.counters.no_op_checkpoints += 1;
            return Ok(self.candidate_root);
        }
        self.lifecycle = MountedLifecycle::Checkpointing;
        let result = self.checkpoint_inner();
        match result {
            Ok(state) => {
                self.lifecycle = MountedLifecycle::Live;
                self.counters.checkpoints += 1;
                Ok(state)
            }
            Err(error) => Err(self.classify_publication_error(error)),
        }
    }

    pub fn shutdown(&mut self) -> Result<ObjectId, MountedError> {
        if self.lifecycle == MountedLifecycle::Closed {
            return Ok(self.candidate_root);
        }
        self.require_live()?;
        self.lifecycle = MountedLifecycle::Closing;
        let dirty = self.has_dirty_state();
        let result = if dirty {
            self.checkpoint_inner()
        } else {
            self.counters.no_op_checkpoints += 1;
            Ok(self.candidate_root)
        };
        let root = match result {
            Ok(root) => {
                if dirty {
                    self.counters.checkpoints += 1;
                }
                root
            }
            Err(MountedError::Conflict) => {
                self.lifecycle = MountedLifecycle::Conflict;
                return Err(MountedError::Conflict);
            }
            Err(error) => {
                self.lifecycle = MountedLifecycle::Incomplete;
                return Err(error);
            }
        };
        match self.spool.reset() {
            Ok(true) => self.counters.spool_resets += 1,
            Ok(false) => {}
            Err(error) => {
                self.lifecycle = MountedLifecycle::Incomplete;
                return Err(error);
            }
        }
        self.budget.shutdown();
        self.namespace = self
            .working
            .with_authenticated_canonical(self.candidate_root, decode_namespace_root)?;
        self.logical_workspace_bytes =
            accepted_logical_bytes(&self.working, self.namespace.inode_table_root)?;
        self.lifecycle = MountedLifecycle::Closed;
        self.observe_resources()?;
        Ok(root)
    }

    pub fn release_kernel_cache_ownership(&mut self) -> Result<(), MountedError> {
        if self.lifecycle != MountedLifecycle::Closed {
            return Err(MountedError::Busy);
        }
        let (q_current, _) = self.budget.observation()?;
        if !self.dirty_nodes.is_empty()
            || !self.pending_nodes.is_empty()
            || self.directory_changes != 0
            || self.spool.live != 0
            || self.spool.physical() != 0
            || q_current != 0
        {
            self.lifecycle = MountedLifecycle::Incomplete;
            return Err(MountedError::Indeterminate);
        }
        self.namespace = self
            .working
            .with_authenticated_canonical(self.candidate_root, decode_namespace_root)?;
        self.logical_workspace_bytes =
            accepted_logical_bytes(&self.working, self.namespace.inode_table_root)?;
        let root_inode = self.namespace.root_directory_inode;
        let root = self
            .nodes
            .get_mut(&ROOT_NODE)
            .ok_or(MountedError::Corrupt)?;
        root.lookup_refs = 1;
        root.open_refs = 0;
        self.handles.clear();
        self.directory_cursors = 0;
        self.live_ranges = 0;
        self.nodes.retain(|id, _| *id == ROOT_NODE);
        self.by_inode.clear();
        self.by_inode.insert(root_inode, ROOT_NODE);
        self.reclaimable_inode_mappings.clear();
        self.lookup_refs = 1;
        self.observe_resources()
    }

    pub fn commit_operation(&self) -> Result<CommitResult, MountedError> {
        if self.lifecycle != MountedLifecycle::Closed {
            return Err(MountedError::Busy);
        }
        Ok(self.working.operation_commit(
            self.admission,
            WorkingCandidate {
                operation_id: self.admission.operation_id,
                expected_branch_generation: self.admission.branch_head_before.generation,
                base_root: self.admission.base.root(),
                candidate_root: self.candidate_root,
                normalized_transition: Vec::new(),
            },
        )?)
    }

    pub fn acknowledge_operation(&self, record: OperationRecordRef) -> Result<bool, MountedError> {
        Ok(self.working.acknowledge_operation(record)?)
    }

    fn checkpoint_inner(&mut self) -> Result<ObjectId, MountedError> {
        let _snapshot_reservation = self.budget.try_reserve(MAX_OPERATION_Q_BYTES)?;
        let mut dirty_ids = self.dirty_nodes.iter().copied().collect::<Vec<_>>();
        dirty_ids.sort_unstable();
        if dirty_ids.len() > MAX_DIRTY_NODES {
            return Err(MountedError::ResourceExhausted);
        }
        let mut canonical_ids = HashMap::new();
        for id in &dirty_ids {
            let node = self.nodes.get(id).ok_or(MountedError::Corrupt)?;
            if let Some(inode) = node.canonical {
                canonical_ids.insert(*id, inode);
            }
            if let NodeContent::Directory { changes, .. } = &node.content {
                for child in changes.values().flatten() {
                    if let Some(inode) = self
                        .nodes
                        .get(child)
                        .ok_or(MountedError::Corrupt)?
                        .canonical
                    {
                        canonical_ids.insert(*child, inode);
                    }
                }
            }
        }
        let mut publication = self.working.begin_candidate_write()?;
        for id in &dirty_ids {
            let node = self.nodes.get(id).ok_or(MountedError::Corrupt)?;
            if node.canonical.is_none() && !node.deleted {
                canonical_ids.insert(*id, publication.allocate_inode_id()?);
            }
        }
        let mut persisted = HashMap::new();
        let mut mutations = Vec::with_capacity(dirty_ids.len());
        for id in &dirty_ids {
            let node = self.nodes.get(id).ok_or(MountedError::Corrupt)?;
            if node.deleted {
                continue;
            }
            let metadata_entries = (node.dirty_metadata && node.record.is_some())
                .then(|| metadata_tree_entries(&publication, node.record.unwrap().metadata_root))
                .transpose()?;
            let snapshot = CheckpointNode {
                canonical: node.canonical,
                record: node.record,
                kind: node.kind,
                mode: node.mode,
                mtime_seconds: node.mtime_seconds,
                mtime_nanoseconds: node.mtime_nanoseconds,
                namespace_refs: node.namespace_refs,
                dirty_content: node.dirty_content,
                dirty_metadata: node.dirty_metadata,
                content: node.content.clone(),
                metadata_entries,
            };
            let inode = *canonical_ids.get(id).ok_or(MountedError::Corrupt)?;
            let content_root = if snapshot.canonical.is_none() || snapshot.dirty_content {
                Self::persist_content(&mut self.spool, &mut publication, &snapshot, &canonical_ids)?
            } else {
                snapshot.record.ok_or(MountedError::Corrupt)?.content_root
            };
            let metadata_root = if snapshot.canonical.is_none() || snapshot.dirty_metadata {
                persist_metadata(&mut publication, &snapshot)?
            } else {
                snapshot.record.ok_or(MountedError::Corrupt)?.metadata_root
            };
            let record = InodeRecordV1 {
                kind: inode_kind(snapshot.kind),
                namespace_ref_count: snapshot.namespace_refs,
                content_root,
                metadata_root,
            };
            record.validate(*id == ROOT_NODE)?;
            mutations.push(layerfs_core::logical::InodeMutation::Upsert { inode, record });
            persisted.insert(*id, (inode, record));
        }
        for id in &dirty_ids {
            let node = self.nodes.get(id).ok_or(MountedError::Corrupt)?;
            if node.deleted {
                if let Some(inode) = node.canonical {
                    mutations.push(layerfs_core::logical::InodeMutation::Remove { inode });
                }
            }
        }
        let candidate =
            publication.trusted_apply_inode_mutations(self.candidate_root, mutations)?;
        let root = candidate.root();
        let namespace = layerfs_core::logical::namespace(&publication, root)?;
        publication.commit_trusted_operation_candidate(self.admission.operation_id, candidate)?;
        self.candidate_root = root;
        self.namespace = namespace;
        let cleanup = (|| -> Result<(), MountedError> {
            for id in &dirty_ids {
                if self.nodes.get(id).ok_or(MountedError::Corrupt)?.deleted {
                    continue;
                }
                let (inode, record) = *persisted.get(id).ok_or(MountedError::Corrupt)?;
                let entry = self.nodes.get_mut(id).ok_or(MountedError::Corrupt)?;
                entry.canonical = Some(inode);
                entry.record = Some(record);
                entry.dirty_content = false;
                entry.dirty_metadata = false;
                entry.dirty_links = false;
                entry.directory_mtime_before = None;
                let mut cleared_ranges = (0, 0_u64);
                match &mut entry.content {
                    NodeContent::File {
                        base,
                        base_visible_len,
                        logical_len,
                        ranges,
                        plan,
                    } => {
                        *base = Some(FileStateRoot(record.content_root));
                        *base_visible_len = *logical_len;
                        cleared_ranges = (
                            ranges.len(),
                            ranges.iter().map(|(start, range)| range.end - *start).sum(),
                        );
                        ranges.clear();
                        *plan = None;
                    }
                    NodeContent::Directory { base, changes } => {
                        *base = Some(DirectoryStateRoot(record.content_root));
                        self.directory_changes =
                            self.directory_changes.saturating_sub(changes.len());
                        changes.clear();
                    }
                    NodeContent::Symlink { .. } => {}
                }
                self.live_ranges = self.live_ranges.saturating_sub(cleared_ranges.0);
                self.spool.live = self.spool.live.saturating_sub(cleared_ranges.1);
                self.by_inode.insert(inode, *id);
                self.sync_node_state(*id);
            }
            for id in dirty_ids
                .iter()
                .copied()
                .filter(|id| self.nodes.get(id).is_some_and(|node| node.deleted))
                .collect::<Vec<_>>()
            {
                if self
                    .nodes
                    .get(&id)
                    .is_some_and(|entry| entry.open_refs == 0)
                {
                    self.drain_node_file_ranges(id)?;
                }
                if let Some(entry) = self.nodes.get_mut(&id) {
                    if let Some(inode) = entry.canonical.take() {
                        self.by_inode.remove(&inode);
                        self.reclaimable_inode_mappings.remove(&inode);
                    }
                    entry.dirty_content = false;
                    entry.dirty_metadata = false;
                    entry.dirty_links = false;
                    entry.directory_mtime_before = None;
                }
                self.sync_node_state(id);
                self.reclaim_node(id);
            }
            self.normalize_spool_during_checkpoint()?;
            self.observe_resources()?;
            Ok(())
        })();
        if cleanup.is_err() {
            self.lifecycle = MountedLifecycle::Incomplete;
            return Err(MountedError::CommittedCleanup);
        }
        Ok(root)
    }
}
