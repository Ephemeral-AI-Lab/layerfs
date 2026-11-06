//! Private, permission-authorized project and Sandbox records on the side
//! that runs a Sandbox (layout 3, #536 §5, §6): each Sandbox's folder
//! `<root>/sandbox/<name>/` holds its record (`record.json`, with what
//! reopens it under `daemon`), its daemon keys (`control.json`) and one
//! record per Workspace (`workspace/<id>.json`). No mutation replay.
use crate::sandbox::recorded::{gone, recorded, Recorded};
use crate::sandbox::registry::{Mount, Project, SandboxState};
#[cfg(target_os = "linux")]
use crate::sandbox::runtime::NAMESPACES;
use crate::sandbox::runtime::{LOCAL, NATIVE};
use crate::{internal::workspace, BoundWorkspace, MountedWorkspace};
use layerfs_bridge::{
    control::{Reply, SdkRefusal, SdkValue},
    provision::StoreManifest,
};
use layerfs_home::helpers;
use layerfs_home::home::{paths, read_record, write_entity};
use layerfs_home::outcome::{failed, refused};
use layerfs_isolation::LocalRecord;
#[cfg(target_os = "linux")]
use layerfs_isolation::SandboxRecord;
use layerfs_isolation::{CommandIdentity, Sandbox};
use layerfs_persistence::{SealedStore, SqlitePersistenceProfile};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
type Result<T> = std::result::Result<T, SdkRefusal>;

pub(crate) fn home(args: &SdkValue) -> Result<PathBuf> {
    let home = args
        .get_text("home")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LAYERFS_HOME").map(PathBuf::from))
        .ok_or_else(|| refused("InvalidInput", "LAYERFS_HOME is required"))?;
    if !home.is_absolute() {
        return Err(refused("InvalidInput", "LAYERFS_HOME must be absolute"));
    }
    directory(&home)?;
    Ok(home)
}
/// `path` is this user's and nobody else may read or change it. A folder
/// may let others pass through by name (o+x), as the folders above a
/// namespaces Sandbox's local root must ([`layerfs_home::home::traversable`]).
pub(crate) fn private(path: &Path, dir: bool) -> Result<()> {
    let meta = fs::symlink_metadata(path).map_err(failed)?;
    let others = if dir { 0o076 } else { 0o077 };
    if meta.file_type().is_symlink()
        || meta.is_dir() != dir
        || (!dir && !meta.is_file())
        || meta.uid() != layerfs_home::home::flock::euid()
        || meta.mode() & others != 0
    {
        return Err(refused(
            "Unauthorized",
            format!("{} must be owned by this user and private", path.display()),
        ));
    }
    Ok(())
}
pub(crate) fn directory(path: &Path) -> Result<()> {
    match fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(failed(e)),
    }
    private(path, true)
}
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
/// A project's home entry (#506 Store-first): `<home>/<project>/` for a
/// call that names no `store`, and for the one project of a Store made by
/// an Init of its own (the Store has its project's name and no Store
/// record); `<home>/.stores/<store>/projects/<project>/` for every other
/// project of a Store.
pub(crate) fn project_dir(args: &SdkValue) -> Result<PathBuf> {
    let path = project_entry(args)?;
    private(&path, true)?;
    Ok(path)
}

/// [`project_dir`]'s path, not checked to exist.
pub(crate) fn project_entry(args: &SdkValue) -> Result<PathBuf> {
    let name = helpers::text(args, "project")?;
    layerfs_history::HistoryName::new(name).map_err(failed)?;
    let home = home(args)?;
    // 2.0 (#533 step 4): every project is named with its Store
    let store = args.get_text("store").unwrap_or(name);
    crate::store::identity::store_dir(&home, store)?;
    Ok(layerfs_home::home::paths::project_dir(&home, store, name))
}
/// Calls take turns on one directory: in this process before its OS lock,
/// and between processes on the lock itself. The wait is readiness, bounded,
/// and nothing has been attempted before it ends.
// Unmount exchanges run outside this guard; one shared readiness deadline
// covers the in-process turn and the cross-process discovery lock.
const IN_PROCESS_WAIT: Duration = Duration::from_secs(60);
/// Between two looks at another process's lock.
const LOCK_PAUSE: Duration = Duration::from_millis(2);
static HELD: Mutex<std::collections::BTreeSet<PathBuf>> =
    Mutex::new(std::collections::BTreeSet::new());
static FREED: std::sync::Condvar = std::sync::Condvar::new();

/// One discovery mutation's ownership of a directory: this process's turn and
/// the OS lock. Dropping it releases both.
pub(crate) struct Guard {
    file: File,
    directory: PathBuf,
}
impl Drop for Guard {
    fn drop(&mut self) {
        // The lock belongs to the open file description, which a child forked
        // meanwhile shares until its exec: closing only this descriptor would
        // leave it held (F-053). Unlock explicitly; the close follows.
        layerfs_home::home::flock::unlock(&self.file);
        let mut held = HELD.lock().unwrap_or_else(|error| error.into_inner());
        held.remove(&self.directory);
        FREED.notify_all();
    }
}

