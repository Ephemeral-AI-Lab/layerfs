pub struct MountedWorkspace {
    working: WorkingStore,
    admission: BeginOperation,
    candidate_root: ObjectId,
    namespace: NamespaceRootV1,
    lifecycle: MountedLifecycle,
    nodes: HashMap<MountedNodeId, MountedNode>,
    by_inode: HashMap<InodeId, MountedNodeId>,
    reclaimable_inode_mappings: BTreeSet<InodeId>,
    handles: HashMap<MountedHandleId, Handle>,
    next_node: u64,
    next_handle: u64,
    live_ranges: usize,
    dirty_nodes: HashSet<MountedNodeId>,
    pending_nodes: HashSet<MountedNodeId>,
    directory_cursors: usize,
    directory_changes: usize,
    lookup_refs: u64,
    logical_workspace_bytes: u64,
    spool: Spool,
    budget: Arc<ByteBudget>,
    counters: MountedCounters,
    #[cfg(test)]
    splice_post_visibility_uncertainty: bool,
}

impl MountedWorkspace {
    pub fn open(
        working_root: &Path,
        admission: BeginOperation,
        integrity: IntegrityMode,
        spool: PathBuf,
    ) -> Result<Self, MountedError> {
        let working = WorkingStore::open(working_root, integrity)
            .map_err(|error| startup("WorkingStore open", error))?;
        if working.storage_id() != admission.working_storage_id {
            return Err(MountedError::Corrupt);
        }
        let candidate_root = admission.base.root();
        let namespace = working
            .with_authenticated_canonical(candidate_root, decode_namespace_root)
            .map_err(|error| startup("namespace root", error))?;
        let logical_workspace_bytes = accepted_logical_bytes(&working, namespace.inode_table_root)
            .map_err(|error| startup("logical workspace", error))?;
        if logical_workspace_bytes > MAX_LOGICAL_WORKSPACE_BYTES {
            return Err(MountedError::ResourceExhausted);
        }
        let store_id = working.storage_id();
        let spool = Spool::new(
            spool,
            store_id,
            *admission.operation_id.as_bytes(),
            admission.workspace_nonce,
        )
        .map_err(|error| startup("spool", error))?;
        let mut workspace = Self {
            working,
            admission,
            candidate_root,
            namespace,
            lifecycle: MountedLifecycle::Live,
            nodes: HashMap::new(),
            by_inode: HashMap::new(),
            reclaimable_inode_mappings: BTreeSet::new(),
            handles: HashMap::new(),
            next_node: ROOT_NODE.0 + 1,
            next_handle: 1,
            live_ranges: 0,
            dirty_nodes: HashSet::new(),
            pending_nodes: HashSet::new(),
            directory_cursors: 0,
            directory_changes: 0,
            lookup_refs: 0,
            logical_workspace_bytes,
            spool,
            budget: Arc::new(ByteBudget::new(MAX_OPERATION_Q_BYTES)),
            counters: MountedCounters::default(),
            #[cfg(test)]
            splice_post_visibility_uncertainty: false,
        };
        let root = workspace
            .load_canonical_node(namespace.root_directory_inode, ROOT_NODE)
            .map_err(|error| startup("root inode", error))?;
        if root != ROOT_NODE {
            return Err(MountedError::Corrupt);
        }
        let root = workspace
            .nodes
            .get_mut(&ROOT_NODE)
            .ok_or(MountedError::Corrupt)?;
        root.parent = ROOT_NODE;
        root.lookup_refs = 1;
        workspace.lookup_refs = 1;
        workspace.observe_resources()?;
        Ok(workspace)
    }

    pub fn candidate_root(&self) -> ObjectId {
        self.candidate_root
    }

    pub fn admission(&self) -> BeginOperation {
        self.admission
    }

