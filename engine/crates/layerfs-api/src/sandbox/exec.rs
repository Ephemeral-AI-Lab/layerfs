//! Optional ordinary Sandbox launch at the acknowledged mounted directory.
use crate::{ManagedSandbox, MountedWorkspace, WorkspaceApi};
use layerfs_sandbox::{
    backend::docker::{ExecCreateFailure, ExecCreated, ExecRequest, Sandbox},
    CommandIdentity,
};

/// The daemon's entry inside its Sandbox container, as the deployment installs it.
const DAEMON_ENTRY: &str = "/usr/local/bin/layerfs-daemon";

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
            None,
        )
    }
}
/// Creates the Engine Exec for one command. `ns_pid_file` is the pid file of a
/// daemon that serves from a private mount namespace: the command then runs
/// through that daemon's `ns-exec` helper, because the Workspace mounts are
/// absent from the namespace the Engine enters. `None` keeps the command as
/// it is.
pub(crate) fn launch(
    sandbox: &Sandbox,
    identity: CommandIdentity,
    arguments: Vec<String>,
    environment: Vec<String>,
    directory: String,
    stdin: bool,
    ns_pid_file: Option<&str>,
) -> Result<ExecCreated, Box<ExecCreateFailure>> {
    sandbox.create_exec(ExecRequest {
        identity,
        arguments: launch_arguments(ns_pid_file, identity, &directory, arguments),
        environment,
        directory,
        stdin,
        root_launcher: ns_pid_file.is_some(),
    })
}
/// The Engine argv for one command. Without a private-namespace pid file it is
/// exactly the caller's argv. With one it names the daemon's helper, the pid
/// file, the mount directory the Engine starts in (only the directory under the
/// mount exists in its namespace) and the ordinary identity the helper drops
/// to, then the same argv after `--`: the helper execs it, so the Engine still
/// has one process tree, the same streams and the same exit code.
fn launch_arguments(
    ns_pid_file: Option<&str>,
    identity: CommandIdentity,
    directory: &str,
    arguments: Vec<String>,
) -> Vec<String> {
    let Some(pid_file) = ns_pid_file else {
        return arguments;
    };
    let mut wrapped = vec![
        DAEMON_ENTRY.to_owned(),
        "ns-exec".to_owned(),
        "--pid-file".to_owned(),
        pid_file.to_owned(),
        "--directory".to_owned(),
        directory.to_owned(),
        "--uid".to_owned(),
        identity.uid.to_string(),
        "--gid".to_owned(),
        identity.gid.to_string(),
        "--".to_owned(),
    ];
    wrapped.extend(arguments);
    wrapped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sandbox_without_a_private_namespace_keeps_the_command_as_it_is() {
        let identity = CommandIdentity { uid: 501, gid: 20 };
        let arguments = vec!["bash".into(), "-c".into(), "printf ordinary".into()];
        assert_eq!(
            launch_arguments(None, identity, "/workspaces/1", arguments.clone()),
            arguments
        );
    }

    #[test]
    fn a_private_namespace_sandbox_runs_the_daemon_helper() {
        let identity = CommandIdentity { uid: 501, gid: 20 };
        let arguments = vec!["bash".into(), "-c".into(), "printf ordinary".into()];
        assert_eq!(
            launch_arguments(
                Some("/layerfs-local/daemon.pid"),
                identity,
                "/workspaces/1",
                arguments,
            ),
            [
                "/usr/local/bin/layerfs-daemon",
                "ns-exec",
                "--pid-file",
                "/layerfs-local/daemon.pid",
                "--directory",
                "/workspaces/1",
                "--uid",
                "501",
                "--gid",
                "20",
                "--",
                "bash",
                "-c",
                "printf ordinary",
            ]
        );
    }
}
