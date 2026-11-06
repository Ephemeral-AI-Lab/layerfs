//! Private, permission-authorized Sandbox records on the side that runs a
//! Sandbox (layout 3, #536 §5, §6; project records are `layerfs-store`
//! `project/discovery.rs`): each Sandbox's folder
//! `<root>/sandbox/<name>/` holds its record (`record.json`, with what
//! reopens it under `daemon`), its daemon keys (`control.json`) and one
//! record per Workspace (`workspace/<id>.json`). No mutation replay.
use crate::sandbox::recorded::{gone, recorded, Recorded};
use crate::sandbox::registry::{Mount, SandboxState};
#[cfg(target_os = "linux")]
use crate::sandbox::runtime::NAMESPACES;
use crate::sandbox::runtime::{LOCAL, NATIVE};
use crate::{internal::workspace, BoundWorkspace, MountedWorkspace};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::helpers;
use layerfs_home::home::{paths, read_record, write_entity};
use layerfs_home::outcome::{failed, refused};
use layerfs_isolation::LocalRecord;
#[cfg(target_os = "linux")]
use layerfs_isolation::SandboxRecord;
use layerfs_isolation::{CommandIdentity, Sandbox};
use layerfs_store::project::discovery::{home, private};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
type Result<T> = std::result::Result<T, SdkRefusal>;

/// This process's effective uid and gid: a native Sandbox's command
/// identity (its commands run as this user).
pub(crate) fn user() -> (u32, u32) {
    (
        layerfs_home::home::flock::euid(),
        layerfs_home::home::flock::egid(),
    )
}
/// The folder of the Sandbox a call names (`sandbox`, its name):
/// `<root>/sandbox/<name>/`, in the home on `native` and for a Sandbox
/// whose daemon the home reaches directly (#535 G4), and under a worker's
/// root on every other machine.
pub(crate) fn sandbox_dir(args: &SdkValue) -> Result<PathBuf> {
    let name = layerfs_home::names::SandboxName::parse(helpers::text(args, "sandbox")?)?;
    Ok(paths::sandbox_dir(&home(args)?, &name))
}
/// What reopens the Sandbox recorded in `directory` (its record's
/// `daemon`), or `None` before its start recorded one.
pub(crate) fn started(directory: &Path) -> Result<Option<SdkValue>> {
    let record = read_record(&directory.join("record.json"))?;
    Ok(record.and_then(|record| match record.get("daemon") {
        Some(daemon @ SdkValue::Map(_)) => Some(daemon.clone()),
        _ => None,
    }))
}
/// The labels a record keeps, as the value map its reader expects.
fn label_values(labels: &[(String, String)]) -> SdkValue {
    SdkValue::List(
        (labels.iter())
            .map(|(key, value)| {
                SdkValue::map([
                    ("key", SdkValue::text(key)),
                    ("value", SdkValue::text(value)),
                ])
            })
            .collect(),
    )
}

