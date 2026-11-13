//! Private, permission-authorized Sandbox records on the side that runs a
//! Sandbox (layout 3, #536 §5, §6; project records are `layerfs-store`
//! `project/discovery.rs`): each Sandbox's folder
//! `<root>/sandbox/<name>/` holds its record (`record.json`, with what
//! reopens it under `daemon`), its daemon keys (`control.json`) and one
//! record per Workspace (`workspace/<id>.json`). No mutation replay.
use crate::host::Host;
use crate::record::{DaemonRecord, Local, SandboxRecord, WorkspaceRecord};
use crate::registry::{Mount, SandboxState};
use crate::reopen::{gone, recorded};
#[cfg(target_os = "linux")]
use crate::runtime::NAMESPACES;
use crate::runtime::{LOCAL, NATIVE};
use crate::{internal::workspace, BoundWorkspace, MountedWorkspace};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::helpers;
use layerfs_home::home::paths;
use layerfs_home::outcome::{failed, refused};
use layerfs_sandbox_backend::LocalRecord;
#[cfg(target_os = "linux")]
use layerfs_sandbox_backend::SandboxRecord as BackendRecord;
use layerfs_sandbox_backend::{CommandIdentity, Sandbox};
use layerfs_store::project::discovery::{home_of, private};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
type Result<T> = std::result::Result<T, SdkRefusal>;

/// This process's effective uid and gid: a native Sandbox's command
/// identity (its commands run as this user).
pub fn user() -> (u32, u32) {
    (
        layerfs_home::home::flock::euid(),
        layerfs_home::home::flock::egid(),
    )
}
/// The folder of the Sandbox a call names (`sandbox`, its name):
/// `<root>/sandbox/<name>/`, in the home on `native` and for a Sandbox
/// whose daemon the home reaches directly (#535 G4), and under a worker's
/// root on every other machine.
pub fn sandbox_dir(host: &Host) -> Result<PathBuf> {
    let name = layerfs_home::names::SandboxName::parse(host.sandbox()?)?;
    Ok(paths::sandbox_dir(&home_of(Some(&host.home))?, &name))
}
/// What reopens the Sandbox recorded in `directory` (its record's
/// `daemon`), or `None` before its start recorded one.
pub fn started(directory: &Path) -> Result<Option<DaemonRecord>> {
    let record = SandboxRecord::read_in(directory, "")?;
    Ok(record.and_then(|record| record.daemon().cloned()))
}

/// Records started Sandbox `state` in its folder `directory`: its daemon
/// keys in `control.json`, then what reopens it in its record (`daemon`,
/// with its backend, route and runtime id, and `platform`, which `create`
/// checked before any effect, #529), the record's other fields kept.
pub fn save_sandbox(
    directory: &Path,
    state: &SandboxState,
    (host, platform): (&Host, &str),
    secret: [u8; 32],
    peer: [u8; 32],
    instance: [u8; 32],
) -> Result<()> {
    let platform = platform.to_owned();
    let runtime = state.runtime()?;
    let daemon = match runtime {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        Sandbox::HostProcess(runtime) => {
            let record = runtime
                .local_record()
                .ok_or_else(|| refused("Failed", "local Sandbox has no daemon process"))?;
            local_fields(host, state, platform, &record, NATIVE, LOCAL)?
        }
        #[cfg(target_os = "linux")]
        Sandbox::Namespaces(runtime) => {
            let Some(BackendRecord::Namespaces(record)) = runtime.record() else {
                return Err(refused("Failed", "namespace Sandbox has no init process"));
            };
            let image = record.image().as_str().to_owned();
            local_fields(host, state, platform, record.local(), NAMESPACES, &image)?
        }
    };
    // the keys first: a record with `daemon` is a started Sandbox
    layerfs_home::secrets::save_daemon_keys(
        directory,
        &SdkValue::map([
            (
                "endpoint",
                SdkValue::opt_text(runtime.endpoint().ok().map(|e| e.to_string())),
            ),
            ("secret", SdkValue::Bytes(secret.to_vec())),
            ("peer", SdkValue::Bytes(peer.to_vec())),
            ("instance", SdkValue::Bytes(instance.to_vec())),
        ]),
    )?;
    // on `native` this is the home's record, which keeps its other fields
    // (its `op` among them): changed under the Sandbox lock (#536 §8)
    let _sandbox = crate::record::lock_in(directory, &state.id)?;
    let record = SandboxRecord::read_in(directory, &state.id)?
        .unwrap_or_else(|| SandboxRecord::from_record(&state.id, SdkValue::Null));
    let store = &host.store;
    let facts = [
        crate::runtime::kind(runtime).to_owned(),
        route(runtime).to_owned(),
        runtime.identity().to_string(),
    ];
    let record = record.with_start(format!("{store}/{}", state.project), facts, daemon);
    record.write_in(directory)
}
/// The kernel route a running Sandbox's Workspaces use: loopback NFS for a
/// native (macOS) one, FUSE for every other.
pub fn route(runtime: &Sandbox) -> &'static str {
    match runtime {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        Sandbox::HostProcess(_) => "nfs",
        #[cfg(target_os = "linux")]
        Sandbox::Namespaces(_) => "fuse",
    }
}
/// The fields a local Sandbox record keeps: its directory, its daemon or init
/// process identity, its listener port and the image it runs, under `runtime`
/// `native` (whose image is `local`) or `namespaces` (whose image is the
/// digest its root is an overlay of).
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn local_fields(
    host: &Host,
    state: &SandboxState,
    platform: String,
    record: &LocalRecord,
    runtime: &'static str,
    image: &str,
) -> Result<DaemonRecord> {
    let boot = record
        .boot
        .ok_or_else(|| refused("Failed", "local Sandbox daemon has no boot id"))?;
    let directory = crate::launch::local::directory(host.deploy(), &state.id, None)?;
    Ok(DaemonRecord::local(Local {
        platform,
        name: state.name.clone(),
        container: state.runtime()?.identity().to_string(),
        runtime,
        uid: state.identity.uid,
        gid: state.identity.gid,
        directory: directory.display().to_string(),
        process: (record.pid, record.start_time, format!("{boot:032x}")),
        port: record.port,
        image: image.to_owned(),
        labels: record.labels.clone(),
    }))
}

