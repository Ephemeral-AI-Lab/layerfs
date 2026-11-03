//! Native and namespaces Sandboxes (`deploy.sandbox` `native` on macOS or
//! `namespaces` on Linux, LAYERFS_SANDBOX): the project's daemon runs as a
//! process of this user instead of in a container. Native isolates nothing
//! beyond the caller's own sandbox; namespaces adds Linux namespaces and a
//! cgroup. Linux starts no native Sandbox (#533 O27): `native` there is
//! refused before any effect.
//!
//! Each Sandbox gets a directory `<deploy.localDir>/<prefix><name>` (the
//! machine's `machine/native/run/`, beside the image cache the namespaces
//! backend keeps there) with what a container held at fixed paths: its setup
//! (`/layerfs-local/config`), overlay template (`/layerfs-local/overlay`) and
//! Workspace mounts (`/workspaces`). The daemon opens the installed Store
//! where this process opens it (`storePath`), so the daemon setup and the
//! Store's locator name that path instead of `/layerfs-store`. The daemon
//! listens on 127.0.0.1 only. macOS serves loopback NFS as this user; Linux
//! namespaces keep the root FUSE daemon and nonroot commands (501:20) from
//! #402. Start, failure and reclaim follow the Docker route: one attempt,
//! `Unknown` while the outcome is open, and a failed start's daemon stopped
//! and its directory deleted, each once.
use crate::common::outcome::{failed, refused};
use crate::sandbox::registry::{lock, text, SandboxState, SANDBOXES};
use crate::sandbox::serve::OpenedStore;
use crate::{sandbox::ProviderCreate, SandboxApi};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::{DaemonStore, KernelRoute};
use layerfs_sandbox::{CommandIdentity, Runtime, Sandbox};
use layerfs_sandbox::{LaunchRequest, ProviderLaunchOptions};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

/// Starts the Sandbox the call names as a native (macOS) or namespaces
/// (Linux) daemon. Linux refuses `native` and requires root before any
/// effect. Called by
/// [`crate::sandbox::serve::start`] under its folder's guard, with the
/// project's installed Store file or its PostgreSQL Store already read. A
/// namespaces root binds a sqlite Store's file and nothing for a PostgreSQL
/// Store, whose daemon dials the server (#535 §7.4).
pub(crate) fn start(
    args: &SdkValue,
    directory: &Path,
    project: OpenedStore,
    backend: crate::sandbox::runtime::Backend,
) -> Result<Arc<SandboxState>, SdkRefusal> {
    let deploy = args
        .get("deploy")
        .ok_or_else(|| refused("InvalidInput", "missing `deploy`"))?;
    // Inputs are read first, so a refused request leaves no creation intent.
    #[cfg(target_os = "linux")]
    if backend == crate::sandbox::runtime::Backend::HostProcess {
        return Err(refused(
            "Unsupported",
            crate::sandbox::runtime::NATIVE_ON_LINUX,
        ));
    }
    if cfg!(target_os = "linux") && crate::sandbox::files::user().0 != 0 {
        return Err(refused(
            "Unsupported",
            "LAYERFS_SANDBOX=namespaces on Linux needs root for FUSE and commands as uid 501, gid 20; run inside the VM or CI box that isolates it",
        ));
    }
    let root = deploy.get_text("localDir").ok_or_else(|| {
        refused(
            "InvalidInput",
            "deploy.localDir is not configured; a native Sandbox needs it (the machine's machine/native/run)",
        )
    })?;
    let runtime = native_runtime(root, backend)
        .map_err(|error| refused("InvalidInput", format!("deploy.localDir {root}: {error}")))?;
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
    let id = text(args, "sandbox")?;
    start_named(
        args,
        directory,
        id,
        project,
        (runtime, backend, executable, store),
    )
}

