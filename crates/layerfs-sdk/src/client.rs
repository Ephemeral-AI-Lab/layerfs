use crate::{
    AddLayerResult, ConnectionContext, OperationHandle, Query, QueryItem, QueryKind, QueryPage,
    Result, SdkError, WorkspaceQueryItem,
};
use layerfs_monitor::{
    Monitor, OperationFamily, OperationId, OperationOutcome, OperationReceipt, SemanticOperation,
    TimingFragment,
};
use layerfs_storage::{
    AuthorityAddResult, BranchId, DiffRequest, EntityName, FactKind, InitializeLayerStackResult,
    LayerId, LayerStackInitialization, LocalForkSource, PullBranchResult, PullLayerResult,
    PushResult, RemotePlacement, StorageError, StorageId,
};
use layerfs_workspace::{
    CreateWorkspaceSession, EndWorkspaceMode, ExecutionId, NonEmpty, OutputReader,
    WorkspaceCommitResult, WorkspaceExecution, WorkspaceId,
};
use std::ffi::OsString;
use std::sync::Arc;

#[derive(Clone)]
pub struct Client {
    inner: Arc<ClientInner>,
}

struct ClientInner {
    context: ConnectionContext,
    workspaces: Arc<layerfs_workspace::Workspaces>,
    monitor: Arc<Monitor>,
}

impl Client {
    pub fn connect(context: ConnectionContext) -> Result<Self> {
        if context.layerstack.store_id() != context.branches.parent_store_id() {
            return Err(SdkError::InvalidContext);
        }
        let runtime_root = crate::topology::runtime_root(context.branches.path());
        let workspaces = Arc::new(layerfs_workspace::Workspaces::new(
            runtime_root.join("workspaces"),
            context.branches.clone(),
            context.layerstack.contract(),
        )?);
        let monitor = Arc::new(Monitor::new(
            runtime_root.join("monitor"),
            Arc::new(context.layerstack.store().clone()),
            context.branches.clone(),
            workspaces.clone(),
        )?);
        Ok(Self {
            inner: Arc::new(ClientInner {
                context,
                workspaces,
                monitor,
            }),
        })
    }

    pub fn initialize_layerstack(
        &self,
        name: EntityName,
        source: LayerStackInitialization,
    ) -> Result<InitializeLayerStackResult> {
        let receipt_name = name.clone();
        self.observe(
            OperationId::new(),
            || {
                Ok(self
                    .inner
                    .context
                    .layerstack
                    .store()
                    .initialize_layerstack(name, source)?)
            },
            |result| {
                let mut operation = SemanticOperation::new(OperationFamily::LayerStackInitialize);
                operation.layer_stack_name = Some(receipt_name);
                if let Ok(result) = result {
                    operation.layer_stack_id = Some(result.layer_stack_id);
                    operation.result_layer_id = Some(result.genesis_layer_id);
                }
                operation
            },
        )
    }

    pub fn pull_layer(
        &self,
        through_layer_id: LayerId,
        placement: RemotePlacement,
    ) -> Result<PullLayerResult> {
        self.observe(
            OperationId::new(),
            || {
                Ok(self.inner.context.branches.pull_layer(
                    self.inner.context.layerstack.contract(),
                    through_layer_id,
                    placement,
                )?)
            },
            |_| {
                let mut operation = SemanticOperation::new(OperationFamily::LayerStackPull);
                operation.through_layer_id = Some(through_layer_id);
                operation.placement = Some(placement);
                self.describe_layer(&mut operation, through_layer_id);
                operation
            },
        )
    }

    pub fn pull_branch(
        &self,
        branch_id: BranchId,
        through_commit_id: layerfs_storage::CommitId,
        placement: RemotePlacement,
    ) -> Result<PullBranchResult> {
        self.observe(
            OperationId::new(),
            || {
                Ok(self.inner.context.branches.pull_branch(
                    self.inner.context.layerstack.contract(),
                    branch_id,
                    through_commit_id,
                    placement,
                )?)
            },
            |_| {
                let mut operation = SemanticOperation::new(OperationFamily::BranchPull);
                operation.through_commit_id = Some(through_commit_id);
                operation.placement = Some(placement);
                self.describe_branch(&mut operation, branch_id);
                operation
            },
        )
    }

