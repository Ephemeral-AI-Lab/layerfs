use crate::{
    OperationHandle, Query, QueryItem, QueryKind, QueryPage, Result, SdkError, WorkspaceQueryItem,
};
use layerfs_layerstack_store::{
    AddLayerResult, BranchId, CommitId, DiffRequest, EntityName, InitializeLayerStackResult,
    LayerId, LayerStackId, LayerStackInitialization, LayerStackStore, LocalForkSource,
    StorageReceipt,
};
use layerfs_monitor::{
    CandidateStats, Monitor, OperationFamily, OperationId, OperationOutcome, OperationReceipt,
    SemanticOperation,
};
use layerfs_workspace::{
    ConflictCursor, ConflictId, ConflictPage, ContainerBinding, CreateWorkspaceSession,
    EndWorkspaceMode, ExecutionId, NonEmpty, OutputReader, ResolveChoice, ResolveResult,
    WorkspaceCommitResult, WorkspaceCommitStatus, WorkspaceEndResult, WorkspaceExecution,
    WorkspaceFileRangeEdit, WorkspaceId, WorkspaceSession, Workspaces,
};
use std::ffi::OsString;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone)]
pub struct Client(Arc<ClientInner>);

struct ClientInner {
    store: Arc<LayerStackStore>,
    workspaces: Arc<Workspaces>,
    monitor: Arc<Monitor>,
}

impl Client {
    pub fn connect(store: Arc<LayerStackStore>) -> Result<Self> {
        Self::connect_inner(store, None)
    }

    pub fn connect_with_container(
        store: Arc<LayerStackStore>,
        binding: ContainerBinding,
    ) -> Result<Self> {
        Self::connect_inner(store, Some(binding))
    }

    fn connect_inner(
        store: Arc<LayerStackStore>,
        binding: Option<ContainerBinding>,
    ) -> Result<Self> {
        static NEXT_RUNTIME: AtomicU64 = AtomicU64::new(0);
        let runtime_root = std::env::temp_dir().join("layerfs-runtime").join(format!(
            "{}-{}",
            std::process::id(),
            NEXT_RUNTIME.fetch_add(1, Ordering::Relaxed)
        ));
        let workspaces = Arc::new(match binding {
            Some(binding) => {
                Workspaces::new_with_container(runtime_root, store.as_ref().clone(), binding)?
            }
            None => Workspaces::new(runtime_root, store.as_ref().clone())?,
        });
        let monitor = Arc::new(Monitor::new(store.clone(), workspaces.clone()));
        Ok(Self(Arc::new(ClientInner {
            store,
            workspaces,
            monitor,
        })))
    }

    pub fn initialize_layerstack(
        &self,
        name: EntityName,
        source: LayerStackInitialization,
    ) -> Result<InitializeLayerStackResult> {
        let operation = SemanticOperation {
            name: Some(name.clone()),
            ..SemanticOperation::new(OperationFamily::LayerStackInitialize)
        };
        self.observe(
            operation,
            || self.0.store.initialize_layerstack(name, source),
            |_| OperationOutcome::Success,
        )
    }

    pub fn fork_branch(&self, name: EntityName, source: LocalForkSource) -> Result<BranchId> {
        let operation = SemanticOperation {
            name: Some(name.clone()),
            ..SemanticOperation::new(OperationFamily::BranchFork)
        };
        self.observe(
            operation,
            || self.0.store.fork_branch(name, source),
            |_| OperationOutcome::Success,
        )
    }

    pub fn diff(&self, request: DiffRequest) -> Result<OperationHandle> {
        let id = OperationId::new();
        layerfs_layerstack_store::take_storage_receipts();
        let started = Instant::now();
        let handle = OperationHandle::build(id, |emit| {
            self.0
                .store
                .visit_diff(request, |entry| {
                    emit(entry)
                        .map_err(|_| layerfs_layerstack_store::StoreError::Integrity("Diff spool"))
                })
                .map_err(Into::into)
        });
        let family = match request {
            DiffRequest::Layers { .. } => OperationFamily::LayerStackDiff,
            DiffRequest::BranchCommits { .. } | DiffRequest::BranchLayer { .. } => {
                OperationFamily::BranchDiff
            }
        };
        self.record(
            id,
            SemanticOperation::new(family),
            if handle.is_ok() {
                OperationOutcome::Success
            } else {
                OperationOutcome::Failed
            },
            started,
        )?;
        handle
    }

