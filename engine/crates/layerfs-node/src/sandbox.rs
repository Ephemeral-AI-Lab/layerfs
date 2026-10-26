//! Project Sandboxes (#373): numbered containers `<prefix><project>-<n>`,
//! each running a daemon on the Store that Import installed into the project
//! volume. A Sandbox starts only when an agent asks for one ([`create`],
//! #428; there is no placement since 2026-10-23: a mount names its
//! Sandbox). Its records go when it is found gone ([`reclaim`], by the next
//! `create` or `runtime.sandbox.reclaim`) or when an agent closes it
//! ([`close`]).
//!
//! A start that failed before anything reached the Store volume removes the
//! container it made (stop, then delete, each once) and its creation intent,
//! then reports the failure with its daemon's last log lines, read once
//! before the delete. One whose effect is unknown keeps the intent, which
//! refuses later starts until a person resolves it. Nothing is retried on
//! the caller's behalf.
use crate::job::{failed, refused, Outcome};
use crate::registry::{lock, text, SandboxState, SANDBOXES};
use layerfs_api::internal::split;
use layerfs_api::sandbox::values;
use layerfs_api::{
    OperationFailure, SandboxApi, SandboxCause, SandboxCreate, SandboxFailure, SandboxPhase,
};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::daemon_setup::{DaemonLimits, DaemonSetup};
use layerfs_bridge::provision::StoreManifest;
use layerfs_sandbox::backend::docker::{CreatePhase, Docker, SandboxRequest};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Port the daemon listens on inside its container.
const PORT: u16 = 30421;
/// Wait for the daemon's listener after Start.
const STARTUP: Duration = Duration::from_secs(30);
/// Wait for one Engine control response.
const ENGINE: Duration = Duration::from_secs(30);
/// A failed create's report keeps its daemon's last log lines, up to this many bytes.
const LOG_LINES: u16 = 20;
const LOG_BYTES: usize = 2048;

pub(crate) fn operation(failure: Box<OperationFailure>) -> SdkRefusal {
    values::refusal(&failure).unwrap_or_else(|| failed(failure))
}

/// A Sandbox's daemon ends itself after this many seconds with no Workspace
/// and no call (#373 idle close; owner answer, 2026-10-21: 2 minutes). Its
/// container then stops, and the next `sandbox.create` or `runtime.sandbox.reclaim` reclaims it.
const IDLE_EXIT_S: u32 = 120;

/// Workspaces one Sandbox holds at once; a mount into a full one is refused
/// `Capacity`.
pub(crate) const WORKSPACES: u32 = 16;

/// Control connections a Sandbox's daemon admits: [`crate::pool::CONNECTIONS`]
/// for this process and the rest for others (#352 F-017).
pub(crate) const DAEMON_CONNECTIONS: u16 = 8;

fn setup(private: [u8; 32], peer: [u8; 32]) -> DaemonSetup {
    DaemonSetup {
        listen: format!("0.0.0.0:{PORT}"),
        private_key: private,
        control_peer: peer,
        store: crate::project::LOCATOR.into(),
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
            idle_exit_s: IDLE_EXIT_S,
        },
        existing_store: None,
    }
}

/// A container name prefix when the deployment sets none.
const PREFIX: &str = "layerfs-";

/// The container labels of Sandbox `id` of `project`: what it is, whose,
/// and which. Saved in its record, so a reopen expects the same.
pub(crate) fn labels(project: &str, id: &str) -> Vec<(String, String)> {
    [
        ("layerfs.role", "sandbox"),
        ("layerfs.project", project),
        ("layerfs.sandbox", id),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_owned(), value.to_owned()))
    .collect()
}

