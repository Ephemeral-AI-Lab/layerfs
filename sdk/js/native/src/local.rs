//! Local Sandboxes (`deploy.sandbox` `local`, LAYERFS_SANDBOX=local): the
//! project's daemon runs as a process of this kernel instead of in a
//! container, for a VM or CI box that is itself the isolation boundary. It
//! isolates nothing from the rest of that machine.
//!
//! Each Sandbox gets a directory `<LAYERFS_LOCAL_DIR>/<prefix><project>-<n>`
//! with what a container held at fixed paths: its setup
//! (`/layerfs-local/config`), overlay template (`/layerfs-local/overlay`) and
//! Workspace mounts (`/workspaces`). The daemon opens the installed Store
//! where this process opens it (`storePath`), so the daemon setup and the
//! Store's locator name that path instead of `/layerfs-store`. The daemon
//! listens on 127.0.0.1 only. Commands run as the same nonroot identity as
//! in a container. Start, failure and reclaim follow the Docker route: one
//! attempt, a creation intent while the outcome is open, and a failed start's
//! daemon stopped and its directory deleted, each once.
use crate::job::{failed, refused};
use crate::registry::{lock, text, SandboxState, SANDBOXES};
use crate::runtime::Runtime;
use layerfs_api::internal::Control;
use layerfs_bridge::control::{DaemonPhase, HelloRequest, Reply, Request, SdkRefusal, SdkValue};
use layerfs_bridge::duplex::initiate_observed;
use layerfs_bridge::provision::StoreManifest;
use layerfs_sandbox::backend::local::{Local, LocalCreateFailure, Sandbox};
use layerfs_sandbox::CommandIdentity;
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Starts the project's next Sandbox as a local daemon. Called by
/// [`crate::sandbox::start`] under the project's guard, with the project
/// and its installed manifest already read.
pub(crate) fn start(
    args: &SdkValue,
    directory: &Path,
    project: (&crate::registry::Project, StoreManifest),
) -> Result<Arc<SandboxState>, SdkRefusal> {
    let deploy = args
        .get("deploy")
        .ok_or_else(|| refused("InvalidInput", "missing `deploy`"))?;
    // Inputs are read first, so a refused request leaves no creation intent.
    if !crate::discovery::root() {
        return Err(refused(
            "Unsupported",
            "LAYERFS_SANDBOX=local needs root: its daemon mounts FUSE and its commands run as \
             uid 501, gid 20. Run it as root inside the VM or CI box that isolates it",
        ));
    }
    let root = deploy.get_text("localDir").ok_or_else(|| {
        refused(
            "InvalidInput",
            "deploy.localDir is not configured (LAYERFS_LOCAL_DIR); a local Sandbox needs it",
        )
    })?;
    let local = Local::new(root)
        .map_err(|error| refused("InvalidInput", format!("LAYERFS_LOCAL_DIR {root}: {error}")))?;
    let executable = PathBuf::from(text(deploy, "daemon")?);
    let store = match args.get_text("storePath") {
        Some(path) if Path::new(path).is_absolute() => path.to_owned(),
        _ => {
            return Err(refused(
                "InvalidInput",
                "a local Sandbox's daemon opens the client's storePath (LAYERFS_STORE_PATH), an absolute path",
            ))
        }
    };
    crate::sandbox::unresolved(
        directory,
        &format!("a directory under {root} named for it, and its daemon process"),
    )?;
    let (id, saved_directory) = crate::discovery::claim_next(args)?;
    let made = start_claimed(
        args,
        directory,
        &saved_directory,
        &id,
        project,
        (local, executable, store),
    );
    if made.is_err() {
        let _ = std::fs::remove_dir(&saved_directory);
    }
    made
}