    pub fn fork_branch(&self, name: EntityName, source: LocalForkSource) -> Result<BranchId> {
        let receipt_name = name.clone();
        self.observe(
            OperationId::new(),
            || Ok(self.inner.context.branches.fork_branch(name, source)?),
            |result| {
                let mut operation = SemanticOperation::new(OperationFamily::BranchFork);
                operation.branch_name = Some(receipt_name);
                if let Ok(branch_id) = result {
                    self.describe_branch(&mut operation, *branch_id);
                }
                operation
            },
        )
    }

    pub fn diff(&self, request: DiffRequest) -> Result<OperationHandle> {
        let id = OperationId::new();
        self.observe(
            id,
            || {
                OperationHandle::build(id, |emit| {
                    match request {
                        DiffRequest::BranchCommits {
                            branch_id,
                            from_commit_id,
                            to_commit_id,
                        } => self.inner.context.branches.visit_branch_commit_diff(
                            self.inner.context.layerstack.contract(),
                            branch_id,
                            from_commit_id,
                            to_commit_id,
                            emit,
                        )?,
                        DiffRequest::BranchLayer {
                            branch_id,
                            layer_id,
                        } => self.inner.context.branches.visit_branch_layer_diff(
                            self.inner.context.layerstack.contract(),
                            branch_id,
                            layer_id,
                            emit,
                        )?,
                        DiffRequest::Layers {
                            from_layer_id,
                            to_layer_id,
                        } => self.inner.context.branches.visit_layer_diff(
                            self.inner.context.layerstack.contract(),
                            from_layer_id,
                            to_layer_id,
                            emit,
                        )?,
                    }
                    Ok(())
                })
            },
            |_| {
                let mut operation = SemanticOperation::new(match request {
                    DiffRequest::Layers { .. } => OperationFamily::LayerStackDiff,
                    DiffRequest::BranchCommits { .. } | DiffRequest::BranchLayer { .. } => {
                        OperationFamily::BranchDiff
                    }
                });
                match request {
                    DiffRequest::BranchCommits { branch_id, .. }
                    | DiffRequest::BranchLayer { branch_id, .. } => {
                        self.describe_branch(&mut operation, branch_id);
                    }
                    DiffRequest::Layers { to_layer_id, .. } => {
                        self.describe_layer(&mut operation, to_layer_id);
                    }
                }
                operation
            },
        )
    }

    pub fn push_branch(&self, branch_id: BranchId) -> Result<PushResult> {
        self.observe(
            OperationId::new(),
            || {
                Ok(self
                    .inner
                    .context
                    .branches
                    .push_branch(self.inner.context.layerstack.contract(), branch_id)?)
            },
            |result| {
                let mut operation = SemanticOperation::new(OperationFamily::BranchPush);
                self.describe_branch(&mut operation, branch_id);
                operation.result_commit_id = result.as_ref().ok().and_then(push_commit);
                operation
            },
        )
    }

    pub fn add_layer(&self, branch_id: BranchId) -> Result<AddLayerResult> {
        self.observe(
            OperationId::new(),
            || self.add_layer_inner(branch_id),
            |result| {
                let mut operation = SemanticOperation::new(OperationFamily::LayerStackAdd);
                self.describe_branch(&mut operation, branch_id);
                operation.result_layer_id = result.as_ref().ok().and_then(add_layer_id);
                operation
            },
        )
    }

