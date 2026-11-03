//! Named Sandboxes on the machine that runs them (#536 §6): a container
//! `<prefix><name>` (or a local daemon whose directory has that name), each
//! running a daemon on its project's Store, recorded in its folder
//! `<root>/sandbox/<name>/`. A Sandbox starts only when an agent asks for
//! one ([`create`]); its records go when an agent closes it ([`close`]),
//! after a stopped runtime is deleted ([`reclaim`]) when it ended by itself.
//!
//! A start that failed before anything reached the Store volume removes the
//! container it made (stop, then delete, each once), then reports the
//! failure with its daemon's last log lines, read once before the delete.
//! One whose effect is unknown is `Unknown`: the caller keeps the name's
//! record as `unknown` until `sandbox.close` clears it. Nothing is retried
//! on the caller's behalf.
use crate::common::outcome::{failed, refused, Outcome};
use crate::daemon::values;
use crate::internal::split_provider;
use crate::sandbox::registry::{lock, text, SandboxState, SANDBOXES};
use crate::sandbox::{ProviderCreate, ProviderFailure};
use crate::{OperationFailure, SandboxApi, SandboxCause, SandboxPhase};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::daemon_setup::{DaemonLimits, DaemonSetup};
use layerfs_bridge::provision::StoreManifest;
use layerfs_bridge::{DaemonStore, KernelRoute};
use layerfs_sandbox::docker::{CreatePhase, Docker, DockerProvider};
use layerfs_sandbox::Runtime;
use layerfs_sandbox::{LaunchRequest, ProviderLaunchOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Port the daemon listens on inside its container.
const PORT: u16 = 30421;
/// Wait for the daemon's listener after Start.
pub(crate) const STARTUP: Duration = Duration::from_secs(30);
/// Wait for one Engine control response.
const ENGINE: Duration = Duration::from_secs(30);
/// A failed create's report keeps its daemon's last log lines, up to this many bytes.
pub(crate) const LOG_LINES: u16 = 20;
pub(crate) const LOG_BYTES: usize = 2048;

pub(crate) fn operation(failure: Box<OperationFailure>) -> SdkRefusal {
    values::refusal(&failure).unwrap_or_else(|| failed(failure))
}

/// Workspaces one Sandbox holds at once; a mount into a full one is refused
/// `Capacity`.
pub(crate) const WORKSPACES: u32 = 16;

/// Control connections a Sandbox's daemon admits: [`crate::daemon::pool::CONNECTIONS`]
/// for this process and the rest for others (#352 F-017).
pub(crate) const DAEMON_CONNECTIONS: u16 = 8;

pub(crate) fn setup(
    private: [u8; 32],
    peer: [u8; 32],
    store: DaemonStore,
    route: KernelRoute,
) -> DaemonSetup {
    DaemonSetup {
        listen: format!("0.0.0.0:{PORT}"),
        private_key: private,
        control_peer: peer,
        route,
        overlay: "/layerfs-local/overlay/overlay.sqlite".into(),
        mounts: "/workspaces".into(),
        command_uid: 501,
        command_gid: 20,
        limits: DaemonLimits {
            connections: DAEMON_CONNECTIONS,
            handshake_ms: 2000,
            // The daemon's shared immutable-object cache: a fixed bound, the
            // daemon's own default. 64 KiB held too few directory and inode
            // pages for lookups in a large directory to stay resident
            // (#356 F-032); 8 MiB was overrun by one `find`, vitest or
            // `ls -l` of 10,000 entries, so a later Workspace re-read it all.
            // 64 MiB by owner decision (#359 warm daemon, record 031).
            cache_bytes: 64 * 1024 * 1024,
            namespaces: WORKSPACES,
            pager_kib: 1024,
            serial_low_water: 256,
        },
        store,
    }
}

/// A container name prefix when the deployment sets none.
pub(crate) const PREFIX: &str = "layerfs-";

/// Sandbox `id`'s runtime name (its id is its name in the home), on every
/// backend: `<prefix><id>`. The prefix is the deployment's (`lfs-<home id>-`
/// from the home; `layerfs-` by default), so `docker ps` says whose each
/// container is, and a native or namespaces Sandbox's directory carries the
/// name its container would have.
pub(crate) fn sandbox_name(args: &SdkValue, id: &str) -> Result<String, SdkRefusal> {
    let prefix = args
        .get("deploy")
        .and_then(|d| d.get_text("containerPrefix"))
        .filter(|prefix| !prefix.is_empty())
        .unwrap_or(PREFIX);
    crate::common::names::SandboxName::parse(id)?;
    Ok(format!("{prefix}{id}"))
}

/// The container labels of Sandbox `id` of `project`: what it is, whose,
/// and which. Saved in its record, so a reopen expects the same.
/// A project of a shared Store (#506) also carries `layerfs.store`.
pub(crate) fn labels(project: &str, store: Option<&str>, id: &str) -> Vec<(String, String)> {
    [
        ("layerfs.role", "sandbox"),
        ("layerfs.project", project),
        ("layerfs.sandbox", id),
    ]
    .into_iter()
    .chain(store.map(|store| ("layerfs.store", store)))
    .map(|(key, value)| (key.to_owned(), value.to_owned()))
    .collect()
}

/// The Store a call's project selected for its Sandbox's daemon: the
/// project's installed Store file, or a PostgreSQL Store (#506), whose
/// Sandbox mounts no volume.
pub(crate) enum OpenedStore {
    File(Box<(crate::sandbox::registry::Project, StoreManifest)>),
    Postgres(Box<DaemonStore>),
}

/// The installed Store file of the project a call names.
fn installed(args: &SdkValue) -> Result<OpenedStore, SdkRefusal> {
    let name = text(args, "project")?;
    if args.get_text("store").is_some() {
        let identity = crate::store::identity::identity(args)?;
        crate::store::identity::sqlite_only(&identity, "a Sandbox of a Store file")?;
    }
    let project = crate::sandbox::files::project(args)?;
    let Some(installed) = project.installed.clone() else {
        return Err(refused(
            "NoStore",
            format!("project {name} has no installed Store: its Import did not finish, so import it again"),
        ));
    };
    Ok(OpenedStore::File(Box::new((project, installed))))
}

/// Starts the Sandbox the call names (`sandbox`) on the deployment's image
/// (`deploy.image`), its daemon on `store`. Called under the guard of its
/// folder `directory`, so no other call on it runs meanwhile.
pub(crate) fn start(
    args: &SdkValue,
    directory: &Path,
    store: OpenedStore,
) -> Result<Arc<SandboxState>, SdkRefusal> {
    let deploy = args
        .get("deploy")
        .ok_or_else(|| refused("InvalidInput", "missing `deploy`"))?;
    // Inputs are read first, so a refused request leaves no creation intent.
    if let Some(backend) = crate::sandbox::runtime::backend(deploy)? {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            return crate::sandbox::launch::local::start(args, directory, store, backend);
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            drop((store, backend));
            return Err(refused(
                "Unsupported",
                "LAYERFS_SANDBOX=native requires Linux (FUSE) or macOS (loopback NFS); use Docker",
            ));
        }
    }
    let docker = text(deploy, "docker")?;
    let image = deploy
        .get_text("image")
        .ok_or_else(|| {
            refused(
                "InvalidInput",
                "deploy.image is not configured (LAYERFS_IMAGE); a Workspace needs it for its Sandbox",
            )
        })?
        .to_owned();
    // checked here, before any intent is written: a malformed digest is the
    // caller's mistake, not an uncertain creation
    let digest = image.strip_prefix("sha256:").unwrap_or_default();
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(refused(
            "InvalidInput",
            format!("image must be an exact local image digest, sha256:<64 hex>, not {image}"),
        ));
    }
    let store_volume = match &store {
        OpenedStore::File(_) => Some(text(deploy, "volume")?.to_owned()),
        OpenedStore::Postgres(_) => None,
    };
    let executable = PathBuf::from(text(deploy, "daemon")?);
    let id = text(args, "sandbox")?;
    start_claimed(
        args,
        directory,
        id,
        store,
        (docker, image, store_volume, executable),
    )
}