    pub fn add_layer(&self, branch_id: BranchId) -> Result<AddLayerResult> {
        let operation = SemanticOperation {
            branch_id: Some(branch_id),
            ..SemanticOperation::new(OperationFamily::LayerStackAdd)
        };
        self.observe(
            operation,
            || self.0.store.add_layer(branch_id),
            |result| match result {
                AddLayerResult::Added { .. } => OperationOutcome::Success,
                AddLayerResult::UpToDate { .. } => OperationOutcome::UpToDate,
                AddLayerResult::NoChanges { .. } => OperationOutcome::NoChanges,
                AddLayerResult::HeadMoved { .. } => OperationOutcome::HeadMoved,
            },
        )
    }

    pub fn create_workspace_session(
        &self,
        request: CreateWorkspaceSession,
    ) -> Result<WorkspaceSession> {
        let operation = SemanticOperation {
            branch_id: Some(request.branch_id),
            ..SemanticOperation::new(OperationFamily::WorkspaceCreate)
        };
        self.observe(
            operation,
            || self.0.workspaces.create_workspace_session(request),
            |_| OperationOutcome::Success,
        )
    }

    pub fn workspace_conflicts(
        &self,
        workspace_id: WorkspaceId,
        cursor: Option<ConflictCursor>,
    ) -> Result<ConflictPage> {
        let operation = workspace_operation(OperationFamily::WorkspaceConflicts, workspace_id);
        self.observe(
            operation,
            || self.0.workspaces.workspace_conflicts(workspace_id, cursor),
            |_| OperationOutcome::Success,
        )
    }

    pub fn resolve_workspace_conflict(
        &self,
        workspace_id: WorkspaceId,
        conflict_id: ConflictId,
        choice: ResolveChoice,
    ) -> Result<ResolveResult> {
        let operation = workspace_operation(OperationFamily::WorkspaceResolve, workspace_id);
        self.observe(
            operation,
            || {
                self.0
                    .workspaces
                    .resolve_workspace_conflict(workspace_id, conflict_id, choice)
            },
            |_| OperationOutcome::Success,
        )
    }