    pub fn splice_path(
        &mut self,
        path: &CanonicalPath,
        start: u64,
        delete_len: u64,
        replacement: &[u8],
    ) -> Result<MountedSpliceReceipt, MountedError> {
        self.require_live()?;
        if self.has_dirty_state() {
            return Err(MountedError::Busy);
        }
        if replacement.len() > MAX_REQUEST_BYTES {
            return Err(MountedError::ResourceExhausted);
        }
        let before = self.candidate_root;
        let reservation = self.budget.try_reserve(MAX_OPERATION_Q_BYTES)?;
        let result = (|| {
            let mut writer = self.working.begin_candidate_write()?;
            let candidate = writer.trusted_replace_range(
                before,
                path,
                start,
                delete_len,
                Cursor::new(replacement),
            )?;
            let root = candidate.root();
            let counters = candidate.counters();
            writer.commit_trusted_operation_candidate(self.admission.operation_id, candidate)?;
            Ok((root, counters))
        })();
        drop(reservation);
        let (after, counters) = match result {
            Ok(result) => result,
            Err(error) => return Err(self.classify_publication_error(error)),
        };
        #[cfg(test)]
        if self.splice_post_visibility_uncertainty {
            self.candidate_root = after;
            self.lifecycle = MountedLifecycle::Incomplete;
            return Err(MountedError::Indeterminate);
        }
        let (current, high) = self.budget.observation()?;
        self.candidate_root = after;
        self.counters.splices += 1;
        self.lifecycle = MountedLifecycle::Closed;
        self.budget.shutdown();
        Ok(MountedSpliceReceipt {
            generation: self.admission.branch_head_before.generation,
            before,
            after,
            counters,
            operation_q_terminal_bytes: current as u64,
            operation_q_high_water_bytes: high as u64,
            remount_required: true,
        })
    }

    pub fn lifecycle(&self) -> MountedLifecycle {
        self.lifecycle
    }

    pub fn mark_incomplete(&mut self) {
        self.lifecycle = MountedLifecycle::Incomplete;
    }

    fn classify_publication_error(&mut self, error: MountedError) -> MountedError {
        self.lifecycle = match error {
            MountedError::Conflict => MountedLifecycle::Conflict,
            MountedError::Indeterminate | MountedError::CommittedCleanup => {
                MountedLifecycle::Incomplete
            }
            _ => MountedLifecycle::Live,
        };
        error
    }

    pub fn byte_budget(&self) -> Arc<ByteBudget> {
        self.budget.clone()
    }

    pub fn counters(&mut self) -> Result<MountedCounters, MountedError> {
        self.observe_resources()?;
        Ok(self.counters)
    }

    pub fn capacity(&self) -> Result<MountedCapacity, MountedError> {
        self.require_live_or_incomplete_read()?;
        let free_bytes = MAX_LOGICAL_WORKSPACE_BYTES
            .saturating_sub(self.logical_workspace_bytes)
            .min(MAX_LIVE_SPOOL_BYTES.saturating_sub(self.spool.live));
        let free_files = MAX_MOUNTED_NODES
            .saturating_sub(self.nodes.len())
            .min(MAX_DIRTY_NODES.saturating_sub(self.dirty_nodes.len()))
            .min(MAX_DIRECTORY_CHANGES.saturating_sub(self.directory_changes));
        Ok(MountedCapacity {
            total_bytes: MAX_LOGICAL_WORKSPACE_BYTES,
            free_bytes,
            total_files: MAX_MOUNTED_NODES as u64,
            free_files: free_files as u64,
        })
    }

    pub fn engine_counters(&self) -> Result<EngineCounters, MountedError> {
        Ok(self.working.counters()?)
    }

    pub fn active_store_connections(&self) -> Result<u64, MountedError> {
        Ok(self.working.active_connection_count()?)
    }

    pub fn close_store_connection(&self) -> Result<(), MountedError> {
        Ok(self.working.close_primary_connection()?)
    }

    pub fn reset_engine_counters(&self) -> Result<(), MountedError> {
        Ok(self.working.reset_counters()?)
    }
}