/// Records started Sandbox `state` in its folder `directory`: its daemon
/// keys in `control.json`, then what reopens it in its record (`daemon`,
/// with its backend, route and runtime id), the record's other fields kept.
pub(crate) fn save_sandbox(
    directory: &Path,
    state: &SandboxState,
    args: &SdkValue,
    secret: [u8; 32],
    peer: [u8; 32],
    instance: [u8; 32],
) -> Result<()> {
    let deploy = args
        .get("deploy")
        .ok_or_else(|| refused("InvalidInput", "deploy"))?;
    // the platform the call named, checked by `create` before any effect (#529)
    let platform =
        crate::sandbox::runtime::platform(args, crate::sandbox::runtime::backend(deploy)?)?;
    let runtime = state.runtime()?;
    let mut fields = vec![
        ("platform", SdkValue::text(platform)),
        ("name", SdkValue::text(&state.name)),
        ("container", SdkValue::text(runtime.identity().to_string())),
    ];
    match runtime {
        Sandbox::Docker(runtime) => {
            let record = runtime.request();
            fields.extend([
                ("docker", SdkValue::text(helpers::text(deploy, "docker")?)),
                ("image", SdkValue::text(&record.image)),
                ("volume", SdkValue::opt_text(record.store_volume.clone())),
                ("labels", label_values(&record.labels)),
            ]);
        }
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        Sandbox::HostProcess(runtime) => {
            let record = runtime
                .local_record()
                .ok_or_else(|| refused("Failed", "local Sandbox has no daemon process"))?;
            fields.extend(local_fields(args, state, &record, NATIVE, LOCAL)?);
        }
        #[cfg(target_os = "linux")]
        Sandbox::Namespaces(runtime) => {
            let Some(SandboxRecord::Namespaces(record)) = runtime.record() else {
                return Err(refused("Failed", "namespace Sandbox has no init process"));
            };
            let image = record.image().as_str().to_owned();
            fields.extend(local_fields(
                args,
                state,
                record.local(),
                NAMESPACES,
                &image,
            )?);
        }
    }
    let daemon = SdkValue::Map(
        (fields.into_iter())
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    );
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
    let _sandbox = crate::sandbox::record::lock_in(directory, &state.id)?;
    let mut record = read_record(&directory.join("record.json"))?.unwrap_or_default();
    let store = args.get_text("store").unwrap_or(&state.project);
    for (field, value) in [
        ("name", SdkValue::text(&state.id)),
        (
            "project",
            SdkValue::text(format!("{store}/{}", state.project)),
        ),
        (
            "backend",
            SdkValue::text(crate::sandbox::runtime::kind(runtime)),
        ),
        ("route", SdkValue::text(route(runtime))),
        ("runtime_id", SdkValue::text(runtime.identity().to_string())),
        ("daemon", daemon),
    ] {
        record = layerfs_home::home::with_field(record, field, value);
    }
    write_entity(&directory.join("record.json"), &record)
}
/// The kernel route a running Sandbox's Workspaces use: loopback NFS for a
/// native (macOS) one, FUSE for every other.
pub(crate) fn route(runtime: &Sandbox) -> &'static str {
    match runtime {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        Sandbox::HostProcess(_) => "nfs",
        _ => "fuse",
    }
}
/// The fields a local Sandbox record keeps: its directory, its daemon or init
/// process identity, its listener port and the image it runs, under `runtime`
/// `native` (whose image is `local`) or `namespaces` (whose image is the
/// digest its root is an overlay of).
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn local_fields(
    args: &SdkValue,
    state: &SandboxState,
    record: &LocalRecord,
    runtime: &'static str,
    image: &str,
) -> Result<Vec<(&'static str, SdkValue)>> {
    let boot = record
        .boot
        .ok_or_else(|| refused("Failed", "local Sandbox daemon has no boot id"))?;
    Ok(vec![
        ("runtime", SdkValue::text(runtime)),
        ("uid", SdkValue::Int(i64::from(state.identity.uid))),
        ("gid", SdkValue::Int(i64::from(state.identity.gid))),
        (
            "directory",
            SdkValue::text(
                crate::sandbox::launch::local::directory(args, &state.id, None)?
                    .display()
                    .to_string(),
            ),
        ),
        ("pid", SdkValue::Int(i64::from(record.pid))),
        ("start", SdkValue::text(record.start_time.to_string())),
        ("boot", SdkValue::text(format!("{boot:032x}"))),
        ("port", SdkValue::Int(i64::from(record.port))),
        ("image", SdkValue::text(image)),
        ("labels", label_values(&record.labels)),
    ])
}