/// Starts Sandbox `id`: container, daemon on the installed Store, then its
/// records in its folder `directory`.
fn start_claimed(
    args: &SdkValue,
    directory: &Path,
    id: &str,
    opened: OpenedStore,
    (docker, image, store_volume, executable): (&str, String, Option<String>, PathBuf),
) -> Result<Arc<SandboxState>, SdkRefusal> {
    let name = text(args, "project")?;
    let controller = layerfs_bridge::duplex::generate_keypair().map_err(failed)?;
    let daemon = layerfs_bridge::duplex::generate_keypair().map_err(failed)?;
    // the Store's path in the container follows where its folder comes from
    let mut manifest = None;
    let daemon_store = match &opened {
        OpenedStore::File(file) => {
            let (project, installed) = &**file;
            let source = store_volume.as_deref().unwrap_or_default();
            let locator = layerfs_sandbox::docker::store_locator(source).ok_or_else(|| {
                refused(
                    "InvalidInput",
                    format!("deploy.volume {source:?} is no volume, volume folder or host folder"),
                )
            })?;
            let mut existing = project.manifest.clone();
            existing.locator = locator.to_owned();
            manifest = Some(existing.clone());
            existing.daemon_sqlite = installed.daemon_sqlite.clone();
            DaemonStore::Sqlite {
                locator: locator.to_owned(),
                install: Some(existing),
            }
        }
        OpenedStore::Postgres(selected) => (**selected).clone(),
    };
    let daemon_setup = setup(
        daemon.private,
        controller.public,
        daemon_store,
        KernelRoute::Fuse,
    );
    let store = crate::store::identity::shared_store(args)?;
    let container_name = sandbox_name(args, id)?;
    let docker = Docker::new(docker, ENGINE)
        .map_err(failed)?
        .naming(&container_name)
        .map_err(|_| refused("InvalidInput", format!("container name {container_name}")))?;
    let provider = Arc::new(Runtime::Docker(DockerProvider::new(docker)));
    let request = ProviderCreate {
        launch: LaunchRequest {
            name: container_name.clone(),
            daemon_executable: executable,
            daemon_config: Vec::new(),
            labels: labels(name, store.as_deref(), id),
            startup_timeout: STARTUP,
            options: ProviderLaunchOptions::Docker {
                image,
                store_volume,
                port: PORT,
                connect_host: crate::daemon::pool::daemon_host(args),
            },
        },
        setup: daemon_setup,
        controller_private: controller.private,
        daemon_peer: daemon.public,
    };
    let api = SandboxApi::with_runtime(provider);
    let created = match &opened {
        OpenedStore::File(file) => api.create_provider_file(
            request,
            &file.0.store,
            manifest.as_ref().unwrap_or(&file.0.manifest),
        ),
        OpenedStore::Postgres(_) => api.create_provider_postgres(request),
    };
    let owner = created.map_err(|failure| abandon(id, failure))?;
    let container = owner.runtime.identity().to_string();
    let instance = owner.startup.instance;
    let (runtime, control, identity) = split_provider(owner);
    let dial = crate::daemon::pool::Dial {
        endpoint: runtime.endpoint().map_err(failed)?,
        host: crate::daemon::pool::daemon_host(args),
        secret: controller.private,
        peer: daemon.public,
        instance,
    };
    let state = Arc::new(SandboxState {
        id: id.to_owned(),
        project: name.to_owned(),
        name: container_name,
        directory: directory.to_owned(),
        runtime,
        identity,
        controls: crate::daemon::pool::Controls::new(control, dial),
        mounts: Default::default(),
    });
    // a started container that cannot be recorded is left running: its
    // outcome is not known to the caller, who keeps the name `unknown`
    crate::sandbox::files::save_sandbox(
        &state.directory,
        &state,
        args,
        controller.private,
        daemon.public,
        instance,
    )
    .map_err(|refusal| unrecorded(id, &container, refusal))?;
    lock(&SANDBOXES)?.insert(state.directory.clone(), state.clone());
    Ok(state)
}