/// Another call of this process, or another process holding the OS lock,
/// is waited for until the bounded wait ends; then the call is refused Busy,
/// which is not retried.
pub(crate) fn guard(directory: &Path) -> Result<Guard> {
    let deadline = std::time::Instant::now() + IN_PROCESS_WAIT;
    let mut held = helpers::lock(&HELD)?;
    while held.contains(directory) {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(refused(
                "Busy",
                "another call of this process owns this discovery mutation",
            ));
        }
        held = FREED
            .wait_timeout(held, remaining)
            .map_err(|_| refused("Failed", "discovery turn poisoned"))?
            .0;
    }
    held.insert(directory.to_owned());
    drop(held);
    match os_lock(directory, deadline) {
        Ok(file) => Ok(Guard {
            file,
            directory: directory.to_owned(),
        }),
        Err(refusal) => {
            let mut held = HELD.lock().unwrap_or_else(|error| error.into_inner());
            held.remove(directory);
            FREED.notify_all();
            Err(refusal)
        }
    }
}
/// The file of a directory's discovery turn. It is not `lock`: on `native`
/// a Sandbox's directory is the home's `sandbox/<name>/`, where `lock` is
/// the Sandbox lock (#536 §8), which is short and never held across a
/// daemon call, while this turn is held across a start, a mount or a close.
pub(crate) const TURN: &str = "serve.lock";

/// The OS lock, waited for until `deadline`; it is released even if its
/// process exits.
fn os_lock(directory: &Path, deadline: std::time::Instant) -> Result<File> {
    let path = directory.join(TURN);
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(no_follow())
        .open(&path)
        .map_err(failed)?;
    private(&path, false)?;
    while !layerfs_home::home::flock::try_lock(&file, true).map_err(failed)? {
        if std::time::Instant::now() >= deadline {
            return Err(refused(
                "Busy",
                "another process owns this discovery mutation",
            ));
        }
        std::thread::sleep(LOCK_PAUSE);
    }
    Ok(file)
}
#[cfg(target_os = "macos")]
fn no_follow() -> i32 {
    0x100
}
#[cfg(target_os = "linux")]
fn no_follow() -> i32 {
    0x20000
}

pub(crate) fn save(path: &Path, value: SdkValue) -> Result<()> {
    let bytes = layerfs_bridge::control::Answer {
        id: 1,
        reply: Reply::Sdk(value),
    }
    .encode()
    .map_err(failed)?;
    let temp = path.with_extension(format!(
        "{}-{}",
        std::process::id(),
        layerfs_bridge::sdk_values::hex(&helpers::random()?[..8])
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(no_follow())
        .open(&temp)
        .map_err(failed)?;
    file.write_all(&bytes).map_err(failed)?;
    drop(file);
    fs::rename(temp, path).map_err(failed)
}
pub(crate) fn load(path: &Path) -> Result<SdkValue> {
    private(path, false)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(no_follow())
        .open(path)
        .map_err(failed)?;
    if file.metadata().map_err(failed)?.len() > 65535 {
        return Err(refused("InvalidInput", "discovery record too large"));
    }
    let mut bytes = Vec::new();
    file.take(65536).read_to_end(&mut bytes).map_err(failed)?;
    match layerfs_bridge::control::Answer::decode(&bytes)
        .map_err(failed)?
        .reply
    {
        Reply::Sdk(value) => Ok(value),
        _ => Err(refused("InvalidInput", "discovery record kind")),
    }
}
pub(crate) fn save_project(path: &Path, project: &Project) -> Result<()> {
    // the record is read, changed and written whole under the lock of the
    // home it is in: `<home>/store/<store>/project/<name>/`
    let home = path
        .ancestors()
        .nth(4)
        .filter(|_| path.parent().and_then(Path::file_name) == Some("project".as_ref()));
    let _lock = home.map(layerfs_home::home::lock::take).transpose()?;
    let old = layerfs_home::home::read_record(&path.join("record.json"))?.unwrap_or_default();
    let mut record = SdkValue::map([
        (
            "manifest",
            SdkValue::Bytes(project.manifest.encode().map_err(failed)?),
        ),
        (
            "installed",
            project
                .installed
                .as_ref()
                .map(|m| m.encode().map(SdkValue::Bytes))
                .transpose()
                .map_err(failed)?
                .unwrap_or_default(),
        ),
    ]);
    // the record's other fields (its name and Store) are kept
    if let SdkValue::Map(fields) = old {
        for (name, value) in fields {
            if name != "manifest" && name != "installed" {
                record = layerfs_home::home::with_field(record, &name, value);
            }
        }
    }
    layerfs_home::home::write_entity(&path.join("record.json"), &record)
}
pub(crate) fn project(args: &SdkValue) -> Result<Project> {
    let path = project_dir(args)?;
    let value = layerfs_home::home::read_record(&path.join("record.json"))?
        .ok_or_else(|| refused("NoSuchProject", "no project record"))?;
    let decode = |name| match value.get(name) {
        Some(SdkValue::Bytes(bytes)) => StoreManifest::decode(bytes).map_err(failed),
        _ => Err(refused("NoSuchProject", "no complete project manifest")),
    };
    let manifest = decode("manifest")?;
    if manifest.profile != layerfs_bridge::provision::StoreProfile::Disposable {
        return Err(refused("InvalidInput", "Disposable required"));
    }
    let installed = if matches!(value.get("installed"), Some(SdkValue::Bytes(_))) {
        Some(decode("installed")?)
    } else {
        None
    };
    Ok(Project {
        store: SealedStore {
            path: path.join("sealed.sqlite"),
            profile: SqlitePersistenceProfile::Disposable,
            sqlite_version: manifest.host_sqlite.clone(),
            bytes: manifest.bytes,
        },
        manifest,
        installed,
    })
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