/// Starts Sandbox `id`: directory and daemon on the installed Store, its
/// first control connection, then its records in its folder `directory`.
fn start_named(
    args: &SdkValue,
    directory: &Path,
    id: &str,
    project: OpenedStore,
    (runtime, backend, executable, store): (
        Runtime,
        crate::sandbox::runtime::Backend,
        PathBuf,
        String,
    ),
) -> Result<Arc<SandboxState>, SdkRefusal> {
    let name = text(args, "project")?;
    let sandbox_name = crate::sandbox::serve::sandbox_name(args, id)?;
    let paths = runtime.native_paths(&sandbox_name).map_err(failed)?;
    let path_text = |path: &Path| {
        path.to_str()
            .map(str::to_owned)
            .ok_or_else(|| refused("InvalidInput", "deploy.localDir must be UTF-8"))
    };
    let controller = layerfs_bridge::duplex::generate_keypair().map_err(failed)?;
    let daemon = layerfs_bridge::duplex::generate_keypair().map_err(failed)?;
    let route = match backend {
        crate::sandbox::runtime::Backend::HostProcess => KernelRoute::Nfs,
        _ => KernelRoute::Fuse,
    };
    // the Store's path in the daemon's filesystem, which is this one
    let (file, daemon_store) = match project {
        OpenedStore::File(file) => {
            let (project, installed) = *file;
            let mut existing = project.manifest.clone();
            existing.daemon_sqlite = installed.daemon_sqlite.clone();
            existing.locator = store.clone();
            (
                Some((project, installed)),
                DaemonStore::Sqlite {
                    locator: store.clone(),
                    install: Some(existing),
                },
            )
        }
        OpenedStore::Postgres(selected) => (None, *selected),
    };
    let mut setup =
        crate::sandbox::serve::setup(daemon.private, controller.public, daemon_store, route);
    // macOS uses the caller; Linux keeps #402's separate nonroot identity.
    #[cfg(target_os = "macos")]
    {
        (setup.command_uid, setup.command_gid) = crate::sandbox::files::user();
    }
    setup.listen = "127.0.0.1:0".into();
    setup.overlay = path_text(&paths.overlay)?;
    setup.mounts = path_text(&paths.mounts)?;
    let identity = CommandIdentity {
        uid: setup.command_uid,
        gid: setup.command_gid,
    };
    setup.encode().map_err(failed)?;
    let daemon_locator = match &setup.store {
        DaemonStore::Sqlite { locator, .. } => locator.clone(),
        _ => String::new(),
    };
    let store = crate::store::identity::shared_store(args)?;
    let labels = crate::sandbox::serve::labels(name, store.as_deref(), id);
    let api = SandboxApi::with_runtime(Arc::new(runtime));
    let options = launch_options(backend, identity, args)?;
    let request = ProviderCreate {
        launch: LaunchRequest {
            name: sandbox_name.clone(),
            daemon_executable: executable,
            daemon_config: Vec::new(),
            labels,
            startup_timeout: crate::sandbox::serve::STARTUP,
            options,
        },
        setup,
        controller_private: controller.private,
        daemon_peer: daemon.public,
    };
    let created = match &file {
        Some((project, _)) => {
            let mut manifest = project.manifest.clone();
            manifest.locator = daemon_locator.clone();
            api.create_provider_file(request, &project.store, &manifest)
        }
        None => api.create_provider_postgres(request),
    };
    let created = created.map_err(|mut failure| {
        drop(failure.control.take());
        abandon(
            id,
            format!(
                "native sandbox stopped at {:?}: {:?}",
                failure.phase, failure.cause
            ),
            failure.runtime.take(),
        )
    })?;
    let instance = created.startup.instance;
    let (sandbox, control, identity) = crate::internal::split_provider(created);
    let endpoint = match sandbox.endpoint() {
        Ok(endpoint) => endpoint,
        Err(error) => {
            drop(control);
            return Err(abandon(
                id,
                format!("native endpoint after startup: {error:?}"),
                Some(sandbox),
            ));
        }
    };
    let dial = crate::daemon::pool::Dial {
        endpoint,
        host: None,
        secret: controller.private,
        peer: daemon.public,
        instance,
    };
    let state = SandboxState {
        id: id.to_owned(),
        project: name.to_owned(),
        name: sandbox_name,
        directory: directory.to_owned(),
        runtime: sandbox,
        identity,
        controls: crate::daemon::pool::Controls::new(control, dial),
        mounts: Default::default(),
    };
    let recorded = (|| {
        mark_owner(&self::directory(args, id, None)?, directory)?;
        crate::sandbox::files::save_sandbox(
            &state.directory,
            &state,
            args,
            controller.private,
            daemon.public,
            instance,
        )
    })();
    if let Err(refusal) = recorded {
        // The daemon runs and is this start's own: it is stopped and its
        // directory deleted like any failed start's, and the keys written
        // so far are removed, so the name is free again.
        let SandboxState { runtime, .. } = state;
        let _ = std::fs::remove_file(directory.join("control.json"));
        return Err(abandon(
            id,
            format!("local sandbox records: {}", refusal.message),
            Some(runtime),
        ));
    }
    let state = Arc::new(state);
    lock(&SANDBOXES)?.insert(state.directory.clone(), state.clone());
    Ok(state)
}

