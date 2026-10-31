//! Optional ordinary Sandbox launch at the acknowledged mounted directory.
use super::{MountedWorkspace, ProviderSandbox, WorkspaceApi};
use layerfs_sandbox::{provider::ExecExit, ExecRequest, WireFailure};
use std::io::Write;

impl WorkspaceApi<'_> {
    /// Runs one ordinary command at this mount using the deployment's admitted identity.
    /// The runtime provider owns execution, streams, exit status and cancellation.
    /// This sends no daemon request, registers no Exec and performs no Commit or unmount.
    pub fn exec(
        sandbox: &ProviderSandbox,
        workspace: &MountedWorkspace,
        arguments: Vec<String>,
        environment: Vec<String>,
        stdout: &mut (dyn Write + Send),
        stderr: &mut (dyn Write + Send),
    ) -> Result<ExecExit, Box<WireFailure>> {
        sandbox.runtime.run(
            &ExecRequest {
                identity: sandbox.command_identity,
                arguments,
                environment,
                directory: workspace.ready.directory.clone(),
                stdin: false,
            },
            stdout,
            stderr,
        )
    }
}
