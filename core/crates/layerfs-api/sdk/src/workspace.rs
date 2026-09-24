use layerfs_api_core::{
    ExecResult, Mount, Project, SandboxId, WorkspaceError, WorkspaceId, WorkspaceStatus,
};
use layerfs_bridge::contract::{
    Code, Operation, Response, WorkspaceAttachOutcome, WorkspaceCommitOutcome,
    WorkspaceCommitReportWire, WorkspaceLifecycleOutcome, PROJECTION_CLASS_LABELS,
};
use layerfs_sandbox::{ControlRoute, RouteError, SandboxOwner};

pub struct WorkspaceApi<'a> {
    owner: &'a SandboxOwner,
}
impl<'a> WorkspaceApi<'a> {
    pub fn new(owner: &'a SandboxOwner) -> Self {
        Self { owner }
    }

    pub fn mount(
        &self,
        sandbox: SandboxId,
        project: &Project,
        branch: [u8; 17],
        commit_id: Option<[u8; 33]>,
    ) -> Result<Mount, WorkspaceError> {
        let (binding, _) = self.owner.lookup_route(sandbox).map_err(route_error)?;
        let random = layerfs_sandbox::random::<16>()?;
        let mut id = String::with_capacity(32);
        for byte in random {
            use std::fmt::Write;
            write!(&mut id, "{byte:02x}").expect("String write");
        }
        let id = WorkspaceId(id);
        let incarnation = layerfs_sandbox::random::<32>()?;
        self.owner
            .bind_workspace(&binding, id.clone(), incarnation)
            .map_err(route_error)?;
        let response = self
            .owner
            .control_call(
                ControlRoute {
                    sandbox,
                    instance: binding.instance,
                    workspace: None,
                },
                15_000,
                || Operation::WorkspaceOpen {
                    workspace: id.0.as_bytes().to_vec(),
                    incarnation,
                    instance: binding.instance,
                    project: project.id,
                    branch,
                    commit: commit_id,
                },
            )
            .map_err(route_error)
            .map_err(|error| match error {
                WorkspaceError::Failure(cause) if cause.unknown => WorkspaceError::UncertainMount {
                    id: id.clone(),
                    cause,
                },
                other => other,
            })?;
        let Response::WorkspaceAttach(result) = response else {
            return Err(Code::Integrity.into());
        };
        match result.outcome {
            WorkspaceAttachOutcome::Completed => Ok(Mount {
                location: format!("/layerfs/workspace/{}", id.0),
                id,
            }),
            WorkspaceAttachOutcome::Retained(cause) => Err(WorkspaceError::Retained { id, cause }),
        }
    }

    pub fn exec(&self, id: &WorkspaceId, command: &str) -> Result<ExecResult, WorkspaceError> {
        let route = self.owner.workspace(id).map_err(route_error)?;
        let (workspace, incarnation) = route.selector()?;
        let response = self
            .owner
            .control_call(route, 30_000, || Operation::WorkspaceExec {
                workspace,
                incarnation,
                command: command.as_bytes().to_vec(),
            })
            .map_err(route_error)?;
        let Response::WorkspaceExec(result) = response else {
            return Err(Code::Integrity.into());
        };
        Ok(ExecResult {
            exit_status: result.exit_status,
            stdout: result.stdout,
            stderr: result.stderr,
            stdout_truncated: result.stdout_truncated,
            stderr_truncated: result.stderr_truncated,
        })
    }

    pub fn commit(&self, id: &WorkspaceId) -> Result<WorkspaceCommitReportWire, WorkspaceError> {
        let route = self.owner.workspace(id).map_err(route_error)?;
        let (workspace, incarnation) = route.selector()?;
        let response = self
            .owner
            .control_call(route, 600_000, || Operation::WorkspaceCommit {
                workspace,
                incarnation,
            })
            .map_err(route_error)?;
        let Response::WorkspaceCommit(result) = response else {
            return Err(Code::Integrity.into());
        };
        match result.outcome {
            WorkspaceCommitOutcome::Completed(report) => Ok(report),
            WorkspaceCommitOutcome::Failed(failure) => Err(WorkspaceError::Commit(failure)),
        }
    }

    /// Read one current Workspace observation through the public status route.
    ///
    /// The counts are bounded callback and upstream-call counts the daemon
    /// reports for this incarnation. Callers must not insert this between Edit
    /// and Commit: it is an observation, not part of the acknowledgement.
    pub fn status(&self, id: &WorkspaceId) -> Result<WorkspaceStatus, WorkspaceError> {
        let route = self.owner.workspace(id).map_err(route_error)?;
        let (workspace, incarnation) = route.selector()?;
        let response = self
            .owner
            .control_call(route, 5_000, || Operation::WorkspaceStatus {
                workspace,
                incarnation,
            })
            .map_err(route_error)?;
        // A Workspace in local-edit mode answers the same status request with
        // its writable observation; the projection counts are the same fields,
        // and both are one current observation rather than a receipt.
        let status = match response {
            Response::WorkspaceStatus(status) => *status,
            Response::WorkspaceWritableStatus(writable) => writable.status,
            _ => return Err(Code::Integrity.into()),
        };
        if status.projection.len() != PROJECTION_CLASS_LABELS.len() {
            return Err(Code::Integrity.into());
        }
        Ok(WorkspaceStatus {
            mounted: status.mounted,
            stopping: status.stopping,
            closed: status.closed,
            active_operations: status.active_operations,
            nodes: status.nodes,
            handles: status.handles,
            cookies: status.cookies,
            consumer_accounted_bytes: status.consumer_accounted_bytes,
            projection: PROJECTION_CLASS_LABELS
                .iter()
                .zip(status.projection)
                .map(|(label, count)| ((*label).to_string(), count))
                .collect(),
            upstream_calls: status.upstream_calls,
            range_accepted_payload_bytes: status.range_accepted_payload_bytes,
            range_shifted_suffix_bytes: status.range_shifted_suffix_bytes,
        })
    }

    pub fn unmount(&self, id: &WorkspaceId) -> Result<(), WorkspaceError> {
        let route = self.owner.workspace(id).map_err(route_error)?;
        let (workspace, incarnation) = route.selector()?;
        let response = self
            .owner
            .control_call(route, 5_000, || Operation::WorkspaceUnmount {
                workspace,
                incarnation,
            })
            .map_err(route_error)?;
        let Response::WorkspaceUnmount(result) = response else {
            return Err(Code::Integrity.into());
        };
        match result.outcome {
            WorkspaceLifecycleOutcome::Completed => Ok(()),
            WorkspaceLifecycleOutcome::Retained(cause) => Err(WorkspaceError::Retained {
                id: id.clone(),
                cause,
            }),
        }
    }
}

fn route_error(error: RouteError) -> WorkspaceError {
    match error {
        RouteError::Failure(failure) => WorkspaceError::Failure(failure),
        RouteError::Stale => WorkspaceError::Stale,
        RouteError::NotFound => Code::NotFound.into(),
    }
}