    fn add_layer_inner(&self, branch_id: BranchId) -> Result<AddLayerResult> {
        let branch = self
            .inner
            .context
            .branches
            .require_local_branch(branch_id)?;
        let head = branch
            .head_commit_id
            .ok_or(StorageError::InvalidInput("Branch without Commit"))?;
        let authority = self.inner.context.layerstack.store().branch(branch_id)?;
        let Some(authority) = authority else {
            return Ok(AddLayerResult::NotPushed {
                branch_id,
                head_commit_id: head,
            });
        };
        if authority.head_commit_id != Some(head) {
            if let Some(known) = authority.head_commit_id {
                if self
                    .inner
                    .context
                    .branches
                    .branch_contains_commit(branch_id, known)?
                {
                    return Ok(AddLayerResult::NotPushed {
                        branch_id,
                        head_commit_id: head,
                    });
                }
            }
            return Err(StorageError::CommitHeadMoved {
                expected: Some(head),
                actual: authority.head_commit_id,
            }
            .into());
        }
        let outcome = self.inner.context.layerstack.store().add_layer(branch_id)?;
        match outcome {
            AuthorityAddResult::Added { layer_id } => Ok(AddLayerResult::Added { layer_id }),
            AuthorityAddResult::UpToDate { layer_id } => Ok(AddLayerResult::UpToDate { layer_id }),
            AuthorityAddResult::NoChanges { head_layer_id } => {
                Ok(AddLayerResult::NoChanges { head_layer_id })
            }
            AuthorityAddResult::HeadMoved { expected, actual } => {
                if self.inner.context.branches.layer(actual)?.is_none() {
                    return Ok(AddLayerResult::LayerNotPulled { layer_id: actual });
                }
                let (workspace_id, conflict_count) = self
                    .inner
                    .workspaces
                    .create_reconciliation_workspace(branch_id, actual)?;
                Ok(AddLayerResult::NeedsResolution {
                    workspace_id,
                    old_base_layer_id: expected,
                    current_layer_id: actual,
                    conflict_count,
                })
            }
        }
    }

    pub fn create_workspace_session(&self, request: CreateWorkspaceSession) -> Result<WorkspaceId> {
        let branch_id = request.branch_id;
        self.observe(
            OperationId::new(),
            || Ok(self.inner.workspaces.create_workspace_session(request)?.id),
            |result| {
                let mut operation = SemanticOperation::new(OperationFamily::WorkspaceCreate);
                self.describe_branch(&mut operation, branch_id);
                operation.workspace_id = result.as_ref().ok().copied();
                operation
            },
        )
    }

    pub fn commit_workspace_session(
        &self,
        workspace_id: WorkspaceId,
    ) -> Result<WorkspaceCommitResult> {
        self.observe(
            OperationId::new(),
            || {
                Ok(self
                    .inner
                    .workspaces
                    .commit_workspace_session(workspace_id)?)
            },
            |result| {
                let mut operation =
                    self.describe_workspace(OperationFamily::WorkspaceCommit, workspace_id);
                operation.result_commit_id = result.as_ref().ok().and_then(workspace_commit_id);
                operation
            },
        )
    }

    pub fn workspace_conflicts(
        &self,
        workspace_id: WorkspaceId,
        cursor: Option<layerfs_workspace::ConflictCursor>,
    ) -> Result<layerfs_workspace::ConflictPage> {
        self.observe(
            OperationId::new(),
            || {
                Ok(self
                    .inner
                    .workspaces
                    .workspace_conflicts(workspace_id, cursor)?)
            },
            |_| self.describe_workspace(OperationFamily::WorkspaceConflicts, workspace_id),
        )
    }

    pub fn resolve_workspace_conflict(
        &self,
        workspace_id: WorkspaceId,
        conflict_id: layerfs_workspace::ConflictId,
        choice: layerfs_workspace::ResolveChoice,
    ) -> Result<layerfs_workspace::ResolveResult> {
        self.observe(
            OperationId::new(),
            || {
                Ok(self.inner.workspaces.resolve_workspace_conflict(
                    workspace_id,
                    conflict_id,
                    choice,
                )?)
            },
            |_| self.describe_workspace(OperationFamily::WorkspaceResolve, workspace_id),
        )
    }

    pub fn end_workspace_session(
        &self,
        workspace_id: WorkspaceId,
        mode: EndWorkspaceMode,
    ) -> Result<()> {
        self.observe(
            OperationId::new(),
            || {
                self.inner
                    .workspaces
                    .end_workspace_session(workspace_id, mode)?;
                Ok(())
            },
            |_| self.describe_workspace(OperationFamily::WorkspaceEnd, workspace_id),
        )
    }

