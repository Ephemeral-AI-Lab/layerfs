//! Private, permission-authorized project and Sandbox discovery. No mutation replay.
use crate::runtime::Runtime;
use crate::{
    job::{failed, refused},
    registry::{self, Mount, Process, Project, SandboxState},
};
use layerfs_api::{internal::workspace, BoundWorkspace, MountedWorkspace};
use layerfs_bridge::{
    control::{Reply, SdkRefusal, SdkValue},
    provision::StoreManifest,
};
use layerfs_persistence::{SealedStore, SqlitePersistenceProfile};
#[cfg(any(target_os = "linux", target_os = "macos"))]
use layerfs_sandbox::backend::local::{self, LocalProcess};
use layerfs_sandbox::{
    backend::docker::{Docker, Sandbox, SandboxRequest},
    CommandIdentity, ContainerId, WireFailure,
};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::{
        fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
        io::AsRawFd,
    },
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
extern "C" {
    fn geteuid() -> u32;
    fn getegid() -> u32;
    fn flock(fd: i32, operation: i32) -> i32;
}
const LOCK_EX: i32 = 2;
const LOCK_NB: i32 = 4;
const LOCK_UN: i32 = 8;
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
pub(crate) fn private(path: &Path, dir: bool) -> Result<()> {
    let meta = fs::symlink_metadata(path).map_err(failed)?;
    if meta.file_type().is_symlink()
        || meta.is_dir() != dir
        || (!dir && !meta.is_file())
        || meta.uid() != unsafe { geteuid() }
        || meta.mode() & 0o077 != 0
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
/// Whether this process runs as root, which the local runtime needs: its
/// daemon mounts FUSE and its commands run as another uid/gid.
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) fn root() -> bool {
    unsafe { geteuid() == 0 }
}
/// This process's effective uid and gid: a native Sandbox's command
/// identity (its commands run as this user).
pub(crate) fn user() -> (u32, u32) {
    unsafe { (geteuid(), getegid()) }
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
pub(crate) fn project_dir(args: &SdkValue) -> Result<PathBuf> {
    let name = registry::text(args, "project")?;
    layerfs_history::HistoryName::new(name).map_err(failed)?;
    let path = home(args)?.join(name);
    private(&path, true)?;
    Ok(path)
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
        unsafe { flock(self.file.as_raw_fd(), LOCK_UN) };
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
    while unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) } != 0 {
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
    save(
        &path.join("project.record"),
        SdkValue::map([
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
        ]),
    )
}
pub(crate) fn project(args: &SdkValue) -> Result<Project> {
    let path = project_dir(args)?;
    let value = load(&path.join("project.record"))?;
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
    let labels = |labels: &[(String, String)]| {
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
    };
    let mut fields = vec![
        ("id", SdkValue::text(&state.id)),
        ("project", SdkValue::text(&state.project)),
        ("name", SdkValue::text(&state.name)),
        (
            "container",
            SdkValue::text(state.runtime.identity().to_string()),
        ),
    ];
    match &state.runtime {
        Runtime::Docker(runtime) => fields.extend([
            ("docker", SdkValue::text(registry::text(deploy, "docker")?)),
            ("image", SdkValue::text(&runtime.request().image)),
            ("volume", SdkValue::text(&runtime.request().store_volume)),
            ("labels", labels(&runtime.request().labels)),
        ]),
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        Runtime::Local(runtime) => {
            let process = runtime
                .process()
                .ok_or_else(|| refused("Failed", "local Sandbox has no daemon process"))?;
            let boot = process
                .boot
                .ok_or_else(|| refused("Failed", "local Sandbox daemon has no boot id"))?;
            fields.extend([
                (
                    "runtime",
                    SdkValue::text(if crate::runtime::native(deploy) {
                        crate::runtime::NATIVE
                    } else {
                        crate::runtime::LOCAL
                    }),
                ),
                ("uid", SdkValue::Int(i64::from(state.identity.uid))),
                ("gid", SdkValue::Int(i64::from(state.identity.gid))),
                (
                    "directory",
                    SdkValue::text(runtime.directory().display().to_string()),
                ),
                ("pid", SdkValue::Int(i64::from(process.pid))),
                ("start", SdkValue::text(process.start.to_string())),
                ("boot", SdkValue::text(format!("{boot:032x}"))),
                ("port", SdkValue::Int(i64::from(runtime.port()))),
                ("image", SdkValue::text(crate::runtime::LOCAL)),
                ("labels", labels(runtime.labels())),
            ]);
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
fn runtime(value: &SdkValue) -> Result<(Docker, ContainerId, SandboxRequest)> {
    let docker =
        Docker::new(registry::text(value, "docker")?, Duration::from_secs(30)).map_err(failed)?;
    let request = SandboxRequest {
        image: registry::text(value, "image")?.to_owned(),
        store_volume: registry::text(value, "volume")?.to_owned(),
        port: 30421,
        labels: labels(value)?,
    };
    let container = ContainerId::parse(registry::text(value, "container")?).map_err(failed)?;
    Ok((docker, container, request))
}
/// What a Sandbox record names: a Docker container, or (`runtime` `local`)
/// a local daemon process, its directory and port.
pub(crate) enum Recorded {
    Docker(Docker, ContainerId, SandboxRequest),
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    Local {
        directory: PathBuf,
        id: ContainerId,
        process: LocalProcess,
        port: u16,
        labels: Vec<(String, String)>,
    },
}
/// Reads Sandbox `id`'s record's runtime.
pub(crate) fn recorded(args: &SdkValue, id: &str, value: &SdkValue) -> Result<Recorded> {
    if !matches!(
        value.get_text("runtime"),
        Some(crate::runtime::LOCAL | crate::runtime::NATIVE)
    ) {
        let (docker, container, request) = runtime(value)?;
        return Ok(Recorded::Docker(docker, container, request));
    }
    local_record(args, id, value)
}
/// A local record is used only where this deployment puts that Sandbox:
/// its directory must be exactly `<LAYERFS_LOCAL_DIR>/<prefix><project>-<id>`
/// and named so, so a damaged, edited or stale record (another
/// LAYERFS_LOCAL_DIR or prefix) never points a delete elsewhere. A record
/// without `boot` was written before boot ids were kept: its daemon is
/// matched by id and start time as then, and never signalled.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn local_record(args: &SdkValue, id: &str, value: &SdkValue) -> Result<Recorded> {
    let directory = PathBuf::from(registry::text(value, "directory")?);
    let expected = crate::local::directory(args, id)?;
    if directory != expected
        || expected.file_name().and_then(|n| n.to_str()) != Some(registry::text(value, "name")?)
    {
        return Err(refused(
            "InvalidInput",
            format!(
                "Sandbox {id}'s local record names {} (as {}), but this deployment puts it at \
                 {}: the record is damaged or from another LAYERFS_LOCAL_DIR or prefix, so it is \
                 neither used nor deleted. Remove what it left by hand, then delete \
                 LAYERFS_HOME/{}/sandboxes/{id}/sandbox.record",
                directory.display(),
                value.get_text("name").unwrap_or_default(),
                expected.display(),
                registry::text(args, "project")?
            ),
        ));
    }
    // with no placement (#428), a mount names its Sandbox: local and native
    // Sandboxes are still never mixed, so a record is used only by a
    // deployment of its own runtime
    let wanted = match args.get("deploy").map(crate::runtime::native) {
        Some(true) => crate::runtime::NATIVE,
        _ => crate::runtime::LOCAL,
    };
    if value.get_text("runtime") != Some(wanted) {
        return Err(refused(
            "InvalidInput",
            format!(
                "Sandbox {id} is a {} Sandbox, but this deployment is {wanted}                  (LAYERFS_SANDBOX): local and native Sandboxes are never mixed",
                value.get_text("runtime").unwrap_or_default()
            ),
        ));
    }
    let number = |key: &str| {
        value
            .get_int(key)
            .ok_or_else(|| refused("InvalidInput", format!("local Sandbox record {key}")))
    };
    let boot = match value.get_text("boot") {
        None => None,
        Some(hex) => Some(
            (hex.len() == 32)
                .then(|| u128::from_str_radix(hex, 16).ok())
                .flatten()
                .ok_or_else(|| refused("InvalidInput", "local Sandbox record boot"))?,
        ),
    };
    let process = LocalProcess {
        pid: u32::try_from(number("pid")?).map_err(failed)?,
        start: registry::text(value, "start")?
            .parse::<u64>()
            .map_err(failed)?,
        boot,
    };
    Ok(Recorded::Local {
        directory,
        id: ContainerId::parse(registry::text(value, "container")?).map_err(failed)?,
        process,
        port: u16::try_from(number("port")?).map_err(failed)?,
        labels: labels(value)?,
    })
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn local_record(_: &SdkValue, id: &str, _: &SdkValue) -> Result<Recorded> {
    Err(refused(
        "Unsupported",
        format!("Sandbox {id} is a local Sandbox (LAYERFS_SANDBOX=local), which needs Linux"),
    ))
}
impl Recorded {
    /// The runtime, observed running once (it starts nothing).
    pub(crate) fn reopen(self) -> std::result::Result<Runtime, Box<WireFailure>> {
        match self {
            Self::Docker(docker, id, request) => {
                Sandbox::reopen(docker, id, request).map(Runtime::Docker)
            }
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::Local {
                directory,
                id,
                process,
                port,
                labels,
            } => local::Sandbox::reopen(directory, id, process, port, labels).map(Runtime::Local),
        }
    }
    /// The runtime found stopped: only its one Delete is admitted.
    pub(crate) fn stopped(self) -> Runtime {
        match self {
            Self::Docker(docker, id, request) => {
                Runtime::Docker(Sandbox::stopped(docker, id, request))
            }
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            Self::Local {
                directory,
                id,
                process,
                port,
                labels,
            } => Runtime::Local(local::Sandbox::stopped(
                directory, id, process, port, labels,
            )),
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
        Recorded::Docker(..) => crate::pool::daemon_host(args),
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        Recorded::Local { .. } => None,
    };
    let runtime = recorded.reopen().map_err(|failure| {
        if failure.container_gone() || failure.container_stopped() {
            gone(id, container, failure.container_stopped())
        } else {
            failed(failure)
        }
    })?;
    let dial = crate::pool::Dial {
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
        controls: crate::pool::Controls::new(control, dial),
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
            let status =
                match workspace(&mut *crate::trace::control(&state.controls)?).status(token) {
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
fn absent(failure: &layerfs_api::OperationFailure) -> bool {
    matches!(&failure.cause, layerfs_api::OperationCause::Remote(refusal)
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