/// The `Unknown` of a started Sandbox `id` whose records could not be
/// written: its runtime `left` runs and nothing here names it.
pub(crate) fn unrecorded(id: &str, left: &str, refusal: SdkRefusal) -> SdkRefusal {
    refused(
        "Unknown",
        format!(
            "Sandbox {id} started ({left}) but could not be recorded: {}; it is left \
             running, so remove it by hand, then `sandbox.close` frees the name",
            refusal.message
        ),
    )
}

/// Whether a failed create's every effect is known: nothing reached the
/// Store volume, and any container it made is acknowledged by identity.
fn settled(failure: &ProviderFailure) -> bool {
    // A Create the Engine refused (409: the container name is taken) made no container.
    let unknown_container = match &failure.cause {
        SandboxCause::Provider(launch) => match &launch.cause {
            layerfs_sandbox::RuntimeError::Docker(docker_failure) => {
                match docker_failure.as_ref() {
                    layerfs_sandbox::docker::ProviderFailure::Create(create) => {
                        create.phase == CreatePhase::Create
                            && !matches!(
                                create.transfer.cause,
                                layerfs_sandbox::RuntimeError::Http(409)
                            )
                    }
                    _ => false,
                }
            }
            _ => false,
        },
        _ => false,
    };
    failure.phase != SandboxPhase::Install && failure.installed.is_none() && !unknown_container
}

