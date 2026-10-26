//! Hidden original-channel access for recovery, diagnostics and external proofs.
pub use super::control::{Control, ControlCause, ControlFailure, ControlPhase};
use crate::{BoundWorkspace, OperationFailure, WorkspaceApi};
use layerfs_bridge::{
    control::{CleanupObservation, ForcedOutcome, ReadyMount, WorkspaceStatus, WorkspaceToken},
    duplex::Connection,
};
use layerfs_history::{BranchId, WorkspaceId};
pub use layerfs_project::store::install;
use std::ops::{Deref, DerefMut};

/// Diagnostic facade over the same operation owner, without channel policy changes.
pub struct InternalWorkspaceApi<'a>(WorkspaceApi<'a>);
/// Borrows an original channel without reconnecting or resetting correlation.
pub fn workspace(control: &mut Control) -> InternalWorkspaceApi<'_> {
    InternalWorkspaceApi(WorkspaceApi {
        control,
        observation_scope: None,
    })
}
/// Applies explicit observation scope only to this borrow's filesystem calls.
pub fn with_observations<'a>(
    control: &'a mut Control,
    scope: &'a [u8; 32],
) -> InternalWorkspaceApi<'a> {
    InternalWorkspaceApi(WorkspaceApi {
        control,
        observation_scope: Some(scope),
    })
}
impl<'a> Deref for InternalWorkspaceApi<'a> {
    type Target = WorkspaceApi<'a>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl DerefMut for InternalWorkspaceApi<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
impl InternalWorkspaceApi<'_> {
    /// Original logical bind for component proofs; kernel mount still uses both steps.
    pub fn bind(
        &mut self,
        workspace: WorkspaceId,
        branch: BranchId,
    ) -> Result<BoundWorkspace, Box<OperationFailure>> {
        self.0.bind(workspace, branch)
    }
    /// Original attachment for separately acknowledged ownership proofs.
    pub fn attach(&mut self, token: WorkspaceToken) -> Result<ReadyMount, Box<OperationFailure>> {
        self.0.attach(token)
    }
    /// Observes a lost mount acknowledgement without replay.
    pub fn locate(
        &mut self,
        workspace: WorkspaceId,
    ) -> Result<WorkspaceStatus, Box<OperationFailure>> {
        self.0.locate(workspace)
    }
    /// Observes automatic local physical reclamation once.
    pub fn cleanup(
        &mut self,
        token: WorkspaceToken,
    ) -> Result<CleanupObservation, Box<OperationFailure>> {
        self.0.cleanup(token)
    }
    /// Original explicit unknown-relinquishment policy for forced custody proofs.
    pub fn force_unmount(
        &mut self,
        token: WorkspaceToken,
        relinquish_unknown: bool,
    ) -> Result<ForcedOutcome, Box<OperationFailure>> {
        self.0.force_unmount(token, relinquish_unknown)
    }
}
/// Sends this channel's later ordinary requests as SDK-traced observations
/// under `scope` (`None` ends it); results are unchanged.
pub fn trace(control: &mut Control, scope: Option<[u8; 32]>) {
    control.trace(scope);
}
/// Takes the telemetry of the traced replies received since the last take.
pub fn take_telemetry(control: &mut Control) -> Vec<layerfs_bridge::control::SdkValue> {
    control.take_telemetry()
}
/// Original install on an existing control owner, without a correlation reset.
pub fn install_project(
    control: &mut Control,
    project: &crate::SealedProject,
) -> Result<crate::Installed, Box<crate::InstallFailure>> {
    control.install_project(project)
}
/// Transfers the original terminal connection only for its fence proof.
pub fn into_connection(control: Control) -> Connection {
    control.into_connection()
}

/// Original authenticated control owner, without a reconnect or reset.
#[cfg(unix)]
pub fn control(sandbox: &mut crate::ManagedSandbox) -> &mut Control {
    &mut sandbox.control
}
/// The runtime, control owner and command identity of one Sandbox, taken
/// apart for an owner that serves several Workspaces at once: commands run on
/// the runtime while the control owner answers one request at a time.
#[cfg(unix)]
pub fn split(
    sandbox: crate::ManagedSandbox,
) -> (
    layerfs_sandbox::backend::docker::Sandbox,
    Control,
    layerfs_sandbox::CommandIdentity,
) {
    (sandbox.runtime, sandbox.control, sandbox.command_identity)
}
/// Original one-attempt startup used by existing-Store deployments and lifecycle proofs.
#[cfg(unix)]
pub fn start(
    api: &crate::SandboxApi,
    request: crate::SandboxCreate,
) -> Result<crate::ManagedSandbox, Box<crate::SandboxFailure>> {
    crate::sandbox::create::create(&api.docker, request)
}
/// Existing bounded startup observation for control-channel qualification.
#[cfg(unix)]
pub fn sandbox_status(
    sandbox: &mut crate::ManagedSandbox,
    wait_for_store: bool,
) -> Result<layerfs_bridge::control::DaemonStatus, Box<OperationFailure>> {
    sandbox.status(wait_for_store)
}

/// Disjoint original runtime/control borrows for concurrent Workspace orchestration.
#[cfg(unix)]
pub fn parts(
    sandbox: &mut crate::ManagedSandbox,
) -> (&layerfs_sandbox::backend::docker::Sandbox, &mut Control) {
    (&sandbox.runtime, &mut sandbox.control)
}

/// The same ordinary launcher for external host commands and disjoint concurrent proof borrows.
#[cfg(unix)]
pub fn exec(
    sandbox: &layerfs_sandbox::backend::docker::Sandbox,
    identity: layerfs_sandbox::CommandIdentity,
    arguments: Vec<String>,
    environment: Vec<String>,
    directory: String,
    stdin: bool,
) -> Result<
    layerfs_sandbox::backend::docker::ExecCreated,
    Box<layerfs_sandbox::backend::docker::ExecCreateFailure>,
> {
    crate::sandbox::exec::launch(sandbox, identity, arguments, environment, directory, stdin)
}