/// Reopens the Sandbox a call names from its folder: its record's `daemon`
/// and its keys, its runtime observed running once, then a control
/// connection and its Workspace records.
pub fn reopen(host: &Host) -> Result<Arc<SandboxState>> {
    let id = host.sandbox()?;
    let directory = sandbox_dir(host)?;
    let Some(value) = started(&directory)? else {
        return Err(refused(
            "NoSuchSandbox",
            format!("no Sandbox {id} runs on this machine"),
        ));
    };
    private(&directory, true)?;
    let keys = layerfs_home::secrets::daemon_keys(&directory)?
        .ok_or_else(|| refused("NoSuchSandbox", format!("Sandbox {id} has no daemon keys")))?;
    let project = &host.project;
    // a Sandbox in a machine whose daemon's port is forwarded here (#535
    // G4): only its forward is checked; its runtime is the machine's
    let (runtime, endpoint) = match crate::direct::forward_of(&value) {
        Some(address) => {
            crate::direct::check(&home_of(Some(&host.home))?, id, &value)?;
            (None, address)
        }
        None => {
            let runtime = observed(host, id, &value)?;
            let endpoint = runtime.endpoint().map_err(failed)?;
            (Some(runtime), endpoint)
        }
    };
    let dial = crate::daemon::pool::Dial {
        endpoint,
        secret: layerfs_home::secrets::key(&keys, "secret")?,
        peer: layerfs_home::secrets::key(&keys, "peer")?,
        instance: layerfs_home::secrets::key(&keys, "instance")?,
    };
    let forwarded = runtime.is_none();
    let control = dial.open_waiting().map_err(|refusal| match forwarded {
        true => crate::direct::unanswered(id, &value, refusal),
        false => refusal,
    })?;
    let state = Arc::new(SandboxState {
        id: id.to_owned(),
        project: project.to_owned(),
        name: value.name().unwrap_or_default().to_owned(),
        directory,
        runtime,
        identity: recorded_identity(&value),
        controls: crate::daemon::pool::Controls::new(control, dial),
        mounts: Default::default(),
    });
    refresh(&state)?;
    Ok(state)
}
/// The runtime a Sandbox record's `daemon` names, observed running once;
/// its daemon listens on this kernel's loopback.
fn observed(host: &Host, id: &str, value: &DaemonRecord) -> Result<Box<Sandbox>> {
    let container = helpers::required(value.container(), "container")?;
    recorded(host, id, value)?.reopen().map_err(|failure| {
        if failure.container_stopped() {
            gone(id, container, true)
        } else {
            failed(failure)
        }
    })
}
/// The command identity a Sandbox record names: a native Sandbox's is the
/// user who started it (`uid`, `gid`); every other runs as 501:20.
fn recorded_identity(value: &DaemonRecord) -> CommandIdentity {
    let id = |n: Option<i64>| n.and_then(|n| u32::try_from(n).ok());
    match (id(value.uid()), id(value.gid())) {
        (Some(uid), Some(gid)) => CommandIdentity { uid, gid },
        _ => CommandIdentity { uid: 501, gid: 20 },
    }
}
/// Refresh shared executor/mount metadata without repeating an operation.
pub fn refresh(state: &SandboxState) -> Result<()> {
    refresh_except(state, &[])
}
/// [`refresh`], waiting at most `wait` in total for all status probes. An
/// unmount uses a short bound so a refusing daemon returns its exact refusal
/// instead of blocking behind the folder guard (#540).
pub fn refresh_within(state: &SandboxState, wait: std::time::Duration) -> Result<()> {
    refresh_except_within(state, &[], wait)
}
/// Explicit Sandbox close may dispose retained unmounts without asking their
/// daemon for a Ready receipt that no longer exists. Other records still
/// undergo the ordinary refresh and MountsActive admission.
pub fn refresh_except(state: &SandboxState, retained: &[String]) -> Result<()> {
    refresh_except_within(state, retained, crate::daemon::pool::ADMISSION_WAIT)
}
/// [`refresh_except`], waiting at most `wait` in total for all Workspace
/// status probes. One deadline is shared by every Workspace, so N records
/// cannot multiply the wait (#540). An explicit close uses a short wait so its
/// `op` intent is not held for the whole admission wait while the daemon
/// refuses connections.
pub fn refresh_except_within(
    state: &SandboxState,
    retained: &[String],
    wait: std::time::Duration,
) -> Result<()> {
    let deadline = std::time::Instant::now() + wait;
    let mut mounts = std::collections::BTreeMap::new();
    for (_, value) in workspaces(&state.directory)? {
        let path = helpers::required(value.path(), "path")?;
        if retained.iter().any(|kept| kept == path) {
            continue;
        }
        let token = token_of(&value)?;
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        let status = match workspace(&mut *crate::daemon::trace::control_within(
            &state.controls,
            remaining,
        )?)
        .status(token)
        {
            Ok(status) => status,
            // Unmounted by the daemon; its owner removes the record next.
            Err(failure) if absent(&failure) => continue,
            Err(failure) => return Err(failed(failure)),
        };
        let ready = status
            .kernel
            .and_then(|k| k.ready)
            .ok_or_else(|| refused("NotMounted", "saved Workspace is no longer mounted"))?;
        if ready.directory != path {
            return Err(refused("StaleSandbox", "mount path mismatch"));
        }
        mounts.insert(
            path.to_owned(),
            Mount {
                workspace: MountedWorkspace {
                    bound: BoundWorkspace {
                        token,
                        binding: status.binding,
                    },
                    ready,
                },
                branch: helpers::required(value.branch(), "branch")?.to_owned(),
            },
        );
    }
    *helpers::lock(&state.mounts)? = mounts;
    Ok(())
}
/// The Workspace records in Sandbox folder `directory`
/// (`workspace/<id>.json`), each with its file; one its owner removed after
/// the listing is left out (#352 F-017).
pub fn workspaces(directory: &Path) -> Result<Vec<(PathBuf, WorkspaceRecord)>> {
    let mut found = Vec::new();
    let entries = match fs::read_dir(directory.join("workspace")) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(found),
        other => other.map_err(failed)?,
    };
    for item in entries {
        let path = item.map_err(failed)?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or(".");
        if name.starts_with('.') || !name.ends_with(".json") {
            continue;
        }
        if let Some(value) = WorkspaceRecord::read(&path)? {
            found.push((path, value));
        }
    }
    Ok(found)
}
/// A Workspace record's token: its id (64 hex digits) and namespace.
fn token_of(value: &WorkspaceRecord) -> Result<layerfs_bridge::control::WorkspaceToken> {
    let id = helpers::required(value.id(), "id")?;
    let bytes: Vec<u8> = (0..id.len())
        .step_by(2)
        .map(|i| {
            id.get(i..i + 2)
                .and_then(|b| u8::from_str_radix(b, 16).ok())
        })
        .collect::<Option<_>>()
        .filter(|b: &Vec<u8>| b.len() == 32)
        .ok_or_else(|| refused("InvalidInput", "Workspace record id"))?;
    Ok(layerfs_bridge::control::WorkspaceToken {
        workspace: layerfs_history::WorkspaceId::from_slice(&bytes).map_err(failed)?,
        namespace: value
            .namespace()
            .ok_or_else(|| refused("InvalidInput", "namespace"))?,
    })
}
fn absent(failure: &crate::OperationFailure) -> bool {
    matches!(&failure.cause, crate::OperationCause::Remote(refusal)
        if refusal.code == layerfs_bridge::control::ControlCode::Missing)
}
/// The record of the Workspace `token` names: `workspace/<id>.json`.
pub fn workspace_file(
    state: &SandboxState,
    token: layerfs_bridge::control::WorkspaceToken,
) -> PathBuf {
    state.directory.join("workspace").join(format!(
        "{}.json",
        layerfs_bridge::sdk_values::hex(&token.workspace.to_bytes())
    ))
}
/// Records the Workspace `mount` mounted of `branch`, with its `name` if
/// it has one: `{ v, id, name?, branch, path, namespace, created }`,
/// after the mount succeeded, under the Sandbox lock (#536 §8).
pub fn save_workspace(
    state: &SandboxState,
    mount: &MountedWorkspace,
    branch: &str,
    name: Option<&str>,
) -> Result<()> {
    let token = mount.bound.token;
    let id = layerfs_bridge::sdk_values::hex(&token.workspace.to_bytes());
    let record =
        WorkspaceRecord::mounted(id, name, branch, &mount.ready.directory, token.namespace);
    let _sandbox = crate::record::lock_in(&state.directory, &state.id)?;
    record.write(&workspace_file(state, token))
}