    pub fn exec_workspace_session(
        &self,
        workspace_id: WorkspaceId,
        argv: NonEmpty<Vec<OsString>>,
    ) -> Result<WorkspaceExecution> {
        self.observe(
            OperationId::new(),
            || Ok(self.inner.workspaces.exec(workspace_id, argv)?),
            |result| {
                let mut operation =
                    self.describe_workspace(OperationFamily::WorkspaceExec, workspace_id);
                operation.execution_id = result.as_ref().ok().map(|execution| execution.id);
                operation
            },
        )
    }

    pub fn shell_workspace_session(&self, workspace_id: WorkspaceId) -> Result<WorkspaceExecution> {
        self.observe(
            OperationId::new(),
            || Ok(self.inner.workspaces.shell(workspace_id)?),
            |result| {
                let mut operation =
                    self.describe_workspace(OperationFamily::WorkspaceShell, workspace_id);
                operation.execution_id = result.as_ref().ok().map(|execution| execution.id);
                operation
            },
        )
    }

    pub fn workspace_output(&self, execution_id: ExecutionId) -> Result<OutputReader> {
        self.observe(
            OperationId::new(),
            || Ok(self.inner.workspaces.output(execution_id)?),
            |_| {
                let mut operation = SemanticOperation::new(OperationFamily::WorkspaceOutput);
                operation.execution_id = Some(execution_id);
                operation
            },
        )
    }

    pub fn stop_workspace_execution(&self, execution_id: ExecutionId) -> Result<()> {
        self.observe(
            OperationId::new(),
            || Ok(self.inner.workspaces.stop(execution_id)?),
            |_| {
                let mut operation = SemanticOperation::new(OperationFamily::WorkspaceStop);
                operation.execution_id = Some(execution_id);
                operation
            },
        )
    }

    pub fn query(&self, query: Query) -> Result<QueryPage> {
        validate_query(&query)?;
        let limit = query.page_limit();
        match query.kind() {
            QueryKind::LayerStacks => {
                let page = self.inner.context.branches.layer_stack_scope_page(
                    typed_after::<layerfs_storage::LayerStackId>(&query)?,
                    limit,
                )?;
                Ok(QueryPage {
                    items: page
                        .records
                        .into_iter()
                        .map(|(fact, scope)| QueryItem::LayerStackScope(fact, scope))
                        .collect(),
                    continuation: page.continuation.map(id_bytes),
                })
            }
            QueryKind::AuthorityLayerStacks => {
                let page = self
                    .inner
                    .context
                    .layerstack
                    .store()
                    .layer_stack_record_page(
                        typed_after::<layerfs_storage::LayerStackId>(&query)?,
                        limit,
                    )?;
                Ok(QueryPage {
                    items: page
                        .records
                        .into_iter()
                        .map(QueryItem::LayerStack)
                        .collect(),
                    continuation: page.continuation.map(id_bytes),
                })
            }
            QueryKind::Branches => {
                let page = self.inner.context.branches.branch_scope_page(
                    query.layer_stack_id(),
                    typed_after::<BranchId>(&query)?,
                    limit,
                )?;
                Ok(QueryPage {
                    items: page
                        .records
                        .into_iter()
                        .map(|(record, scope)| QueryItem::BranchScope(record, scope))
                        .collect(),
                    continuation: page.continuation.map(id_bytes),
                })
            }
            QueryKind::AuthorityBranches => {
                let page = self.inner.context.layerstack.store().branch_record_page(
                    query.layer_stack_id(),
                    typed_after::<BranchId>(&query)?,
                    limit,
                )?;
                Ok(QueryPage {
                    items: page.records.into_iter().map(QueryItem::Branch).collect(),
                    continuation: page.continuation.map(id_bytes),
                })
            }
            QueryKind::Layers => paged_facts(query.continuation(), limit, |after, limit| {
                self.inner
                    .context
                    .branches
                    .fact_page(FactKind::Layer, after, limit)
            }),
            QueryKind::AuthorityLayers => {
                paged_facts(query.continuation(), limit, |after, limit| {
                    self.inner
                        .context
                        .layerstack
                        .store()
                        .fact_page(FactKind::Layer, after, limit)
                })
            }
            QueryKind::Commits => paged_facts(query.continuation(), limit, |after, limit| {
                self.inner
                    .context
                    .branches
                    .fact_page(FactKind::Commit, after, limit)
            }),
            QueryKind::AuthorityCommits => {
                paged_facts(query.continuation(), limit, |after, limit| {
                    self.inner
                        .context
                        .layerstack
                        .store()
                        .fact_page(FactKind::Commit, after, limit)
                })
            }
            QueryKind::Workspaces => {
                let (records, continuation) = self
                    .inner
                    .workspaces
                    .session_page(workspace_after(&query)?, limit)?;
                Ok(QueryPage {
                    items: records
                        .into_iter()
                        .map(|summary| QueryItem::Workspace(self.workspace_query_item(summary)))
                        .collect(),
                    continuation: continuation.map(|id| id.to_string().into_bytes()),
                })
            }
            QueryKind::Monitor => Ok(QueryPage {
                items: vec![QueryItem::Monitor(self.inner.monitor.snapshot()?)],
                continuation: None,
            }),
        }
    }

