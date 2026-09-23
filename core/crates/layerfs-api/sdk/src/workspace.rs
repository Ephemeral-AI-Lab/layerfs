use layerfs_api_core::{ExecResult, Mount, Project, SandboxId, WorkspaceError, WorkspaceId};
use layerfs_bridge::{
    adapters::native::{client::Client, connection::connect_until},
    contract::{
        Code, Operation, Request, Response, WorkspaceAttachOutcome, WorkspaceCommitOutcome,
        WorkspaceCommitReportWire, WorkspaceLifecycleOutcome, WORKSPACE_STATUS_PROFILE,
    },
};
use layerfs_sandbox::{Binding, RouteError, SandboxOwner};
use std::{
    io,
    time::{Duration, Instant},
};

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
        let binding = self.owner.lookup(sandbox).map_err(route_error)?;
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
        let response = call(
            &binding,
            15_000,
            Operation::WorkspaceOpen {
                workspace: id.0.as_bytes().to_vec(),
                incarnation,
                instance: binding.instance,
                project: project.id,
                branch,
                commit: commit_id,
            },
        )
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
        let (binding, route) = self.owner.workspace(id).map_err(route_error)?;
        let response = call(
            &binding,
            30_000,
            Operation::WorkspaceExec {
                workspace: id.0.as_bytes().to_vec(),
                incarnation: route.incarnation,
                command: command.as_bytes().to_vec(),
            },
        )?;
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
        let (binding, route) = self.owner.workspace(id).map_err(route_error)?;
        let response = call(
            &binding,
            600_000,
            Operation::WorkspaceCommit {
                workspace: id.0.as_bytes().to_vec(),
                incarnation: route.incarnation,
            },
        )?;
        let Response::WorkspaceCommit(result) = response else {
            return Err(Code::Integrity.into());
        };
        match result.outcome {
            WorkspaceCommitOutcome::Completed(report) => Ok(report),
            WorkspaceCommitOutcome::Failed(failure) => Err(WorkspaceError::Commit(failure)),
        }
    }

    pub fn unmount(&self, id: &WorkspaceId) -> Result<(), WorkspaceError> {
        let (binding, route) = self.owner.workspace(id).map_err(route_error)?;
        let response = call(
            &binding,
            5_000,
            Operation::WorkspaceUnmount {
                workspace: id.0.as_bytes().to_vec(),
                incarnation: route.incarnation,
            },
        )?;
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

fn call(
    binding: &Binding,
    deadline_ms: u32,
    operation: Operation,
) -> Result<Response, WorkspaceError> {
    let deadline = Instant::now() + Duration::from_millis(u64::from(deadline_ms));
    let mut client = Client::new(connect_until(
        binding.endpoint,
        binding.selector,
        &binding.private,
        &binding.server,
        deadline,
    )?)?;
    let request = Request {
        id: 1,
        generation: 0,
        store: 0,
        profile: WORKSPACE_STATUS_PROFILE,
        deadline_ms,
        response_bytes: 0,
        operation,
    };
    Ok(client.call_until(&request, &mut &[][..], &mut io::sink(), deadline)?)
}
