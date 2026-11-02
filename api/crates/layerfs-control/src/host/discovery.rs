//! Private, permission-authorized project and Sandbox discovery. No mutation replay.
#[cfg(target_os = "linux")]
use crate::host::runtime::NAMESPACES;
use crate::host::runtime::{LOCAL, NATIVE};
use crate::host::{
    outcome::{failed, refused},
    registry::{self, Mount, Process, Project, SandboxState},
};
use crate::{internal::workspace, BoundWorkspace, MountedWorkspace};
use layerfs_bridge::{
    control::{Reply, SdkRefusal, SdkValue},
    provision::StoreManifest,
};
use layerfs_persistence::{SealedStore, SqlitePersistenceProfile};
use layerfs_sandbox::docker::{Docker, DockerProvider, SandboxRequest};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use layerfs_sandbox::host_process;
use layerfs_sandbox::local::LocalProcess;
use layerfs_sandbox::{CommandIdentity, ContainerId, Sandbox, WireFailure};
use layerfs_sandbox::{LocalRecord, SandboxRecord};
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
/// namespaces Sandbox's local root must ([`crate::home::traversable`]).
pub(crate) fn private(path: &Path, dir: bool) -> Result<()> {
    let meta = fs::symlink_metadata(path).map_err(failed)?;
    let others = if dir { 0o076 } else { 0o077 };
    if meta.file_type().is_symlink()
        || meta.is_dir() != dir
        || (!dir && !meta.is_file())
        || meta.uid() != crate::host::flock::euid()
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
    (crate::host::flock::euid(), crate::host::flock::egid())
}
/// Where a project's Sandboxes live: `LAYERFS_HOME/<project>/sandboxes/<n>/`
/// (#373), numbered from 1 within the project.
fn sandboxes(args: &SdkValue) -> Result<PathBuf> {
    let root = project_dir(args)?.join("sandboxes");
    directory(&root)?;
    Ok(root)
}
/// A Sandbox number as text: 1 to 9999, no leading zero.
fn number(id: &str) -> Option<u32> {
    let n = id.parse::<u32>().ok()?;
    ((1..=9999).contains(&n) && n.to_string() == id).then_some(n)
}
/// The directory of the project's Sandbox `id`.
pub(crate) fn sandbox_dir(args: &SdkValue, id: &str) -> Result<PathBuf> {
    if number(id).is_none() {
        return Err(refused(
            "InvalidInput",
            format!("a Sandbox is a number from 1 to 9999, not {id}"),
        ));
    }
    Ok(sandboxes(args)?.join(id))
}
/// The project's Sandbox numbers that have a directory, lowest first.
pub(crate) fn sandbox_ids(args: &SdkValue) -> Result<Vec<String>> {
    // a project of a PostgreSQL Store has an entry once it had a Sandbox
    if !project_entry(args)?.exists() && crate::host::stores::postgresql(args)? {
        return Ok(Vec::new());
    }
    let mut ids = Vec::new();
    for item in fs::read_dir(sandboxes(args)?).map_err(failed)? {
        let name = item.map_err(failed)?.file_name();
        if let Some(n) = name.to_str().and_then(number) {
            ids.push(n);
        }
    }
    ids.sort_unstable();
    Ok(ids.into_iter().map(|n| n.to_string()).collect())
}
/// Claims the lowest free Sandbox number: its directory, made once. Called
/// under the project's guard, so no other claim runs meanwhile.
pub(crate) fn claim_next(args: &SdkValue) -> Result<(String, PathBuf)> {
    let root = sandboxes(args)?;
    let n = (1..=9999u32)
        .find(|n| !root.join(n.to_string()).exists())
        .ok_or_else(|| refused("Capacity", "the project has 9999 Sandboxes"))?;
    let path = root.join(n.to_string());
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&path)
        .map_err(failed)?;
    private(&path, true)?;
    Ok((n.to_string(), path))
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
    let name = registry::text(args, "project")?;
    layerfs_history::HistoryName::new(name).map_err(failed)?;
    let home = home(args)?;
    // 2.0 (#533 step 4): every project is named with its Store
    let store = args.get_text("store").unwrap_or(name);
    crate::host::stores::store_dir(&home, store)?;
    Ok(crate::home::paths::project_dir(&home, store, name))
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
        crate::host::flock::unlock(&self.file);
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
    let mut held = registry::lock(&HELD)?;
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
/// The OS lock, waited for until `deadline`; it is released even if its
/// process exits.
fn os_lock(directory: &Path, deadline: std::time::Instant) -> Result<File> {
    let path = directory.join("ownership.lock");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(no_follow())
        .open(&path)
        .map_err(failed)?;
    private(&path, false)?;
    while !crate::host::flock::try_lock(&file, true).map_err(failed)? {
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
        layerfs_bridge::sdk_values::hex(&registry::random()?[..8])
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
/// A small private text file, written once (0600, not a symlink).
pub(crate) fn save_text(path: &Path, text: &str) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(no_follow())
        .open(path)
        .map_err(failed)?;
    file.write_all(format!("{text}\n").as_bytes())
        .map_err(failed)
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
fn bytes<const N: usize>(value: &SdkValue, key: &str) -> Result<[u8; N]> {
    match value.get(key) {
        Some(SdkValue::Bytes(bytes)) => bytes.as_slice().try_into().map_err(failed),
        _ => Err(refused("InvalidInput", format!("discovery {key}"))),
    }
}
pub(crate) fn save_project(path: &Path, project: &Project) -> Result<()> {
    // the record is read, changed and written whole under the lock of the
    // home it is in: `<home>/store/<store>/project/<name>/`
    let home = path
        .ancestors()
        .nth(4)
        .filter(|_| path.parent().and_then(Path::file_name) == Some("project".as_ref()));
    let _lock = home.map(crate::home::lock::take).transpose()?;
    let old = crate::home::read_record(&path.join("record.json"))?.unwrap_or_default();
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
                record = crate::home::with_field(record, &name, value);
            }
        }
    }
    crate::home::write_record(&path.join("record.json"), &record)
}
pub(crate) fn project(args: &SdkValue) -> Result<Project> {
    let path = project_dir(args)?;
    let value = crate::home::read_record(&path.join("record.json"))?
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
    let platform = crate::host::runtime::platform(args, crate::host::runtime::backend(deploy)?)?;
    let mut fields = vec![
        ("id", SdkValue::text(&state.id)),
        ("platform", SdkValue::text(platform)),
        ("project", SdkValue::text(&state.project)),
        ("name", SdkValue::text(&state.name)),
        (
            "container",
            SdkValue::text(state.runtime.identity().to_string()),
        ),
    ];
    match state.runtime.as_ref() {
        Sandbox::Docker(runtime) => {
            let record = runtime.request();
            fields.extend([
                ("docker", SdkValue::text(registry::text(deploy, "docker")?)),
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
    fields.extend([
        ("secret", SdkValue::Bytes(secret.to_vec())),
        ("peer", SdkValue::Bytes(peer.to_vec())),
        ("instance", SdkValue::Bytes(instance.to_vec())),
    ]);
    save(
        &directory.join("sandbox.record"),
        SdkValue::Map(
            (fields.into_iter())
                .map(|(name, value)| (name.to_owned(), value))
                .collect(),
        ),
    )
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
                crate::host::local::directory(args, &state.id, None)?
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

/// A Sandbox whose container no longer runs it: the Engine no longer has the
/// container, or has it stopped (it closed when idle, or Docker restarted),
/// which lost its daemon and every mount in it. Only its records, and a
/// stopped container, hold its number; the next mount reclaims them.
pub(crate) fn gone(id: &str, container: &str, stopped: bool) -> SdkRefusal {
    let state = if stopped { "stopped" } else { "gone" };
    SdkRefusal {
        code: "NoSuchSandbox".to_owned(),
        message: format!(
            "the container of Sandbox {id} is {state}, with every Workspace that was in it"
        ),
        detail: SdkValue::map([
            ("sandbox", SdkValue::text(id)),
            ("container", SdkValue::text(container)),
            ("containerGone", SdkValue::Bool(true)),
            ("containerStopped", SdkValue::Bool(stopped)),
        ]),
    }
}
/// The container of a refusal from [`gone`], and whether the Engine still has it stopped.
pub(crate) fn gone_container(refusal: &SdkRefusal) -> Option<(&str, bool)> {
    match refusal.detail.get("containerGone") {
        Some(SdkValue::Bool(true)) if refusal.code == "NoSuchSandbox" => Some((
            refusal.detail.get_text("container")?,
            matches!(
                refusal.detail.get("containerStopped"),
                Some(SdkValue::Bool(true))
            ),
        )),
        _ => None,
    }
}
/// The labels a Sandbox record keeps; a record written before #373 has none.
fn labels(value: &SdkValue) -> Result<Vec<(String, String)>> {
    match value.get("labels") {
        Some(SdkValue::List(items)) => items
            .iter()
            .map(|item| {
                Ok((
                    registry::text(item, "key")?.to_owned(),
                    registry::text(item, "value")?.to_owned(),
                ))
            })
            .collect(),
        _ => Ok(Vec::new()),
    }
}
/// The Engine, container and deployment a Docker Sandbox record names.
fn runtime(value: &SdkValue) -> Result<(DockerProvider, ContainerId, SandboxRecord)> {
    let docker =
        Docker::new(registry::text(value, "docker")?, Duration::from_secs(30)).map_err(failed)?;
    let request = SandboxRequest {
        image: registry::text(value, "image")?.to_owned(),
        // a PostgreSQL Store's Sandbox mounts no volume (#506)
        store_volume: value.get_text("volume").map(str::to_owned),
        port: 30421,
        labels: labels(value)?,
    };
    let container = ContainerId::parse(registry::text(value, "container")?).map_err(failed)?;
    let provider = DockerProvider::new(docker);
    Ok((provider, container, SandboxRecord::Docker(request.into())))
}
/// What a Sandbox record names: a Docker container, a local daemon process
/// (`runtime` `native`) with its directory and port, or a namespace Sandbox
/// (`runtime` `namespaces`) with its own runtime, record and directory.
pub(crate) enum Recorded {
    Docker(DockerProvider, ContainerId, SandboxRecord),
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    Local {
        directory: PathBuf,
        id: ContainerId,
        process: LocalProcess,
        port: u16,
        labels: Vec<(String, String)>,
    },
    #[cfg(target_os = "linux")]
    Namespaces(layerfs_sandbox::Runtime, ContainerId, SandboxRecord),
}
/// Reads Sandbox `id`'s record's runtime.
/// The record alone decides (#529): a caller's deployment backend is not
/// consulted, so every call on a Sandbox reopens it by its own runtime.
pub(crate) fn recorded(args: &SdkValue, id: &str, value: &SdkValue) -> Result<Recorded> {
    #[cfg(target_os = "linux")]
    if value.get_text("runtime") == Some(NAMESPACES) {
        return namespaces_record(args, id, value);
    }
    if value.get_text("runtime") == Some(NATIVE) {
        return local_record(args, id, value);
    }
    if let Some(other) = value.get_text("runtime") {
        // the former `local` runtime (an earlier, unmerged build), or one
        // this host does not run: nothing it names is used or deleted
        return Err(refused(
            "InvalidInput",
            format!(
                "Sandbox {id} is a {other} Sandbox, which this host never reopens, so the record \
                 is neither used nor deleted. Remove what it left by hand, then delete \
                 LAYERFS_HOME/{}/sandboxes/{id}/sandbox.record",
                registry::text(args, "project")?
            ),
        ));
    }
    let (docker, container, request) = runtime(value)?;
    Ok(Recorded::Docker(docker, container, request))
}
/// A local record is used only where this deployment puts that Sandbox:
/// its directory must be exactly `<LAYERFS_LOCAL_DIR>/<prefix><project>-<id>`
/// and named so, so a damaged, edited or stale record (another
/// LAYERFS_LOCAL_DIR or prefix) never points a delete elsewhere. A record
/// without `boot` was written before boot ids were kept: its daemon is
/// matched by id and start time as then, and never signalled.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn local_record(args: &SdkValue, id: &str, value: &SdkValue) -> Result<Recorded> {
    let (directory, record) = local_record_of(args, id, value)?;
    Ok(Recorded::Local {
        directory,
        id: ContainerId::parse(registry::text(value, "container")?).map_err(failed)?,
        process: LocalProcess {
            pid: record.pid,
            start: record.start_time,
            boot: record.boot,
        },
        port: record.port,
        labels: record.labels,
    })
}
/// A record's directory and physical facts, checked against where this
/// deployment puts that Sandbox: under its LAYERFS_LOCAL_DIR when it sets
/// one, else (#529: the record decides, so a caller of another backend has
/// none) beside the record's own directory, which must still carry the
/// name this deployment gives Sandbox `id` and be owned by this record
/// ([`crate::host::local::owned`]).
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn local_record_of(args: &SdkValue, id: &str, value: &SdkValue) -> Result<(PathBuf, LocalRecord)> {
    let directory = PathBuf::from(registry::text(value, "directory")?);
    let expected = crate::host::local::directory(args, id, directory.parent())?;
    // the Sandbox's own directory name, exactly as this deployment names it
    let name = registry::text(value, "name")?;
    if directory != expected || expected.file_name().and_then(|n| n.to_str()) != Some(name) {
        return Err(refused(
            "InvalidInput",
            format!(
                "Sandbox {id}'s local record names {} (as {name}), but this deployment puts it at \
                 {}: the record is damaged or from another LAYERFS_LOCAL_DIR or prefix, so it is \
                 neither used nor deleted. Remove what it left by hand, then delete \
                 LAYERFS_HOME/{}/sandboxes/{id}/sandbox.record",
                directory.display(),
                expected.display(),
                registry::text(args, "project")?
            ),
        ));
    }
    if (args
        .get("deploy")
        .and_then(|deploy| deploy.get_text("localDir")))
    .is_none()
    {
        crate::host::local::owned(args, id, &directory)?;
    }
    let process = recorded_process(value)?;
    let port = value
        .get_int("port")
        .ok_or_else(|| refused("InvalidInput", "local Sandbox record port"))?;
    let record = LocalRecord {
        name: name.to_owned(),
        pid: process.pid,
        start_time: process.start,
        boot: process.boot,
        port: u16::try_from(port).map_err(failed)?,
        labels: labels(value)?,
    };
    Ok((directory, record))
}
/// The daemon (native) or init (namespaces) process a local record names:
/// its id, start time and boot. A record from before boot ids has none.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn recorded_process(value: &SdkValue) -> Result<LocalProcess> {
    let pid = value
        .get_int("pid")
        .ok_or_else(|| refused("InvalidInput", "local Sandbox record pid"))?;
    let boot = match value.get_text("boot") {
        None => None,
        Some(hex) => Some(
            (hex.len() == 32)
                .then(|| u128::from_str_radix(hex, 16).ok())
                .flatten()
                .ok_or_else(|| refused("InvalidInput", "local Sandbox record boot"))?,
        ),
    };
    Ok(LocalProcess {
        pid: u32::try_from(pid).map_err(failed)?,
        start: registry::text(value, "start")?
            .parse::<u64>()
            .map_err(failed)?,
        boot,
    })
}
/// Whether the process a native or namespaces record names still runs (id,
/// start time, boot), from this kernel alone: no deployment is consulted.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) fn local_running(value: &SdkValue) -> Result<bool> {
    Ok(recorded_process(value)?.alive())
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(crate) fn local_running(_: &SdkValue) -> Result<bool> {
    Err(refused(
        "Unsupported",
        "a local process is read on Linux or macOS",
    ))
}
/// A namespaces record is used only where its directory checks put that
/// Sandbox ([`local_record_of`]), by any deployment (#529: the record
/// decides).
#[cfg(target_os = "linux")]
fn namespaces_record(args: &SdkValue, id: &str, value: &SdkValue) -> Result<Recorded> {
    let (directory, local) = local_record_of(args, id, value)?;
    let image =
        layerfs_sandbox::Image::parse(registry::text(value, "image")?).map_err(|error| {
            refused(
                "InvalidInput",
                format!(
                    "Sandbox {id} is a namespaces Sandbox whose record names no image digest \
                 ({error}); it cannot be given a root, so it is neither reopened nor deleted"
                ),
            )
        })?;
    let record = layerfs_sandbox::NamespaceRecord { local, image };
    let root = (directory.parent())
        .ok_or_else(|| refused("InvalidInput", "namespaces Sandbox directory"))?;
    let provider = layerfs_sandbox::namespaces::NamespaceProvider::new(root).map_err(|error| {
        refused(
            "Unsupported",
            format!(
                "namespaces Sandbox at {} cannot run on this host: {error}",
                directory.display()
            ),
        )
    })?;
    Ok(Recorded::Namespaces(
        layerfs_sandbox::Runtime::Namespaces(provider),
        ContainerId::parse(registry::text(value, "container")?).map_err(failed)?,
        SandboxRecord::Namespaces(record),
    ))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn local_record(_: &SdkValue, id: &str, _: &SdkValue) -> Result<Recorded> {
    Err(refused(
        "Unsupported",
        format!(
            "Sandbox {id} is a native Sandbox (LAYERFS_SANDBOX=native), which needs Linux or macOS"
        ),
    ))
}
impl Recorded {
    /// The runtime, observed running once (it starts nothing).
    pub(crate) fn reopen(self) -> std::result::Result<Box<Sandbox>, Box<WireFailure>> {
        match self {
            Self::Docker(provider, id, record) => provider
                .reopen(id, &record)
                .map(|s| Box::new(Sandbox::Docker(s))),
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::Local {
                directory,
                id,
                process,
                port,
                labels,
            } => host_process::Sandbox::reopen(directory, id, process, port, labels)
                .map(|s| Box::new(Sandbox::HostProcess(s))),
            #[cfg(target_os = "linux")]
            Self::Namespaces(runtime, id, record) => runtime.reopen(id, &record).map(Box::new),
        }
    }
    /// The runtime found stopped: only its one Delete is admitted.
    pub(crate) fn stopped(self) -> Box<Sandbox> {
        match self {
            Self::Docker(provider, id, record) => {
                let instance = provider
                    .reopen_stopped(id, &record)
                    .map_err(failed)
                    .expect("validated record");
                Box::new(Sandbox::Docker(instance))
            }
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::Local {
                directory,
                id,
                process,
                port,
                labels,
            } => Box::new(Sandbox::HostProcess(host_process::Sandbox::stopped(
                directory, id, process, port, labels,
            ))),
            #[cfg(target_os = "linux")]
            Self::Namespaces(runtime, id, record) => Box::new(
                runtime
                    .reopen_stopped(id, &record)
                    .map_err(failed)
                    .expect("validated record"),
            ),
        }
    }
}
pub(crate) fn reopen(args: &SdkValue) -> Result<Arc<SandboxState>> {
    let id = registry::text(args, "sandbox")?;
    let directory = sandbox_dir(args, id)?;
    if !directory.join("sandbox.record").exists() {
        return Err(refused(
            "NoSuchSandbox",
            format!("the project has no Sandbox {id}"),
        ));
    }
    private(&directory, true)?;
    let value = load(&directory.join("sandbox.record"))?;
    if registry::text(&value, "id")? != id {
        return Err(refused("Unauthorized", "Sandbox record mismatch"));
    }
    let project = registry::text(&value, "project")?;
    let container = registry::text(&value, "container")?;
    let recorded = recorded(args, id, &value)?;
    // a local daemon listens on this kernel's loopback; no other host reaches it
    let host = match recorded {
        Recorded::Docker(..) => crate::host::pool::daemon_host(args),
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
    let dial = crate::host::pool::Dial {
        endpoint: runtime.endpoint().map_err(failed)?,
        host,
        secret: bytes(&value, "secret")?,
        peer: bytes(&value, "peer")?,
        instance: bytes(&value, "instance")?,
    };
    let control = dial.open_waiting()?;
    let state = Arc::new(SandboxState {
        id: id.to_owned(),
        project: project.to_owned(),
        // a record from before #373 has no name
        name: value.get_text("name").unwrap_or_default().to_owned(),
        directory,
        runtime,
        identity: recorded_identity(&value),
        controls: crate::host::pool::Controls::new(control, dial),
        mounts: Default::default(),
        processes: Default::default(),
    });
    refresh(&state)?;
    Ok(state)
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
    let mut processes = std::collections::BTreeMap::new();
    for item in fs::read_dir(&state.directory).map_err(failed)? {
        let item = item.map_err(failed)?;
        let name = item.file_name().to_string_lossy().into_owned();
        if name.starts_with("workspace-") && name.ends_with(".record") {
            let Some(value) = present(&item.path())? else {
                continue;
            };
            let path = registry::text(&value, "path")?;
            if retained.iter().any(|kept| kept == path) {
                continue;
            }
            let token = layerfs_bridge::control::WorkspaceToken {
                workspace: layerfs_history::WorkspaceId::from_slice(&bytes::<32>(
                    &value,
                    "workspace",
                )?)
                .map_err(failed)?,
                namespace: value
                    .get_int("namespace")
                    .ok_or_else(|| refused("InvalidInput", "namespace"))?,
            };
            let status = match workspace(&mut *crate::host::trace::control(&state.controls)?)
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
                    branch: registry::text(&value, "branch")?.to_owned(),
                },
            );
        } else if name.starts_with("process-") && name.ends_with(".record") {
            let Some(v) = present(&item.path())? else {
                continue;
            };
            processes.insert(
                registry::text(&v, "id")?.to_owned(),
                Process {
                    path: registry::text(&v, "path")?.to_owned(),
                    command: registry::text(&v, "command")?.to_owned(),
                    pid: v
                        .get_int("pid")
                        .ok_or_else(|| refused("InvalidInput", "pid"))?
                        as u32,
                    start: registry::text(&v, "start")?.to_owned(),
                    stopped: v.get_bool("stopped"),
                },
            );
        }
    }
    *registry::lock(&state.mounts)? = mounts;
    *registry::lock(&state.processes)? = processes;
    Ok(())
}
/// A listed record, or `None` when its owner removed it after the listing:
/// concurrent calls of this process unmount and refresh at once (#352 F-017).
fn present(path: &Path) -> Result<Option<SdkValue>> {
    match load(path) {
        Ok(value) => Ok(Some(value)),
        Err(_) if !path.exists() => Ok(None),
        Err(refusal) => Err(refusal),
    }
}
fn absent(failure: &crate::OperationFailure) -> bool {
    matches!(&failure.cause, crate::OperationCause::Remote(refusal)
        if refusal.code == layerfs_bridge::control::ControlCode::Missing)
}
pub(crate) fn workspace_file(
    state: &SandboxState,
    token: layerfs_bridge::control::WorkspaceToken,
) -> PathBuf {
    state.directory.join(format!(
        "workspace-{}.record",
        layerfs_bridge::sdk_values::hex(&token.workspace.to_bytes())
    ))
}
pub(crate) fn save_workspace(
    state: &SandboxState,
    mount: &MountedWorkspace,
    branch: &str,
) -> Result<()> {
    save(
        &workspace_file(state, mount.bound.token),
        SdkValue::map([
            ("path", SdkValue::text(&mount.ready.directory)),
            ("branch", SdkValue::text(branch)),
            (
                "workspace",
                SdkValue::Bytes(mount.bound.token.workspace.to_bytes().to_vec()),
            ),
            ("namespace", SdkValue::Int(mount.bound.token.namespace)),
        ]),
    )
}
pub(crate) fn save_process(state: &SandboxState, id: &str, p: &Process) -> Result<()> {
    save(
        &state.directory.join(format!("process-{id}.record")),
        SdkValue::map([
            ("id", SdkValue::text(id)),
            ("path", SdkValue::text(&p.path)),
            ("command", SdkValue::text(&p.command)),
            ("pid", SdkValue::Int(p.pid as i64)),
            ("start", SdkValue::text(&p.start)),
            ("stopped", SdkValue::Bool(p.stopped)),
        ]),
    )
}