    pub fn commit_workspace_session(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<WorkspaceCommitResult> {
        self.commit_workspace_session_with_status(workspace_id)
            .map(|status| status.result)
    }

    pub fn commit_workspace_session_with_status(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<WorkspaceCommitStatus> {
        let operation = workspace_operation(OperationFamily::WorkspaceCommit, workspace_id);
        self.observe(
            operation,
            || {
                self.0
                    .workspaces
                    .commit_workspace_session_with_status(workspace_id)
            },
            |status| match status.result {
                WorkspaceCommitResult::Created { .. } => OperationOutcome::Success,
                WorkspaceCommitResult::UpToDate { .. } => OperationOutcome::UpToDate,
                WorkspaceCommitResult::Busy => OperationOutcome::Busy,
                WorkspaceCommitResult::HeadMoved { .. } => OperationOutcome::HeadMoved,
            },
        )
    }

    pub fn recover_workspace_presentation(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<WorkspaceSession> {
        self.0
            .workspaces
            .recover_workspace_presentation(workspace_id)
            .map_err(Into::into)
    }

    pub fn edit_workspace_file_range(&self, edit: WorkspaceFileRangeEdit) -> Result<()> {
        let operation =
            workspace_operation(OperationFamily::WorkspaceFileRangeEdit, edit.workspace_id);
        self.observe(
            operation,
            || self.0.workspaces.edit_workspace_file_range(edit),
            |_| OperationOutcome::Success,
        )
    }

    /// Applies one public, prevalidated same-file edit batch with one projection refresh.
    ///
    /// The batch is failure-atomic: a rejected member leaves the file and presentation at
    /// their pre-call state. This is the supported v0.1.2 high-throughput owner-side API;
    /// callers that need one edit may use [`Self::edit_workspace_file_range`].
    pub fn edit_workspace_file_ranges(&self, edits: Vec<WorkspaceFileRangeEdit>) -> Result<()> {
        if edits.is_empty() {
            return Err(SdkError::InvalidRequest("Workspace file range edits"));
        }
        self.0
            .workspaces
            .edit_workspace_file_ranges(edits)
            .map_err(Into::into)
    }

    pub fn start_workspace_resource_sample(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<crate::ResourceSampleClock> {
        self.0
            .workspaces
            .start_workspace_resource_sample(workspace_id)
            .map_err(Into::into)
    }

    pub fn finish_workspace_resource_sample(
        &self,
        workspace_id: WorkspaceId,
        t0_unix_ns: u64,
        t3_unix_ns: u64,
        uncertainty_ns: u64,
    ) -> Result<layerfs_workspace::CgroupResourceSample> {
        self.0
            .workspaces
            .finish_workspace_resource_sample(workspace_id, t0_unix_ns, t3_unix_ns, uncertainty_ns)
            .map_err(Into::into)
    }

    pub fn end_workspace_session(
        &self,
        workspace_id: WorkspaceId,
        mode: EndWorkspaceMode,
    ) -> Result<WorkspaceEndResult> {
        let operation = workspace_operation(OperationFamily::WorkspaceEnd, workspace_id);
        self.observe(
            operation,
            || self.0.workspaces.end_workspace_session(workspace_id, mode),
            |_| OperationOutcome::Success,
        )
    }

    pub fn exec_workspace_session(
        &self,
        workspace_id: WorkspaceId,
        argv: NonEmpty<Vec<OsString>>,
    ) -> Result<WorkspaceExecution> {
        let operation = workspace_operation(OperationFamily::WorkspaceExec, workspace_id);
        self.observe(
            operation,
            || self.0.workspaces.exec(workspace_id, argv),
            |_| OperationOutcome::Success,
        )
    }

    pub fn shell_workspace_session(&self, workspace_id: WorkspaceId) -> Result<WorkspaceExecution> {
        let operation = workspace_operation(OperationFamily::WorkspaceShell, workspace_id);
        self.observe(
            operation,
            || self.0.workspaces.shell(workspace_id),
            |_| OperationOutcome::Success,
        )
    }

    pub fn workspace_output(&self, execution_id: ExecutionId) -> Result<OutputReader> {
        let operation = SemanticOperation {
            execution_id: Some(execution_id),
            ..SemanticOperation::new(OperationFamily::WorkspaceOutput)
        };
        self.observe(
            operation,
            || self.0.workspaces.output(execution_id),
            |_| OperationOutcome::Success,
        )
    }

    pub fn stop_workspace_execution(&self, execution_id: ExecutionId) -> Result<()> {
        let operation = SemanticOperation {
            execution_id: Some(execution_id),
            ..SemanticOperation::new(OperationFamily::WorkspaceStop)
        };
        self.observe(
            operation,
            || self.0.workspaces.stop(execution_id),
            |_| OperationOutcome::Success,
        )
    }

    pub fn query(&self, query: Query) -> Result<QueryPage> {
        let operation = SemanticOperation::new(OperationFamily::Query);
        self.observe(
            operation,
            || self.query_inner(&query),
            |_| OperationOutcome::Success,
        )
    }

    pub fn monitor_snapshot(&self) -> Result<layerfs_monitor::MonitorSnapshot> {
        Ok(self.0.monitor.snapshot()?)
    }

    pub fn active_workspace_count(&self) -> Result<usize> {
        Ok(self.0.workspaces.active_workspace_count()?)
    }

    pub fn active_execution_count(&self) -> Result<usize> {
        Ok(self.0.workspaces.active_execution_count()?)
    }

    pub fn analyze_dedup(&self) -> Result<layerfs_monitor::DedupAnalysis> {
        let operation = SemanticOperation::new(OperationFamily::DedupAnalyze);
        self.observe(
            operation,
            || self.0.monitor.analyze_dedup(),
            |_| OperationOutcome::Success,
        )
    }

    fn query_inner(&self, query: &Query) -> Result<QueryPage> {
        let limit = query.page_limit();
        match query.kind() {
            QueryKind::LayerStacks => {
                let after = query
                    .continuation()
                    .map(decode_id::<17, LayerStackId>)
                    .transpose()?;
                let page = self.0.store.layer_stack_record_page(after, limit)?;
                Ok(QueryPage {
                    items: page
                        .records
                        .into_iter()
                        .map(QueryItem::LayerStack)
                        .collect(),
                    continuation: page.continuation.map(|id| id.to_bytes().to_vec()),
                })
            }
            QueryKind::Layers => {
                let after = query
                    .continuation()
                    .map(decode_id::<33, LayerId>)
                    .transpose()?;
                let page = self
                    .0
                    .store
                    .layer_record_page(query.layer_stack_id(), after, limit)?;
                Ok(QueryPage {
                    items: page.records.into_iter().map(QueryItem::Layer).collect(),
                    continuation: page.continuation.map(|id| id.to_bytes().to_vec()),
                })
            }
            QueryKind::Branches => {
                let after = query
                    .continuation()
                    .map(decode_id::<17, BranchId>)
                    .transpose()?;
                let page = self
                    .0
                    .store
                    .branch_record_page(query.layer_stack_id(), after, limit)?;
                Ok(QueryPage {
                    items: page.records.into_iter().map(QueryItem::Branch).collect(),
                    continuation: page.continuation.map(|id| id.to_bytes().to_vec()),
                })
            }
            QueryKind::Commits => {
                let after = query
                    .continuation()
                    .map(decode_id::<33, CommitId>)
                    .transpose()?;
                let page = self.0.store.commit_record_page(after, limit)?;
                Ok(QueryPage {
                    items: page.records.into_iter().map(QueryItem::Commit).collect(),
                    continuation: page.continuation.map(|id| id.to_bytes().to_vec()),
                })
            }
            QueryKind::Workspaces => {
                let after = query
                    .continuation()
                    .map(|bytes| {
                        std::str::from_utf8(bytes)
                            .map_err(|_| SdkError::InvalidRequest("Workspace cursor"))?
                            .parse()
                            .map_err(|_| SdkError::InvalidRequest("Workspace cursor"))
                    })
                    .transpose()?;
                let (records, continuation) = self.0.workspaces.session_page(after, limit)?;
                Ok(QueryPage {
                    items: records
                        .into_iter()
                        .map(|summary| QueryItem::Workspace(WorkspaceQueryItem { summary }))
                        .collect(),
                    continuation: continuation.map(|id| id.to_string().into_bytes()),
                })
            }
            QueryKind::Monitor => {
                if query.continuation().is_some() {
                    return Err(SdkError::InvalidRequest("Monitor cursor"));
                }
                Ok(QueryPage {
                    items: vec![QueryItem::Monitor(self.0.monitor.snapshot()?)],
                    continuation: None,
                })
            }
        }
    }

    fn observe<T, E>(
        &self,
        operation: SemanticOperation,
        action: impl FnOnce() -> std::result::Result<T, E>,
        classify: impl FnOnce(&T) -> OperationOutcome,
    ) -> Result<T>
    where
        E: Into<SdkError>,
    {
        let id = OperationId::new();
        layerfs_layerstack_store::take_storage_receipts();
        let started = Instant::now();
        let result = action().map_err(Into::into);
        let outcome = result
            .as_ref()
            .map(classify)
            .unwrap_or(OperationOutcome::Failed);
        self.record(id, operation, outcome, started)?;
        result
    }

    fn record(
        &self,
        id: OperationId,
        operation: SemanticOperation,
        outcome: OperationOutcome,
        started: Instant,
    ) -> Result<()> {
        let storage = layerfs_layerstack_store::take_storage_receipts();
        let candidate = storage.iter().find_map(|receipt| match receipt {
            StorageReceipt::Candidate(receipt) => Some(CandidateStats {
                candidate_objects: receipt.candidate_objects,
                candidate_bytes: receipt.candidate_bytes,
                inserted_objects: receipt.inserted_objects,
                inserted_bytes: receipt.inserted_bytes,
                reused_objects: receipt.reused_objects,
                reused_bytes: receipt.reused_bytes,
                batch_inserted_objects: receipt.batch_inserted_objects,
                batch_inserted_bytes: receipt.batch_inserted_bytes,
                final_inserted_objects: receipt.final_inserted_objects,
                final_inserted_bytes: receipt.final_inserted_bytes,
                preexisting_reused_objects: receipt.preexisting_reused_objects,
                preexisting_reused_bytes: receipt.preexisting_reused_bytes,
                admission_transactions: receipt.admission_transactions,
                max_transaction_objects: receipt.max_transaction_objects,
                max_transaction_bytes: receipt.max_transaction_bytes,
            }),
            _ => None,
        });
        self.0.monitor.record(OperationReceipt {
            id,
            operation,
            outcome,
            queue_ns: 0,
            service_ns: elapsed_ns(started),
            candidate,
            storage,
        })?;
        Ok(())
    }
}

fn workspace_operation(family: OperationFamily, workspace_id: WorkspaceId) -> SemanticOperation {
    SemanticOperation {
        workspace_id: Some(workspace_id),
        ..SemanticOperation::new(family)
    }
}

fn decode_id<const N: usize, T>(bytes: &[u8]) -> Result<T>
where
    T: IdFromBytes<N>,
{
    let bytes: [u8; N] = bytes
        .try_into()
        .map_err(|_| SdkError::InvalidRequest("query cursor"))?;
    T::from_bytes(bytes).map_err(Into::into)
}

trait IdFromBytes<const N: usize>: Sized {
    fn from_bytes(bytes: [u8; N]) -> layerfs_layerstack_store::Result<Self>;
}

impl IdFromBytes<17> for LayerStackId {
    fn from_bytes(bytes: [u8; 17]) -> layerfs_layerstack_store::Result<Self> {
        Self::from_bytes(bytes)
    }
}

impl IdFromBytes<17> for BranchId {
    fn from_bytes(bytes: [u8; 17]) -> layerfs_layerstack_store::Result<Self> {
        Self::from_bytes(bytes)
    }
}

impl IdFromBytes<33> for LayerId {
    fn from_bytes(bytes: [u8; 33]) -> layerfs_layerstack_store::Result<Self> {
        Self::from_bytes(bytes)
    }
}

impl IdFromBytes<33> for CommitId {
    fn from_bytes(bytes: [u8; 33]) -> layerfs_layerstack_store::Result<Self> {
        Self::from_bytes(bytes)
    }
}

fn elapsed_ns(started: Instant) -> u64 {
    started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
}