/// Starts the project's next Sandbox (the lowest free number) on the
/// deployment's image (`deploy.image`). Called under the guard of the
/// project's directory `directory`, so no other start runs meanwhile.
pub(crate) fn start(args: &SdkValue, directory: &Path) -> Result<Arc<SandboxState>, SdkRefusal> {
    let name = text(args, "project")?;
    let deploy = args
        .get("deploy")
        .ok_or_else(|| refused("InvalidInput", "missing `deploy`"))?;
    let project = crate::discovery::project(args)?;
    // Inputs are read first, so a refused request leaves no creation intent.
    let Some(installed) = project.installed.clone() else {
        return Err(refused(
            "NoStore",
            format!("project {name} has no installed Store: its Import did not finish, so import it again"),
        ));
    };
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
    let store_volume = text(deploy, "volume")?.to_owned();
    let executable = PathBuf::from(text(deploy, "daemon")?);
    // A creation whose outcome is unknown is never silently attempted again.
    let intent = directory.join("create.intent");
    if intent.exists() {
        let state = match crate::discovery::load(&intent) {
            Ok(SdkValue::Text(state)) => state,
            _ => "its record is unreadable".to_owned(),
        };
        return Err(refused(
            "Unknown",
            format!(
                "a prior Sandbox creation has an unresolved discovery intent: {state}. Its outcome \
                 is unknown, so it is neither retried nor taken as done. Remove any container it \
                 left (docker ps -a --filter volume={store_volume}), then delete {}",
                intent.display()
            ),
        ));
    }
    // The number is claimed before anything is made; a failed start gives it
    // back (its directory holds no record yet), so the number is used again.
    let (id, saved_directory) = crate::discovery::claim_next(args)?;
    let made = start_claimed(
        args,
        directory,
        &saved_directory,
        &id,
        (&project, installed),
        (docker, image, store_volume, executable),
    );
    if made.is_err() {
        let _ = std::fs::remove_dir(&saved_directory);
    }
    made
}

