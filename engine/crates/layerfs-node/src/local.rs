//! Native and namespaces Sandboxes (`deploy.sandbox` `native` or
//! `namespaces`, LAYERFS_SANDBOX): the project's daemon runs as a process of
//! this user instead of in a container. Native isolates nothing beyond the
//! caller's own sandbox; namespaces adds Linux namespaces and a cgroup.
//!
//! Each Sandbox gets a directory `<LAYERFS_LOCAL_DIR>/<prefix><project>-<n>`
//! with what a container held at fixed paths: its setup
//! (`/layerfs-local/config`), overlay template (`/layerfs-local/overlay`) and
//! Workspace mounts (`/workspaces`). The daemon opens the installed Store
//! where this process opens it (`storePath`), so the daemon setup and the
//! Store's locator name that path instead of `/layerfs-store`. The daemon
//! listens on 127.0.0.1 only. macOS serves loopback NFS as this user; Linux
//! keeps the root FUSE daemon and nonroot commands (501:20) from #402. Start,
//! failure and reclaim follow the Docker route: one attempt, a creation intent while the outcome is open, and a failed start's
//! daemon stopped and its directory deleted, each once.
use crate::job::{failed, refused};
use crate::registry::{lock, text, SandboxState, SANDBOXES};
use layerfs_api::{sandbox::ProviderCreate, SandboxApi};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::provision::StoreManifest;
use layerfs_sandbox::{CommandIdentity, Runtime, Sandbox};
use layerfs_sandbox::{LaunchRequest, ProviderLaunchOptions};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

/// Starts the project's next Sandbox as a native daemon. Linux requires
/// root before any effect. Called by
/// [`crate::sandbox::start`] under the project's guard, with the project
/// and its installed manifest already read.
pub(crate) fn start(
    args: &SdkValue,
    directory: &Path,
    project: (&crate::registry::Project, StoreManifest),
    backend: crate::runtime::Backend,
) -> Result<Arc<SandboxState>, SdkRefusal> {
    let deploy = args
        .get("deploy")
        .ok_or_else(|| refused("InvalidInput", "missing `deploy`"))?;
    // Inputs are read first, so a refused request leaves no creation intent.
    if cfg!(target_os = "linux") && crate::discovery::user().0 != 0 {
        return Err(refused(
            "Unsupported",
            "LAYERFS_SANDBOX=native or namespaces on Linux needs root for FUSE and commands as uid 501, gid 20; run inside the VM or CI box that isolates it",
        ));
    }
    let root = deploy.get_text("localDir").ok_or_else(|| {
        refused(
            "InvalidInput",
            "deploy.localDir is not configured (LAYERFS_LOCAL_DIR); a native Sandbox needs it",
        )
    })?;
    let runtime = native_runtime(root, backend)
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
        (runtime, backend, executable, store),
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
    (runtime, backend, executable, store): (Runtime, crate::runtime::Backend, PathBuf, String),
) -> Result<Arc<SandboxState>, SdkRefusal> {
    let name = text(args, "project")?;
    let sandbox_name = crate::sandbox::sandbox_name(args, id)?;
    let paths = runtime.native_paths(&sandbox_name).map_err(failed)?;
    let path_text = |path: &Path| {
        path.to_str()
            .map(str::to_owned)
            .ok_or_else(|| refused("InvalidInput", "LAYERFS_LOCAL_DIR must be UTF-8"))
    };
    let controller = layerfs_bridge::duplex::generate_keypair().map_err(failed)?;
    let daemon = layerfs_bridge::duplex::generate_keypair().map_err(failed)?;
    let mut setup = crate::sandbox::setup(daemon.private, controller.public);
    // macOS uses the caller; Linux keeps #402's separate nonroot identity.
    #[cfg(target_os = "macos")]
    {
        (setup.command_uid, setup.command_gid) = crate::discovery::user();
    }
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
    setup.encode().map_err(failed)?;
    // Read only if this process ends before the intent is resolved.
    crate::discovery::save(
        &directory.join("create.intent"),
        SdkValue::text(format!("creation of Sandbox {id} began and never finished")),
    )?;
    let store = crate::stores::shared_store(args)?;
    let labels = crate::sandbox::labels(name, store.as_deref(), id);
    let api = SandboxApi::with_runtime(Arc::new(runtime));
    let options = launch_options(backend, identity, &setup.store, args)?;
    let mut manifest = project.manifest.clone();
    manifest.locator = setup.store.clone();
    let created = api
        .create_provider_file(
            ProviderCreate {
                launch: LaunchRequest {
                    name: sandbox_name.clone(),
                    daemon_executable: executable,
                    daemon_config: Vec::new(),
                    labels,
                    startup_timeout: crate::sandbox::STARTUP,
                    options,
                },
                setup,
                controller_private: controller.private,
                daemon_peer: daemon.public,
            },
            &project.store,
            &manifest,
        )
        .map_err(|mut failure| {
            drop(failure.control.take());
            abandon(
                directory,
                id,
                format!(
                    "native sandbox stopped at {:?}: {:?}",
                    failure.phase, failure.cause
                ),
                failure.runtime.take(),
            )
        })?;
    let instance = created.startup.instance;
    let (sandbox, control, identity) = layerfs_api::internal::split_provider(created);
    let endpoint = match sandbox.endpoint() {
        Ok(endpoint) => endpoint,
        Err(error) => {
            drop(control);
            return Err(abandon(
                directory,
                id,
                format!("native endpoint after startup: {error:?}"),
                Some(sandbox),
            ));
        }
    };
    let dial = crate::pool::Dial {
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
        directory: saved_directory.to_owned(),
        runtime: sandbox,
        identity,
        controls: crate::pool::Controls::new(control, dial),
        mounts: Default::default(),
        processes: Default::default(),
    };
    let recorded = (|| {
        mark_owner(&self::directory(args, id, None)?, saved_directory)?;
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
        std::fs::remove_file(directory.join("create.intent")).map_err(failed)
    })();
    if let Err(refusal) = recorded {
        // The daemon runs and is this start's own: it is stopped and its
        // directory deleted like any failed start's, and the records written
        // so far are removed, so its number is free again.
        let SandboxState { runtime, .. } = state;
        for record in ["sandbox.record", "container"] {
            let _ = std::fs::remove_file(saved_directory.join(record));
        }
        return Err(abandon(
            directory,
            id,
            format!("local sandbox records: {}", refusal.message),
            Some(runtime),
        ));
    }
    let state = Arc::new(state);
    lock(&SANDBOXES)?.insert(state.directory.clone(), state.clone());
    Ok(state)
}