    pub fn monitor_snapshot(&self) -> Result<layerfs_monitor::MonitorSnapshot> {
        Ok(self.inner.monitor.snapshot()?)
    }

    pub fn analyze_dedup(&self) -> Result<layerfs_monitor::DedupAnalysis> {
        Ok(self.inner.monitor.analyze_dedup()?)
    }

    fn observe<T>(
        &self,
        id: OperationId,
        operation: impl FnOnce() -> Result<T>,
        describe: impl FnOnce(&Result<T>) -> SemanticOperation,
    ) -> Result<T> {
        self.inner.monitor.begin_operation();
        let started = std::time::Instant::now();
        let result = operation();
        let service_ns = started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        let receipt = OperationReceipt {
            id,
            operation: describe(&result),
            outcome: match &result {
                Ok(_) => OperationOutcome::Succeeded,
                Err(_) => OperationOutcome::Failed,
            },
            queued_ns: 0,
            service_ns,
            fragments: vec![TimingFragment {
                process_id: std::process::id(),
                started_ns: 0,
                elapsed_ns: service_ns,
            }],
            storage: self.inner.monitor.finish_operation(),
        };
        self.inner.monitor.record(receipt)?;
        result
    }

    fn describe_layer(&self, operation: &mut SemanticOperation, layer_id: LayerId) {
        let layer = self
            .inner
            .context
            .branches
            .layer(layer_id)
            .ok()
            .flatten()
            .or_else(|| {
                self.inner
                    .context
                    .layerstack
                    .store()
                    .layer(layer_id)
                    .ok()
                    .flatten()
            });
        if let Some(layer) = layer {
            operation.layer_stack_id = Some(layer.layer_stack_id);
            operation.layer_stack_name = self
                .inner
                .context
                .layerstack
                .store()
                .layer_stack_fact(layer.layer_stack_id)
                .ok()
                .flatten()
                .map(|stack| stack.name);
        }
    }

    fn describe_branch(&self, operation: &mut SemanticOperation, branch_id: BranchId) {
        operation.branch_id = Some(branch_id);
        let branch = self
            .inner
            .context
            .branches
            .branch(branch_id)
            .ok()
            .flatten()
            .or_else(|| {
                self.inner
                    .context
                    .layerstack
                    .store()
                    .branch(branch_id)
                    .ok()
                    .flatten()
            });
        if let Some(branch) = branch {
            operation.branch_name = Some(branch.name);
            operation.layer_stack_id = Some(branch.layer_stack_id);
            operation.layer_stack_name = self
                .inner
                .context
                .layerstack
                .store()
                .layer_stack_fact(branch.layer_stack_id)
                .ok()
                .flatten()
                .map(|stack| stack.name);
        }
    }

    fn describe_workspace(
        &self,
        family: OperationFamily,
        workspace_id: WorkspaceId,
    ) -> SemanticOperation {
        let mut operation = SemanticOperation::new(family);
        operation.workspace_id = Some(workspace_id);
        if let Ok(detail) = self.inner.workspaces.session(workspace_id) {
            self.describe_branch(&mut operation, detail.session.branch_id);
        }
        operation
    }

    fn workspace_query_item(
        &self,
        summary: layerfs_workspace::WorkspaceSummary,
    ) -> WorkspaceQueryItem {
        WorkspaceQueryItem {
            layer_stack_id: summary.layer_stack_id,
            layer_stack_name: summary.layer_stack_name.clone(),
            branch_name: summary.branch_name.clone(),
            summary,
        }
    }
}