/// Starts the claimed Sandbox `id`: directory and daemon on the installed
/// Store, its first control connection, then its records.
fn start_claimed(
    args: &SdkValue,
    directory: &Path,
    saved_directory: &Path,
    id: &str,
    (project, installed): (&crate::registry::Project, StoreManifest),
    (local, executable, store): (Local, PathBuf, String),
) -> Result<Arc<SandboxState>, SdkRefusal> {
    let name = text(args, "project")?;
    let prefix = args
        .get("deploy")
        .and_then(|d| d.get_text("containerPrefix"))
        .filter(|prefix| !prefix.is_empty())
        .unwrap_or(crate::sandbox::PREFIX);
    let sandbox_name = format!("{prefix}{name}-{id}");
    let local = local
        .naming(&sandbox_name)
        .map_err(|_| refused("InvalidInput", format!("local Sandbox name {sandbox_name}")))?;
    let paths = local.paths().map_err(failed)?;
    let path_text = |path: &Path| {
        path.to_str()
            .map(str::to_owned)
            .ok_or_else(|| refused("InvalidInput", "LAYERFS_LOCAL_DIR must be UTF-8"))
    };
    let controller = layerfs_bridge::duplex::generate_keypair().map_err(failed)?;
    let daemon = layerfs_bridge::duplex::generate_keypair().map_err(failed)?;
    let mut setup = crate::sandbox::setup(daemon.private, controller.public);
    setup.listen = "127.0.0.1:0".into();
    setup.store = store.clone();
    setup.overlay = path_text(&paths.overlay)?;
    setup.mounts = path_text(&paths.mounts)?;
    let mut existing = project.manifest.clone();
    existing.daemon_sqlite = installed.daemon_sqlite;
    // the Store's path in the daemon's filesystem, which is this one
    existing.locator = store;
    setup.existing_store = Some(existing);
    let identity = CommandIdentity {
        uid: setup.command_uid,
        gid: setup.command_gid,
    };
    let config = setup.encode().map_err(failed)?;
    // Read only if this process ends before the intent is resolved.
    crate::discovery::save(
        &directory.join("create.intent"),
        SdkValue::text(format!("creation of Sandbox {id} began and never finished")),
    )?;
    let labels = crate::sandbox::labels(name, id);
    let sandbox = local
        .create(&executable, &config, crate::sandbox::STARTUP, labels)
        .map_err(|failure| {
            let LocalCreateFailure {
                phase,
                sandbox,
                cause,
            } = *failure;
            abandon(
                directory,
                id,
                format!("local sandbox create stopped at {phase:?}: {cause:?}"),
                sandbox,
            )
        })?;
    let first = sandbox.endpoint().map_err(failed).and_then(|endpoint| {
        first(endpoint, controller.private, daemon.public).map(|c| (endpoint, c))
    });
    let (endpoint, (control, instance)) = match first {
        Ok(opened) => opened,
        Err(refusal) => {
            return Err(abandon(
                directory,
                id,
                format!("local sandbox first connection: {}", refusal.message),
                Some(sandbox),
            ))
        }
    };
    let dial = crate::pool::Dial {
        endpoint,
        host: None,
        secret: controller.private,
        peer: daemon.public,
        instance,
    };
    let state = Arc::new(SandboxState {
        id: id.to_owned(),
        project: name.to_owned(),
        name: sandbox_name,
        directory: saved_directory.to_owned(),
        runtime: Runtime::Local(sandbox),
        identity,
        controls: crate::pool::Controls::new(control, dial),
        mounts: Default::default(),
        processes: Default::default(),
    });
    crate::discovery::save_sandbox(
        &state.directory,
        &state,
        args,
        controller.private,
        daemon.public,
        instance,
    )?;
    crate::discovery::save_text(
        &state.directory.join("container"),
        &state.runtime.identity().to_string(),
    )?;
    std::fs::remove_file(directory.join("create.intent")).map_err(failed)?;
    lock(&SANDBOXES)?.insert(state.directory.clone(), state.clone());
    Ok(state)
}

/// The first control connection to a new daemon: one connect, one
/// authentication, one Hello that waits for the Store and must find the
/// daemon ControlReady; its instance identifies the daemon from then on.
fn first(
    endpoint: SocketAddr,
    secret: [u8; 32],
    peer: [u8; 32],
) -> Result<(Control, [u8; 32]), SdkRefusal> {
    let wait = crate::sandbox::STARTUP;
    let stream = TcpStream::connect_timeout(&endpoint, wait).map_err(failed)?;
    stream.set_read_timeout(Some(wait)).map_err(failed)?;
    stream.set_write_timeout(Some(wait)).map_err(failed)?;
    let connection = initiate_observed(stream, &secret, peer)
        .result
        .map_err(failed)?;
    connection.clear_io_waits().map_err(failed)?;
    let mut control = Control::new(connection);
    match control
        .call(Request::Hello(HelloRequest {
            expected_instance: None,
            wait_for_store: true,
        }))
        .map_err(failed)?
    {
        Reply::Hello(status) if status.phase == DaemonPhase::ControlReady => {
            Ok((control, status.instance))
        }
        reply => Err(refused(
            "Failed",
            format!("the new daemon is not ControlReady: {reply:?}"),
        )),
    }
}

/// The refusal for a failed start. Everything a local start makes is its
/// own (a directory and a child process), so each effect is known: the
/// daemon's last log lines are read once, then it is stopped and its
/// directory deleted, each once, and the intent removed. If that cleanup
/// fails, the intent stays and says what is left.
fn abandon(directory: &Path, id: &str, mut cause: String, sandbox: Option<Sandbox>) -> SdkRefusal {
    let container = sandbox.as_ref().map(|s| s.identity().to_string());
    let mut log = None;
    let mut left = None;
    if let Some(mut sandbox) = sandbox {
        match sandbox.log_tail(crate::sandbox::LOG_LINES, crate::sandbox::LOG_BYTES) {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes).trim_end().to_owned();
                cause = format!("{cause}; its daemon's last log lines:\n{text}");
                log = Some(text);
            }
            Err(error) => cause = format!("{cause}; its daemon log was unreadable: {error:?}"),
        }
        let removed = match sandbox.process() {
            Some(_) => sandbox.stop(1).and_then(|()| sandbox.delete()),
            None => sandbox.delete(),
        };
        left = removed.err().map(|error| {
            format!(
                "its local Sandbox {} could not be removed: {error:?}",
                sandbox.directory().display()
            )
        });
    }
    let intent = directory.join("create.intent");
    let (message, removed) = match left {
        None => match std::fs::remove_file(&intent) {
            Ok(()) => (cause, container.is_some()),
            Err(error) => (
                format!("{cause}; its creation intent stays: {error}"),
                container.is_some(),
            ),
        },
        Some(left) => {
            let state = format!("creation of Sandbox {id} failed ({cause}); {left}");
            let kept = crate::discovery::save(&intent, SdkValue::text(&state))
                .err()
                .map(|error| format!(" (recording it failed: {})", error.message))
                .unwrap_or_default();
            (format!("{state}; its creation intent stays{kept}"), false)
        }
    };
    SdkRefusal {
        code: "Failed".to_owned(),
        message,
        detail: SdkValue::map([
            ("container", SdkValue::opt_text(container)),
            ("removed", SdkValue::Bool(removed)),
            ("log", SdkValue::opt_text(log)),
        ]),
    }
}