/// Reopens the Sandbox a call names from its folder: its record's `daemon`
/// and its keys, its runtime observed running once, then a control
/// connection and its Workspace records.
pub(crate) fn reopen(args: &SdkValue) -> Result<Arc<SandboxState>> {
    let id = helpers::text(args, "sandbox")?;
    let directory = sandbox_dir(args)?;
    let Some(value) = started(&directory)? else {
        return Err(refused(
            "NoSuchSandbox",
            format!("no Sandbox {id} runs on this machine"),
        ));
    };
    private(&directory, true)?;
    let keys = layerfs_home::secrets::daemon_keys(&directory)?
        .ok_or_else(|| refused("NoSuchSandbox", format!("Sandbox {id} has no daemon keys")))?;
    let project = helpers::text(args, "project")?;
    // a Sandbox in a machine whose daemon's port is forwarded here (#535
    // G4): only its forward is checked; its runtime is the machine's
    let (runtime, endpoint, host) = match crate::sandbox::direct::forward_of(&value) {
        Some(address) => {
            crate::sandbox::direct::check(&home(args)?, id, &value)?;
            (None, address, None)
        }
        None => {
            let (runtime, host) = observed(args, id, &value)?;
            let endpoint = runtime.endpoint().map_err(failed)?;
            (Some(runtime), endpoint, host)
        }
    };
    let dial = crate::daemon::pool::Dial {
        endpoint,
        host,
        secret: layerfs_home::secrets::key(&keys, "secret")?,
        peer: layerfs_home::secrets::key(&keys, "peer")?,
        instance: layerfs_home::secrets::key(&keys, "instance")?,
    };
    let forwarded = runtime.is_none();
    let control = dial.open_waiting().map_err(|refusal| match forwarded {
        true => crate::sandbox::direct::unanswered(id, &value, refusal),
        false => refusal,
    })?;
    let state = Arc::new(SandboxState {
        id: id.to_owned(),
        project: project.to_owned(),
        name: value.get_text("name").unwrap_or_default().to_owned(),
        directory,
        runtime,
        identity: recorded_identity(&value),
        controls: crate::daemon::pool::Controls::new(control, dial),
        mounts: Default::default(),
    });
    refresh(&state)?;
    Ok(state)
}
/// The runtime a Sandbox record's `daemon` names, observed running once,
/// and the host this process dials its daemon at.
fn observed(args: &SdkValue, id: &str, value: &SdkValue) -> Result<(Box<Sandbox>, Option<String>)> {
    let container = helpers::text(value, "container")?;
    let recorded = recorded(args, id, value)?;
    // a local daemon listens on this kernel's loopback; no other host reaches it
    let host = match recorded {
        Recorded::Docker(..) => crate::daemon::pool::daemon_host(args),
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        Recorded::Local { .. } => None,
        #[cfg(target_os = "linux")]
        Recorded::Namespaces(..) => None,
    };
    let runtime = recorded.reopen().map_err(|failure| {
        if failure.container_gone() || failure.container_stopped() {
            gone(id, container, failure.container_stopped())
        } else {
            failed(failure)
        }
    })?;
    Ok((runtime, host))
}
/// The command identity a Sandbox record names: a native Sandbox's is the
/// user who started it (`uid`, `gid`); every other runs as 501:20.
fn recorded_identity(value: &SdkValue) -> CommandIdentity {
    let id = |key: &str| value.get_int(key).and_then(|n| u32::try_from(n).ok());
    match (id("uid"), id("gid")) {
        (Some(uid), Some(gid)) => CommandIdentity { uid, gid },
        _ => CommandIdentity { uid: 501, gid: 20 },
    }
}
/// Refresh shared executor/mount metadata without repeating an operation.
pub(crate) fn refresh(state: &SandboxState) -> Result<()> {
    refresh_except(state, &[])
}
/// Explicit Sandbox close may dispose retained unmounts without asking their
/// daemon for a Ready receipt that no longer exists. Other records still
/// undergo the ordinary refresh and MountsActive admission.
pub(crate) fn refresh_except(state: &SandboxState, retained: &[String]) -> Result<()> {
    let mut mounts = std::collections::BTreeMap::new();
    for (_, value) in workspaces(&state.directory)? {
        let path = helpers::text(&value, "path")?;
        if retained.iter().any(|kept| kept == path) {
            continue;
        }
        let token = token_of(&value)?;
        let status =
            match workspace(&mut *crate::daemon::trace::control(&state.controls)?).status(token) {
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
                branch: helpers::text(&value, "branch")?.to_owned(),
            },
        );
    }
    *helpers::lock(&state.mounts)? = mounts;
    Ok(())
}
/// The Workspace records in Sandbox folder `directory`
/// (`workspace/<id>.json`), each with its file; one its owner removed after
/// the listing is left out (#352 F-017).
pub(crate) fn workspaces(directory: &Path) -> Result<Vec<(PathBuf, SdkValue)>> {
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
        if let Some(value) = read_record(&path)? {
            found.push((path, value));
        }
    }
    Ok(found)
}
/// A Workspace record's token: its id (64 hex digits) and namespace.
fn token_of(value: &SdkValue) -> Result<layerfs_bridge::control::WorkspaceToken> {
    let id = helpers::text(value, "id")?;
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
            .get_int("namespace")
            .ok_or_else(|| refused("InvalidInput", "namespace"))?,
    })
}
fn absent(failure: &crate::OperationFailure) -> bool {
    matches!(&failure.cause, crate::OperationCause::Remote(refusal)
        if refusal.code == layerfs_bridge::control::ControlCode::Missing)
}
/// The record of the Workspace `token` names: `workspace/<id>.json`.
pub(crate) fn workspace_file(
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
pub(crate) fn save_workspace(
    state: &SandboxState,
    mount: &MountedWorkspace,
    branch: &str,
    name: Option<&str>,
) -> Result<()> {
    let token = mount.bound.token;
    let mut fields = vec![(
        "id",
        SdkValue::text(layerfs_bridge::sdk_values::hex(&token.workspace.to_bytes())),
    )];
    if let Some(name) = name {
        fields.push(("name", SdkValue::text(name)));
    }
    fields.extend([
        ("branch", SdkValue::text(branch)),
        ("path", SdkValue::text(&mount.ready.directory)),
        ("namespace", SdkValue::Int(token.namespace)),
        ("created", SdkValue::Int(layerfs_home::home::now_ms())),
    ]);
    let _sandbox = crate::sandbox::record::lock_in(&state.directory, &state.id)?;
    write_entity(
        &workspace_file(state, token),
        &SdkValue::Map(
            (fields.into_iter())
                .map(|(n, v)| (n.to_owned(), v))
                .collect(),
        ),
    )
}