/// The native runtime this host offers, over `root`: macOS loopback NFS as
/// this user, or Linux FUSE with a root daemon (this module is built on those
/// two only). The other platform's provider is refused before root access.
#[cfg(target_os = "macos")]
fn native_runtime(
    root: &str,
    _: crate::runtime::Backend,
) -> Result<Runtime, layerfs_sandbox::RuntimeError> {
    layerfs_sandbox::native::NativeMacosProvider::new(root).map(Runtime::MacosNative)
}

#[cfg(target_os = "linux")]
fn native_runtime(
    root: &str,
    backend: crate::runtime::Backend,
) -> Result<Runtime, layerfs_sandbox::RuntimeError> {
    match backend {
        crate::runtime::Backend::Native => {
            layerfs_sandbox::native::NativeLinuxProvider::new(root).map(Runtime::LinuxNative)
        }
        crate::runtime::Backend::Namespaces => {
            layerfs_sandbox::namespaces::NamespaceProvider::new(root).map(Runtime::Namespaces)
        }
    }
}

/// How the daemon of `backend` is launched: the same local options, and for
/// the namespace backend the Store file its private root binds and the image
/// archive its root is unpacked from.
#[cfg(target_os = "linux")]
fn launch_options(
    backend: crate::runtime::Backend,
    identity: CommandIdentity,
    store: &str,
    args: &SdkValue,
) -> Result<ProviderLaunchOptions, SdkRefusal> {
    Ok(match backend {
        crate::runtime::Backend::Native => ProviderLaunchOptions::Local {
            command_identity: identity,
            environment: native_environment(),
        },
        crate::runtime::Backend::Namespaces => {
            let deploy = args
                .get("deploy")
                .ok_or_else(|| refused("InvalidInput", "missing `deploy`"))?;
            let (image, archive) = crate::runtime::namespaces_image(deploy)?;
            ProviderLaunchOptions::Namespaces {
                command_identity: identity,
                environment: native_environment(),
                store: PathBuf::from(store),
                image,
                archive,
            }
        }
    })
}