fn validate_query(query: &Query) -> Result<()> {
    if query.page_limit() == 0 || query.page_limit() > 512 {
        return Err(SdkError::InvalidRequest("query limit"));
    }
    let branch_query = matches!(
        query.kind(),
        QueryKind::Branches | QueryKind::AuthorityBranches
    );
    if query.layer_stack_id().is_some() && !branch_query {
        return Err(SdkError::InvalidRequest("query LayerStack filter"));
    }
    match query.kind() {
        QueryKind::LayerStacks | QueryKind::AuthorityLayerStacks => {
            typed_after::<layerfs_storage::LayerStackId>(query).map(|_| ())
        }
        QueryKind::Layers | QueryKind::AuthorityLayers => {
            typed_after::<layerfs_storage::LayerId>(query).map(|_| ())
        }
        QueryKind::Branches | QueryKind::AuthorityBranches => {
            typed_after::<layerfs_storage::BranchId>(query).map(|_| ())
        }
        QueryKind::Commits | QueryKind::AuthorityCommits => {
            typed_after::<layerfs_storage::CommitId>(query).map(|_| ())
        }
        QueryKind::Monitor if query.continuation().is_some() => {
            Err(SdkError::InvalidRequest("terminal query continuation"))
        }
        QueryKind::Workspaces => workspace_after(query).map(|_| ()),
        QueryKind::Monitor => Ok(()),
    }
}

fn typed_after<T: StorageId>(query: &Query) -> Result<Option<T>> {
    query
        .continuation()
        .map(T::from_slice)
        .transpose()
        .map_err(Into::into)
}

fn id_bytes<T: StorageId>(id: T) -> Vec<u8> {
    id.as_slice().to_vec()
}

fn paged_facts(
    after: Option<&[u8]>,
    limit: u16,
    fetch: impl FnOnce(
        Option<&[u8]>,
        u16,
    ) -> layerfs_storage::Result<(Vec<layerfs_storage::Fact>, Option<Vec<u8>>)>,
) -> Result<QueryPage> {
    let (facts, continuation) = fetch(after, limit)?;
    Ok(QueryPage {
        items: facts.into_iter().map(QueryItem::Fact).collect(),
        continuation,
    })
}

fn workspace_after(query: &Query) -> Result<Option<WorkspaceId>> {
    query
        .continuation()
        .map(|value| {
            std::str::from_utf8(value)
                .map_err(|_| SdkError::InvalidRequest("Workspace continuation"))?
                .parse()
                .map_err(|_| SdkError::InvalidRequest("Workspace continuation"))
        })
        .transpose()
}

fn push_commit(result: &PushResult) -> Option<layerfs_storage::CommitId> {
    match *result {
        PushResult::Created { commit_id }
        | PushResult::Advanced { commit_id, .. }
        | PushResult::UpToDate { commit_id } => Some(commit_id),
        PushResult::HeadMoved { local_head, .. } => Some(local_head),
        PushResult::NoChanges => None,
    }
}

fn add_layer_id(result: &AddLayerResult) -> Option<LayerId> {
    match *result {
        AddLayerResult::Added { layer_id } | AddLayerResult::UpToDate { layer_id } => {
            Some(layer_id)
        }
        AddLayerResult::NoChanges { head_layer_id } => Some(head_layer_id),
        AddLayerResult::LayerNotPulled { layer_id } => Some(layer_id),
        AddLayerResult::NeedsResolution {
            current_layer_id, ..
        } => Some(current_layer_id),
        AddLayerResult::HeadMoved { actual, .. } => Some(actual),
        AddLayerResult::NotPushed { .. } => None,
    }
}

fn workspace_commit_id(result: &WorkspaceCommitResult) -> Option<layerfs_storage::CommitId> {
    match *result {
        WorkspaceCommitResult::Created { commit_id, .. } => Some(commit_id),
        WorkspaceCommitResult::UpToDate { head } => head,
        WorkspaceCommitResult::HeadMoved { actual, .. } => actual,
        WorkspaceCommitResult::Busy => None,
    }
}