/// The local runtime this host offers, over `root`: macOS loopback NFS as
/// this user, or Linux namespaces with a root FUSE daemon (this module is
/// built on those two only). Linux has no native runtime (#533 O27).
#[cfg(target_os = "macos")]
fn native_runtime(
    root: &str,
    _: crate::sandbox::runtime::Backend,
) -> Result<Runtime, layerfs_sandbox::RuntimeError> {
    layerfs_sandbox::host_process::HostProcessProvider::new(root).map(Runtime::HostProcess)
}

#[cfg(target_os = "linux")]
fn native_runtime(
    root: &str,
    backend: crate::sandbox::runtime::Backend,
) -> Result<Runtime, layerfs_sandbox::RuntimeError> {
    match backend {
        crate::sandbox::runtime::Backend::HostProcess => Err(
            layerfs_sandbox::RuntimeError::Unsupported(crate::sandbox::runtime::NATIVE_ON_LINUX),
        ),
        crate::sandbox::runtime::Backend::Namespaces => {
            layerfs_sandbox::namespaces::NamespaceProvider::new(root).map(Runtime::Namespaces)
        }
    }
}

/// How the daemon of `backend` is launched: the same local options, and for
/// the namespace backend the image archive its root is unpacked from. Its
/// binds follow from the daemon's Store when it is launched.
#[cfg(target_os = "linux")]
fn launch_options(
    backend: crate::sandbox::runtime::Backend,
    identity: CommandIdentity,
    args: &SdkValue,
) -> Result<ProviderLaunchOptions, SdkRefusal> {
    Ok(match backend {
        crate::sandbox::runtime::Backend::HostProcess => {
            return Err(refused(
                "Unsupported",
                crate::sandbox::runtime::NATIVE_ON_LINUX,
            ))
        }
        crate::sandbox::runtime::Backend::Namespaces => {
            let deploy = args
                .get("deploy")
                .ok_or_else(|| refused("InvalidInput", "missing `deploy`"))?;
            let (image, archive) = crate::sandbox::runtime::namespaces_image(deploy)?;
            ProviderLaunchOptions::Namespaces {
                command_identity: identity,
                // the daemon starts with `PATH` and its provider's kernel route only;
                // 2.0 passes it no platform setting (#533 step 17)
                environment: Vec::new(),
                image,
                archive,
            }
        }
    })
}

#[cfg(not(target_os = "linux"))]
fn launch_options(
    _: crate::sandbox::runtime::Backend,
    identity: CommandIdentity,
    _: &SdkValue,
) -> Result<ProviderLaunchOptions, SdkRefusal> {
    Ok(ProviderLaunchOptions::Local {
        command_identity: identity,
        environment: Vec::new(),
    })
}

/// Where this deployment puts Sandbox `id`:
/// `<deploy.localDir>/<prefix><id>`, computed from the
/// configuration, as a record's directory must be. With no deploy.localDir
/// configured, `recorded` (a record's own parent directory) stands in for
/// it: a call on a Sandbox follows its record (#529).
pub(crate) fn directory(
    args: &SdkValue,
    id: &str,
    recorded: Option<&Path>,
) -> Result<PathBuf, SdkRefusal> {
    let root = (args.get("deploy"))
        .and_then(|deploy| deploy.get_text("localDir"))
        .map(Path::new)
        .or(recorded)
        .ok_or_else(|| {
            refused(
                "InvalidInput",
                format!(
                    "Sandbox {id} is a local Sandbox; its record is used only with \
                     deploy.localDir configured"
                ),
            )
        })?;
    let name = crate::sandbox::serve::sandbox_name(args, id)?;
    let plain = |path: &Path| {
        !path
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    };
    let single = matches!(
        Path::new(&name).components().collect::<Vec<_>>()[..],
        [Component::Normal(_)]
    );
    if !root.is_absolute() || !plain(root) || !single {
        return Err(refused(
            "InvalidInput",
            format!("local Sandbox directory {}/{name}", root.display()),
        ));
    }
    Ok(root.join(name))
}