/// The refusal for a failed create. A settled one reads its container's log
/// tail once and removes the container (stop, then delete, each once):
/// `Failed`. Otherwise, or when that removal fails, `Unknown`, saying what
/// is left.
fn abandon(id: &str, mut failure: Box<ProviderFailure>) -> SdkRefusal {
    let mut cause = format!(
        "sandbox create stopped at {:?}: {:?}",
        failure.phase, failure.cause
    );
    let container = failure
        .runtime
        .as_ref()
        .map(|sandbox| sandbox.identity().to_string());
    drop(failure.control.take());
    let mut log = None;
    let left = if !settled(&failure) {
        Some("its effect on the Store volume or the Engine is unknown".to_owned())
    } else if let Some(sandbox) = failure.runtime.as_mut() {
        match sandbox.log_tail(LOG_LINES, LOG_BYTES) {
            Ok(bytes) => {
                let text = String::from_utf8_lossy(&bytes).trim_end().to_owned();
                cause = format!("{cause}; its daemon's last log lines:\n{text}");
                log = Some(text);
            }
            Err(error) => cause = format!("{cause}; its daemon log was unreadable: {error:?}"),
        }
        let removed = sandbox.stop(1).and_then(|()| sandbox.delete());
        removed
            .err()
            .map(|error| format!("its container could not be removed: {error:?}"))
    } else {
        None
    };
    let (code, message, removed) = match left {
        None => ("Failed", cause, container.is_some()),
        Some(left) => (
            "Unknown",
            format!(
                "creation of Sandbox {id} failed ({cause}); container {}; {left}",
                container.as_deref().unwrap_or("none acknowledged")
            ),
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

/// `sandbox.create(sandbox, { image, platform })`: under its folder's guard,
/// starts the Sandbox the call names on the call's image (`deploy.image`)
/// and waits for its daemon: `{ sandbox, name, image, platform, runtime,
/// route, runtime_id }`. A folder that already records a started Sandbox
/// is `Exists`. With `deploy.sandbox` `native` or `namespaces` it starts a
/// daemon process instead of a container; a native one's `image` is
/// `local`. `platform` (#529) is checked against that backend and recorded.
/// The only call that starts a Sandbox. Failures are `Failed` or
/// `Unknown`, never retried.
pub(crate) fn create(args: &SdkValue) -> Outcome {
    // the platform is checked against the deployment before any effect (#529)
    let deploy = args
        .get("deploy")
        .ok_or_else(|| refused("InvalidInput", "missing `deploy`"))?;
    let platform =
        crate::sandbox::runtime::platform(args, crate::sandbox::runtime::backend(deploy)?)?;
    // a PostgreSQL Store's URL is the call's
    let store = match crate::store::postgres::sandbox_store(args)? {
        Some(selected) => OpenedStore::Postgres(Box::new(selected)),
        None => installed(args)?,
    };
    let directory = crate::sandbox::files::sandbox_dir(args)?;
    crate::common::home::private_dir(&directory)?;
    let made = (|| {
        let _guard = crate::sandbox::files::guard(&directory)?;
        if crate::sandbox::files::started(&directory)?.is_some() {
            return Err(refused(
                "Exists",
                format!(
                    "a Sandbox is recorded at {} on this machine; `sandbox.close` closes it",
                    directory.display()
                ),
            ));
        }
        start(args, &directory, store)
    })();
    let state = match made {
        Ok(state) => state,
        Err(refusal) => {
            // a worker's folder held only this call's lock; on `native` the
            // folder keeps the caller's record, which the caller removes
            if refusal.code == "Failed" || refusal.code == "InvalidInput" {
                let _ = std::fs::remove_file(directory.join("lock"));
                let _ = std::fs::remove_dir(&directory);
            }
            return Err(refusal);
        }
    };
    Ok(SdkValue::map([
        ("sandbox", SdkValue::text(&state.id)),
        ("name", SdkValue::text(&state.name)),
        (
            "image",
            SdkValue::text(crate::sandbox::runtime::image(&state.runtime)),
        ),
        ("platform", SdkValue::text(platform)),
        (
            "runtime",
            SdkValue::text(crate::sandbox::runtime::kind(&state.runtime)),
        ),
        (
            "route",
            SdkValue::text(crate::sandbox::files::route(&state.runtime)),
        ),
        (
            "runtime_id",
            SdkValue::text(state.runtime.identity().to_string()),
        ),
    ]))
}

/// `sandbox.close(sandbox, { force })`: stops Sandbox `sandbox` now, under
/// its folder's guard. Refused `NoSuchSandbox` when nothing is recorded,
/// and, unless `force`, `MountsActive` while a Workspace is mounted in it.
/// Forced (O31), it stops the Sandbox through its backend anyway and
/// answers each Workspace it could not drain as `Unknown` (`unknown`: their
/// paths): their uncommitted changes and processes end with the Sandbox.
/// Stop, then delete, each attempted once: a failed stop is not followed by
/// a delete and keeps its cause; the folder goes after the delete. A
/// Sandbox whose runtime ended by itself is reclaimed ([`reclaim`]) and
/// answered `NoSuchSandbox`, as gone.
pub(crate) fn close(args: &SdkValue) -> Outcome {
    let id = text(args, "sandbox")?.to_owned();
    let directory = crate::sandbox::files::sandbox_dir(args)?;
    if crate::sandbox::files::started(&directory)?.is_none() {
        return Err(refused(
            "NoSuchSandbox",
            format!("no Sandbox {id} runs on this machine"),
        ));
    }
    let state = match crate::sandbox::registry::sandbox_live(args) {
        Ok(state) => state,
        Err(gone) if gone.detail.get_bool("containerGone") => {
            let container = gone.detail.get_text("container").unwrap_or("").to_owned();
            reclaim(args, &container, gone.detail.get_bool("containerStopped"))?;
            return Err(gone);
        }
        Err(refusal) => return Err(refusal),
    };
    let _guard = crate::sandbox::files::guard(&state.directory)?;
    let recoverable = crate::workspace::unmount_guard::check_all(&state)?;
    crate::sandbox::files::refresh_except(&state, &recoverable)?;
    // Explicit close can take over an orphaned/retained unmount, but must
    // still refuse every unrelated live Workspace. Stop detaches before
    // killing the daemon; Delete independently checks physical detach.
    let mounted = lock(&state.mounts)?
        .keys()
        .filter(|path| !recoverable.contains(path))
        .cloned()
        .collect::<Vec<_>>();
    let force = args.get_bool("force");
    if !mounted.is_empty() && !force {
        return Err(SdkRefusal {
            code: "MountsActive".to_owned(),
            message: format!(
                "Sandbox {id} still holds {} Workspace(s); commit or unmount them first",
                mounted.len()
            ),
            detail: SdkValue::map([(
                "workspaces",
                SdkValue::List(mounted.into_iter().map(SdkValue::Text).collect()),
            )]),
        });
    }
    let value = crate::sandbox::files::started(&directory)?
        .ok_or_else(|| refused("NoSuchSandbox", format!("Sandbox {id} was closed")))?;
    let mut owner = crate::sandbox::recorded::recorded(args, &id, &value)?
        .reopen()
        .map_err(failed)?;
    owner.stop(STOP_S).map_err(failed)?;
    owner.delete().map_err(failed)?;
    lock(&SANDBOXES)?.remove(&directory);
    std::fs::remove_dir_all(&directory).map_err(failed)?;
    Ok(SdkValue::map([
        ("kind", SdkValue::text("Closed")),
        ("sandbox", SdkValue::text(&id)),
        (
            "unknown",
            SdkValue::List(mounted.into_iter().map(SdkValue::Text).collect()),
        ),
    ]))
}

/// Seconds a closing Sandbox's daemon has to exit before the Engine kills it.
const STOP_S: u16 = 5;

/// Reclaims the Sandbox a call names, whose container (or local daemon) a
/// reopen found gone or stopped (`container`, `stopped`): one unforced
/// Delete of a stopped container, then its folder. Under its folder's
/// guard the record is read again: a Sandbox started again meanwhile has
/// another container and is left alone (`Busy`). A Delete the Engine
/// refuses (the container runs again) removes nothing.
pub(crate) fn reclaim(args: &SdkValue, container: &str, stopped: bool) -> Outcome {
    let id = text(args, "sandbox")?;
    let directory = crate::sandbox::files::sandbox_dir(args)?;
    let _guard = crate::sandbox::files::guard(&directory)?;
    let Some(value) = crate::sandbox::files::started(&directory)? else {
        return Err(refused(
            "NoSuchSandbox",
            format!("no Sandbox {id} runs on this machine"),
        ));
    };
    if text(&value, "container")? != container {
        return Err(refused(
            "Busy",
            format!("Sandbox {id} was started again while it was reclaimed; nothing was removed"),
        ));
    }
    if stopped {
        match crate::sandbox::recorded::recorded(args, id, &value)?
            .stopped()
            .delete()
        {
            Err(failure) if !failure.container_gone() => return Err(failed(failure)),
            _ => {}
        }
    }
    lock(&SANDBOXES)?.remove(&directory);
    std::fs::remove_dir_all(&directory).map_err(failed)?;
    Ok(SdkValue::map([
        ("kind", SdkValue::text("Reclaimed")),
        ("id", SdkValue::text(id)),
    ]))
}