#[cfg(not(target_os = "linux"))]
fn launch_options(
    _: crate::runtime::Backend,
    identity: CommandIdentity,
    _: &str,
    _: &SdkValue,
) -> Result<ProviderLaunchOptions, SdkRefusal> {
    Ok(ProviderLaunchOptions::Local {
        command_identity: identity,
        environment: native_environment(),
    })
}

/// What a native Sandbox's daemon starts with besides `PATH` and the kernel
/// route its provider chooses: the NFS mount helper and reference overrides
/// this process was given (`LAYERFS_NFS_*`), so a test host can name its own
/// client.
#[cfg(target_os = "macos")]
fn native_environment() -> Vec<(String, String)> {
    let mut environment = Vec::new();
    for name in [
        "LAYERFS_NFS_MOUNT",
        "LAYERFS_NFS_UNMOUNT",
        "LAYERFS_NFS_FORCE",
        "LAYERFS_NFS_FLUSH",
        "LAYERFS_NFS_REFRESH",
    ] {
        if let Ok(value) = std::env::var(name) {
            environment.push((name.to_owned(), value));
        }
    }
    environment
}

/// Nothing on Linux: the provider chooses FUSE.
#[cfg(target_os = "linux")]
fn native_environment() -> Vec<(String, String)> {
    Vec::new()
}

/// Where this deployment puts Sandbox `id`:
/// `<LAYERFS_LOCAL_DIR>/<prefix><project>-<id>`, computed from the
/// configuration, as a record's directory must be. With no LAYERFS_LOCAL_DIR
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
                     LAYERFS_LOCAL_DIR (deploy.localDir) configured"
                ),
            )
        })?;
    let name = crate::sandbox::sandbox_name(args, id)?;
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

/// The deployment's `localDir` (LAYERFS_LOCAL_DIR), when set, is an absolute
/// path with no `.` or `..`. A call with another is refused before any record
/// is read: the setting is the caller's, not a record's (#529 finding 6).
pub(crate) fn local_dir_setting(args: &SdkValue) -> Result<(), SdkRefusal> {
    let Some(root) = (args.get("deploy")).and_then(|deploy| deploy.get_text("localDir")) else {
        return Ok(());
    };
    let root = Path::new(root);
    let dotted = (root.components()).any(|c| matches!(c, Component::ParentDir | Component::CurDir));
    if root.is_absolute() && !dotted {
        return Ok(());
    }
    Err(refused(
        "InvalidInput",
        format!(
            "deploy.localDir (LAYERFS_LOCAL_DIR) {} must be an absolute path with no . or ..",
            root.display()
        ),
    ))
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

/// With no LAYERFS_LOCAL_DIR, a record's `directory` is used only when it
/// is its own real path and its `owner` marker (not followed if a symlink)
/// names the real path of this record's directory
/// `LAYERFS_HOME/<project>/sandboxes/<id>`. Anything else, including a
/// record made before the marker, is refused before any effect.
pub(crate) fn owned(args: &SdkValue, id: &str, own: &Path) -> Result<(), SdkRefusal> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let refuse = |why: String| {
        refused(
            "InvalidInput",
            format!(
                "Sandbox {id}'s local record names {}, {why}, so with no LAYERFS_LOCAL_DIR it is \
                 neither used nor deleted; set LAYERFS_LOCAL_DIR (deploy.localDir) to the \
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
    let record = std::fs::canonicalize(crate::discovery::sandbox_dir(args, id)?).map_err(failed)?;
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
/// directory deleted, each once, and the intent removed. If that cleanup
/// fails, the intent stays and says what is left.
fn abandon(
    directory: &Path,
    id: &str,
    mut cause: String,
    sandbox: Option<Box<Sandbox>>,
) -> SdkRefusal {
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
        let removed = sandbox.stop(1).and_then(|()| sandbox.delete());
        left = removed.err().map(|error| {
            format!(
                "its local Sandbox {} could not be removed: {error:?}",
                sandbox.identity()
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
