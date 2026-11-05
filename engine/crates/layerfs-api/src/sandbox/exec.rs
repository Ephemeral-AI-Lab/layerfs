//! Optional ordinary Sandbox launch at the acknowledged mounted directory.
use crate::{ManagedSandbox, MountedWorkspace, WorkspaceApi};
use layerfs_sandbox::{
    backend::docker::{ExecCreateFailure, ExecCreated, ExecRequest, Sandbox},
    CommandIdentity, DAEMON_ENTRY,
};

/// The daemon socket a command gets its private mount namespace from. Only a
/// daemon that serves from a private namespace names one, so `Some` always
/// wraps the command and `None` never does: there is no empty middle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MountNamespace<'a> {
    /// The daemon's socket. It sends the descriptor of its own mount
    /// namespace; the daemon is not dumpable, so the helper cannot open
    /// `/proc/<pid>/ns/mnt` itself.
    pub socket: &'a str,
}

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
        // The owner's own daemon setup says whether this Sandbox serves from a
        // private namespace. When it does, the command must enter it through
        // `ns-exec`; running it in the initial namespace would find only the
        // empty directory under the mount.
        let socket = sandbox
            .request
            .setup
            .mount_ns_socket()
            .map(|path| path.to_string_lossy().into_owned());
        launch(
            &sandbox.runtime,
            sandbox.command_identity,
            arguments,
            environment,
            workspace.ready.directory.clone(),
            stdin,
            socket.as_deref().map(|socket| MountNamespace { socket }),
        )
    }
}
/// Creates the Engine Exec for one command. `namespace` names a daemon that
/// serves from a private mount namespace: the command then runs through that
/// daemon's `ns-exec` helper, because the Workspace mounts are absent from the
/// namespace the Engine enters. `None` keeps the command as it is.
pub(crate) fn launch(
    sandbox: &Sandbox,
    identity: CommandIdentity,
    arguments: Vec<String>,
    environment: Vec<String>,
    directory: String,
    stdin: bool,
    namespace: Option<MountNamespace<'_>>,
) -> Result<ExecCreated, Box<ExecCreateFailure>> {
    sandbox.create_exec(ExecRequest {
        identity,
        arguments: exec_arguments(namespace, identity, &directory, arguments),
        environment,
        directory,
        stdin,
        root_launcher: namespace.is_some(),
    })
}
/// The Engine argv for one command. Without a mount-namespace socket it is
/// exactly the caller's argv. With one it names the daemon's helper, the
/// socket, the mount directory the Engine starts in (only the directory under
/// the mount exists in its namespace) and the ordinary identity the helper
/// drops to, then the same argv after `--`: the helper execs it, so the Engine
/// still has one process tree, the same streams and the same exit code.
pub fn exec_arguments(
    namespace: Option<MountNamespace<'_>>,
    identity: CommandIdentity,
    directory: &str,
    arguments: Vec<String>,
) -> Vec<String> {
    let Some(namespace) = namespace else {
        return arguments;
    };
    let mut wrapped = vec![
        DAEMON_ENTRY.to_owned(),
        "ns-exec".to_owned(),
        "--socket".to_owned(),
        namespace.socket.to_owned(),
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
