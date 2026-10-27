//! Optional ordinary Sandbox launch at the acknowledged mounted directory.
use crate::{ManagedSandbox, MountedWorkspace, WorkspaceApi};
use layerfs_sandbox::{
    backend::docker::{ExecCreateFailure, ExecCreated, ExecRequest, Sandbox},
    CommandIdentity,
};

impl WorkspaceApi<'_> {
    /// Creates one ordinary command at this mount using the deployment's admitted identity.
    /// The returned Sandbox runtime owner owns Start, streams, exit status and cancellation.
    /// This sends no daemon request, registers no Exec and performs no Commit or unmount.
    pub fn exec(
        sandbox: &ManagedSandbox,
        workspace: &MountedWorkspace,
        arguments: Vec<String>,
        environment: Vec<String>,
        stdin: bool,
    ) -> Result<ExecCreated, Box<ExecCreateFailure>> {
        launch(
            &sandbox.runtime,
            sandbox.command_identity,
            arguments,
            environment,
            workspace.ready.directory.clone(),
            stdin,
        )
    }
}
pub(crate) fn launch(
    sandbox: &Sandbox,
    identity: CommandIdentity,
    arguments: Vec<String>,
    environment: Vec<String>,
    directory: String,
    stdin: bool,
) -> Result<ExecCreated, Box<ExecCreateFailure>> {
    sandbox.create_exec(ExecRequest {
        identity,
        arguments,
        environment,
        directory,
        stdin,
    })
}
