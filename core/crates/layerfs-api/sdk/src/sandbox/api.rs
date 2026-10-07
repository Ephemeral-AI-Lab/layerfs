//! Concrete ordinary runtime facade; no filesystem launcher or host data service.
use super::{SandboxCreate, SandboxFailure};
use crate::{control::Control, OperationFailure};
use layerfs_bridge::{
    control::{DaemonStatus, HelloRequest, Reply, Request},
    native::ChannelWork,
};
use layerfs_sandbox::backend::docker::{Docker, Sandbox};
/// Actual local Engine and authenticated daemon lifecycle organization.
#[derive(Clone, Debug)]
pub struct SandboxApi {
    pub(super) docker: Docker,
}
/// Exact acknowledged container and authenticated control owner.
pub struct ManagedSandbox {
    /// Owned runtime container; explicit stop/delete only.
    pub runtime: Sandbox,
    /// Existing sequential control owner; correlation and failed state are never reset.
    pub control: Control,
    /// Original correlated startup status; this is not native Workspace Ready.
    pub startup: DaemonStatus,
    /// Actual original native handshake work.
    pub handshake: ChannelWork,
    pub(super) command_identity: layerfs_sandbox::CommandIdentity,
}
impl SandboxApi {
    /// Uses one explicit local Engine endpoint and metadata wait policy.
    pub fn new(docker: Docker) -> Self {
        Self { docker }
    }
    /// Creates, privately configures, starts and authenticates once, preserving partial ownership on failure.
    pub fn create(&self, request: SandboxCreate) -> Result<ManagedSandbox, Box<SandboxFailure>> {
        super::lifecycle::create(&self.docker, request)
    }
}
impl ManagedSandbox {
    /// Creates one ordinary runtime command with this deployment's admitted nonroot identity.
    /// Start/streams/status belong to the returned runtime owner; this registers no filesystem Exec.
    pub fn exec(
        &self,
        arguments: Vec<String>,
        environment: Vec<String>,
        directory: String,
        stdin: bool,
    ) -> Result<
        layerfs_sandbox::backend::docker::ExecCreated,
        Box<layerfs_sandbox::backend::docker::ExecCreateFailure>,
    > {
        self.runtime
            .create_exec(layerfs_sandbox::backend::docker::ExecRequest {
                identity: self.command_identity,
                arguments,
                environment,
                directory,
                stdin,
            })
    }

    /// Explicit startup observation/wait on the same incarnation and Control owner.
    /// A later observation never resolves an earlier unknown operation.
    pub fn status(&mut self, wait_for_store: bool) -> Result<DaemonStatus, Box<OperationFailure>> {
        hello(
            &mut self.control,
            HelloRequest {
                expected_instance: Some(self.startup.instance),
                wait_for_store,
            },
        )
    }
}
pub(super) fn hello(
    control: &mut Control,
    request: HelloRequest,
) -> Result<DaemonStatus, Box<OperationFailure>> {
    let request = Request::Hello(request);
    match crate::operation::exchange(control, request.clone())? {
        Reply::Hello(status) => Ok(status),
        reply => Err(OperationFailure::unexpected(request, reply)),
    }
}