/// Starts the claimed Sandbox `id`: container, daemon on the installed
/// Store, then its records.
fn start_claimed(
    args: &SdkValue,
    directory: &Path,
    saved_directory: &Path,
    id: &str,
    (project, installed): (&crate::registry::Project, StoreManifest),
    (docker, image, store_volume, executable): (&str, String, String, PathBuf),
) -> Result<Arc<SandboxState>, SdkRefusal> {
    let name = text(args, "project")?;
    let controller = layerfs_bridge::duplex::generate_keypair().map_err(failed)?;
    let daemon = layerfs_bridge::duplex::generate_keypair().map_err(failed)?;
    let mut daemon_setup = setup(daemon.private, controller.public);
    let mut existing = project.manifest.clone();
    existing.daemon_sqlite = installed.daemon_sqlite;
    daemon_setup.existing_store = Some(existing);
    // The container is `<prefix><project>-<n>`: the deployment's prefix
    // (`layerfs-` by default; another LayerFS home on the same Engine sets
    // its own), so `docker ps` says what each one is and whose.
    let prefix = args
        .get("deploy")
        .and_then(|d| d.get_text("containerPrefix"))
        .filter(|prefix| !prefix.is_empty())
        .unwrap_or(PREFIX);
    let container_name = format!("{prefix}{name}-{id}");
    let docker = Docker::new(docker, ENGINE)
        .map_err(failed)?
        .naming(&container_name)
        .map_err(|_| refused("InvalidInput", format!("container name {container_name}")))?;
    // Read only if this process ends before the intent is resolved.
    crate::discovery::save(
        &directory.join("create.intent"),
        SdkValue::text(format!("creation of Sandbox {id} began and never finished")),
    )?;
    let created = SandboxApi::new(docker).create_file(
        SandboxCreate {
            deployment: SandboxRequest {
                image,
                store_volume,
                port: PORT,
                labels: labels(name, id),
            },
            setup: daemon_setup,
            executable,
            controller_private: controller.private,
            daemon_peer: daemon.public,
            startup_wait: STARTUP,
            connect_host: crate::pool::daemon_host(args),
        },
        &project.store,
        &project.manifest,
    );
    let owner = created.map_err(|failure| abandon(directory, id, failure))?;
    let container = owner.runtime.identity().to_string();
    let instance = owner.startup.instance;
    let (runtime, control, identity) = split(owner);
    let dial = crate::pool::Dial {
        endpoint: runtime.endpoint().map_err(failed)?,
        host: crate::pool::daemon_host(args),
        secret: controller.private,
        peer: daemon.public,
        instance,
    };
    let state = Arc::new(SandboxState {
        id: id.to_owned(),
        project: name.to_owned(),
        name: container_name,
        directory: saved_directory.to_owned(),
        runtime,
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
    // plain text beside the record, for people and tools reading daemon logs:
    // docker logs $(cat "$LAYERFS_HOME/<project>/sandboxes/<n>/container")
    crate::discovery::save_text(&state.directory.join("container"), &container)?;
    std::fs::remove_file(directory.join("create.intent")).map_err(failed)?;
    lock(&SANDBOXES)?.insert(state.directory.clone(), state.clone());
    Ok(state)
}

/// Whether a failed create's every effect is known: nothing reached the
/// Store volume, and any container it made is acknowledged by identity.
fn settled(failure: &SandboxFailure) -> bool {
    // A Create the Engine refused (409: the container name is taken) made no container.
    let unknown_container = matches!(
        &failure.cause,
        SandboxCause::Create(create) if create.phase == CreatePhase::Create
            && !matches!(create.transfer.cause, layerfs_sandbox::RuntimeError::Http(409))
    );
    failure.phase != SandboxPhase::Install && failure.installed.is_none() && !unknown_container
}

/// The refusal for a failed create. A settled one reads its container's log
/// tail once, removes the container (stop, then delete, each once) and its
/// intent; otherwise the intent stays, rewritten to say what is left.
fn abandon(directory: &Path, id: &str, mut failure: Box<SandboxFailure>) -> SdkRefusal {
    let mut cause = format!(
        "sandbox create stopped at {:?}: {:?}",
        failure.phase, failure.cause
    );
    let container = failure
        .sandbox
        .as_ref()
        .map(|sandbox| sandbox.identity().to_string());
    drop(failure.control.take());
    let mut log = None;
    let left = if !settled(&failure) {
        Some("its effect on the Store volume or the Engine is unknown".to_owned())
    } else if let Some(sandbox) = failure.sandbox.as_mut() {
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
            let state = format!(
                "creation of Sandbox {id} failed ({cause}); container {}; {left}",
                container.as_deref().unwrap_or("none acknowledged")
            );
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

/// `sandbox.create({ image })`: under the project's guard, reclaims the
/// project's idle-closed Sandboxes, then starts the lowest free number on
/// the call's image (`deploy.image`) and waits for its daemon:
/// `{ sandbox, name, image }`. The only call that starts a Sandbox. Failures
/// are `Failed` or `Unknown`, never retried.
pub(crate) fn create(args: &SdkValue) -> Outcome {
    let project = crate::discovery::project_dir(args)?;
    let _placing = crate::discovery::guard(&project)?;
    crate::workspace::reclaim_gone(args)?;
    let state = start(args, &project)?;
    Ok(SdkValue::map([
        ("sandbox", SdkValue::text(&state.id)),
        ("name", SdkValue::text(&state.name)),
        (
            "image",
            SdkValue::text(state.runtime.request().image.as_str()),
        ),
    ]))
}

/// `sandbox.close(sandbox)`: stops Sandbox `sandbox` now, under the
/// project's guard and its own. Refused `NoSuchSandbox` when it is not
/// running and `MountsActive` while a Workspace is mounted in it (a
/// background process runs in one). Stop, then delete, each attempted once:
/// a failed stop is not followed by a delete and keeps its cause; the
/// records go after the delete.
pub(crate) fn close(args: &SdkValue) -> Outcome {
    let id = text(args, "sandbox")?.to_owned();
    let project = crate::discovery::project_dir(args)?;
    let _placing = crate::discovery::guard(&project)?;
    let directory = crate::discovery::sandbox_dir(args, &id)?;
    let record = directory.join("sandbox.record");
    if !record.exists() {
        return Err(refused(
            "NoSuchSandbox",
            format!("the project has no Sandbox {id}"),
        ));
    }
    let state = crate::registry::sandbox_live(args)?;
    let _guard = crate::discovery::guard(&state.directory)?;
    crate::discovery::refresh(&state)?;
    let mounted = lock(&state.mounts)?.keys().cloned().collect::<Vec<_>>();
    if !mounted.is_empty() {
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
    let value = crate::discovery::load(&record)?;
    let (docker, selector, request) = crate::discovery::runtime(&value)?;
    let mut owner = layerfs_sandbox::backend::docker::Sandbox::reopen(docker, selector, request)
        .map_err(failed)?;
    owner.stop(STOP_S).map_err(failed)?;
    owner.delete().map_err(failed)?;
    lock(&SANDBOXES)?.remove(&directory);
    std::fs::remove_file(record).map_err(failed)?;
    std::fs::remove_dir_all(&directory).map_err(failed)?;
    Ok(SdkValue::map([
        ("kind", SdkValue::text("Closed")),
        ("sandbox", SdkValue::text(&id)),
    ]))
}

/// `backend.daemon.sandbox.get_log({ sandbox, lines })`: the last `lines`
/// lines (1..=2000) of Sandbox `sandbox`'s daemon log, at most
/// [`GET_LOG_BYTES`], read once through the Engine: `{ sandbox, container,
/// text, bytes }`. A stopped container's log is readable until its records
/// are reclaimed. Refused `NoSuchSandbox` with no record or no container.
/// Read-only: it starts, stops and reclaims nothing.
pub(crate) fn get_log(args: &SdkValue) -> Outcome {
    let id = text(args, "sandbox")?.to_owned();
    let lines = match args.get("lines") {
        Some(SdkValue::Int(n)) if (1..=2000).contains(n) => *n as u16,
        _ => return Err(refused("InvalidInput", "lines is a count from 1 to 2000")),
    };
    let directory = crate::discovery::sandbox_dir(args, &id)?;
    let record = directory.join("sandbox.record");
    let missing = || refused("NoSuchSandbox", format!("the project has no Sandbox {id}"));
    if !record.exists() {
        return Err(missing());
    }
    let value = {
        let _guard = crate::discovery::guard(&directory)?;
        if !record.exists() {
            return Err(missing());
        }
        crate::discovery::load(&record)?
    };
    let container = text(&value, "container")?.to_owned();
    let (docker, selector, request) = crate::discovery::runtime(&value)?;
    let sandbox = layerfs_sandbox::backend::docker::Sandbox::stopped(docker, selector, request);
    let bytes = match sandbox.log_tail(lines, GET_LOG_BYTES) {
        Ok(bytes) => bytes,
        Err(failure) if failure.container_gone() => {
            return Err(crate::discovery::gone(&id, &container, false))
        }
        Err(failure) => return Err(failed(failure)),
    };
    Ok(SdkValue::map([
        ("sandbox", SdkValue::text(&id)),
        ("container", SdkValue::text(&container)),
        ("text", SdkValue::text(String::from_utf8_lossy(&bytes))),
        ("bytes", SdkValue::Int(bytes.len() as i64)),
    ]))
}

/// What one `get_log` keeps: the final bytes of the tail.
const GET_LOG_BYTES: usize = 64 * 1024;

/// Seconds a closing Sandbox's daemon has to exit before the Engine kills it.
const STOP_S: u16 = 5;

/// Reclaims Sandbox `id`, whose container a reopen found gone or stopped
/// (`container`, `stopped`): one unforced Delete of a stopped container, then
/// its records, so its number is free again. Called under the project's
/// guard; the Sandbox's own guard is taken too. The record is read again
/// under them: a Sandbox started again under the number meanwhile has
/// another container and is left alone (`Busy`). A Delete the Engine refuses
/// (the container runs again) removes nothing.
pub(crate) fn reclaim(args: &SdkValue, id: &str, container: &str, stopped: bool) -> Outcome {
    let directory = crate::discovery::sandbox_dir(args, id)?;
    let _guard = crate::discovery::guard(&directory)?;
    let record = directory.join("sandbox.record");
    if !record.exists() {
        return Err(refused(
            "NoSuchSandbox",
            format!("the project has no Sandbox {id}"),
        ));
    }
    let value = crate::discovery::load(&record)?;
    if text(&value, "container")? != container {
        return Err(refused(
            "Busy",
            format!("Sandbox {id} was started again while it was reclaimed; nothing was removed"),
        ));
    }
    if stopped {
        let (docker, selector, request) = crate::discovery::runtime(&value)?;
        match layerfs_sandbox::backend::docker::Sandbox::stopped(docker, selector, request).delete()
        {
            Err(failure) if !failure.container_gone() => return Err(failed(failure)),
            _ => {}
        }
    }
    lock(&SANDBOXES)?.remove(&directory);
    std::fs::remove_file(record).map_err(failed)?;
    std::fs::remove_dir_all(&directory).map_err(failed)?;
    Ok(SdkValue::map([
        ("kind", SdkValue::text("Reclaimed")),
        ("id", SdkValue::text(id)),
    ]))
}