/// The marker in a local Sandbox's directory naming, after realpath, the
/// directory of the record that owns it (#529).
const OWNER: &str = "owner";

/// Writes the Sandbox directory's `owner` marker once, mode 0600: the real
/// path of its record's directory `record`. A marker already there, or a
/// symlink in its place, refuses the start.
fn mark_owner(own: &Path, record: &Path) -> Result<(), SdkRefusal> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let real = std::fs::canonicalize(record).map_err(failed)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(own.join(OWNER))
        .map_err(failed)?;
    file.write_all(real.as_os_str().as_encoded_bytes())
        .map_err(failed)
}

/// With no deploy.localDir, a record's `directory` is used only when it
/// is its own real path and its `owner` marker (not followed if a symlink)
/// names the real path of this record's folder `<root>/sandbox/<name>`.
/// Anything else, including a record made before the marker, is refused
/// before any effect.
pub(crate) fn owned(args: &SdkValue, id: &str, own: &Path) -> Result<(), SdkRefusal> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let refuse = |why: String| {
        refused(
            "InvalidInput",
            format!(
                "Sandbox {id}'s local record names {}, {why}, so with no deploy.localDir it is \
                 neither used nor deleted; set deploy.localDir to the \
                 directory it was made under",
                own.display()
            ),
        )
    };
    match std::fs::canonicalize(own) {
        Ok(real) if real == own => {}
        Ok(real) => return Err(refuse(format!("which resolves to {}", real.display()))),
        Err(error) => return Err(refuse(format!("which cannot be resolved ({error})"))),
    }
    let mut marker = Vec::new();
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(own.join(OWNER))
        .and_then(|file| file.take(4096).read_to_end(&mut marker))
        .map_err(|error| refuse(format!("whose owner marker cannot be read ({error})")))?;
    let record =
        std::fs::canonicalize(crate::sandbox::files::sandbox_dir(args)?).map_err(failed)?;
    if marker != record.as_os_str().as_encoded_bytes() {
        let named = String::from_utf8_lossy(&marker);
        return Err(refuse(format!(
            "whose owner marker names {named}, not {}",
            record.display()
        )));
    }
    Ok(())
}

/// The refusal for a failed start. Everything a local start makes is its
/// own (a directory and a child process), so each effect is known: the
/// daemon's last log lines are read once, then it is stopped and its
/// directory deleted, each once (`Failed`). If that cleanup fails, the
/// outcome is `Unknown`, saying what is left.
fn abandon(id: &str, mut cause: String, sandbox: Option<Box<Sandbox>>) -> SdkRefusal {
    let container = sandbox.as_ref().map(|s| s.identity().to_string());
    let mut log = None;
    let mut left = None;
    if let Some(mut sandbox) = sandbox {
        match sandbox.log_tail(
            crate::sandbox::serve::LOG_LINES,
            crate::sandbox::serve::LOG_BYTES,
        ) {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes).trim_end().to_owned();
                cause = format!("{cause}; its daemon's last log lines:\n{text}");
                log = Some(text);
            }
            Err(error) => cause = format!("{cause}; its daemon log was unreadable: {error:?}"),
        }
        let removed = sandbox.stop(1).and_then(|()| sandbox.delete());
        left = removed.err().map(|error| {
            format!(
                "its local Sandbox {} could not be removed: {error:?}",
                sandbox.identity()
            )
        });
    }
    let (code, message, removed) = match left {
        None => ("Failed", cause, container.is_some()),
        Some(left) => (
            "Unknown",
            format!("creation of Sandbox {id} failed ({cause}); {left}"),
            false,
        ),
    };
    SdkRefusal {
        code: code.to_owned(),
        message,
        detail: SdkValue::map([
            ("container", SdkValue::opt_text(container)),
            ("removed", SdkValue::Bool(removed)),
            ("log", SdkValue::opt_text(log)),
        ]),
    }
}
