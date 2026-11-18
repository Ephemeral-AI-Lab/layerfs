//! A machine's records become the adapters' [`Spec`]: its
//! [`MachineRecord`] and `machine/<name>/worker.json` (endpoint and
//! secret; 0600, while the worker runs). Nothing in the adapters
//! reads the home.
pub use super::record::vm_name;
use super::record::MachineRecord;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::{self, paths, write_record};
use layerfs_home::outcome::refused;
use layerfs_home::state::Reason;
use layerfs_platform::{
    self as adapters, Endpoint, HostOs, Lasting, MachineError, Platform, Running, Spec, Tools,
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// Default size of a new VM machine.
pub const DEFAULT_CPUS: i64 = 2;
/// 4 GiB.
pub const DEFAULT_MEMORY: i64 = 4 << 30;
/// 20 GiB.
pub const DEFAULT_DISK: i64 = 20 << 30;

/// The homes, by path and id, whose built-in machines this process has
/// found or written (#535 R6).
static ENSURED: Mutex<BTreeSet<(PathBuf, String)>> = Mutex::new(BTreeSet::new());

/// `machine/native/` and `machine/docker/` exist from the first call and
/// are never removed. Checked once per process for each home, by its path
/// and its id `id` (a home made again at the same path is checked again),
/// not on every call (#535 R6). Read first; the home lock is taken only to
/// write one.
pub fn ensure_built_ins(home: &Path, id: &str) -> Result<()> {
    let key = (home.to_owned(), id.to_owned());
    if ENSURED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .contains(&key)
    {
        return Ok(());
    }
    write_built_ins(home, id)?;
    ENSURED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key);
    Ok(())
}

fn write_built_ins(home: &Path, id: &str) -> Result<()> {
    let missing = |name: &str| -> Result<bool> { Ok(MachineRecord::read(home, name)?.is_none()) };
    if !layerfs_home::names::BUILT_IN
        .iter()
        .any(|name| missing(name).unwrap_or(true))
    {
        return Ok(());
    }
    let _lock = home::lock::take(home)?;
    for name in layerfs_home::names::BUILT_IN {
        if missing(name)? {
            MachineRecord::built_in(name, id).write(home)?;
        }
    }
    Ok(())
}

/// The id of the home at `home`, which names its machines at their providers.
pub(super) fn home_id(home: &Path) -> Result<String> {
    Ok(layerfs_home::home::opened(home)?.id)
}

/// The machine `name`'s record, or `NotFound`, for an operation on it.
/// Refused `Busy` while a live process creates, starts, stops or removes it
/// (its record's `op`), so no call starts, moves to or runs on it
/// meanwhile; `Unresolved` once a gone one left that outcome unknown.
pub fn machine_of(home: &Path, name: &str) -> Result<MachineRecord> {
    let record = MachineRecord::of(home, name)?;
    record.not_held()?;
    Ok(record)
}

fn int(record: &MachineRecord, name: &str, default: i64) -> i64 {
    record.int(name).unwrap_or(default)
}

/// How this process reaches its tools, with the docker machine's socket
/// ([`docker_socket`]).
pub fn tools(home: &Path) -> Result<Tools> {
    let mut tools = Tools::from_env();
    tools.docker_socket = docker_socket(home)?.0;
    Ok(tools)
}

/// The Docker Engine socket the docker machine uses and where it comes from,
/// for the machine and for every Docker read of the home's (one choice): the
/// socket its record holds (`recorded`; `machine update docker --docker`, or
/// its first start), else, for a machine that already ran before sockets
/// were recorded (it ran, or its record predates `vm_name`, so an older
/// build used it), Docker's default socket it ran on (`legacy`; recorded
/// at its next start), else the host's automatic choice
/// ([`adapters::docker_automatic`]: `env:DOCKER_HOST`, `context:<name>`,
/// `desktop`, `colima`, `orbstack` or `default`). `--docker ""` leaves the
/// field null: automatic again, ran or not.
pub fn docker_socket(home: &Path) -> Result<(PathBuf, String)> {
    let record = MachineRecord::read(home, "docker")?;
    if let Some(socket) = record.as_ref().and_then(MachineRecord::docker) {
        return Ok((PathBuf::from(socket), "recorded".to_owned()));
    }
    let cleared = record.as_ref().is_some_and(|r| r.field("docker").is_some());
    let older = |r: &MachineRecord| r.ever_started() || r.text("vm_name").is_none();
    if !cleared && record.as_ref().is_some_and(older) {
        let socket = PathBuf::from(adapters::DEFAULT_DOCKER_SOCKET);
        return Ok((socket, "legacy".to_owned()));
    }
    let chosen = adapters::docker_automatic(&adapters::DockerEnv::current());
    let path = chosen
        .path
        .unwrap_or_else(|| PathBuf::from(adapters::DEFAULT_DOCKER_SOCKET));
    Ok((path, chosen.source))
}

/// The adapters' description of the machine `record` describes.
pub fn spec(home: &Path, home_id: &str, record: &MachineRecord) -> Result<Spec> {
    let mut spec = provider_spec(home, home_id, record)?;
    spec.worker = worker(home, record.name())?;
    Ok(spec)
}

/// A provider description that does not read or authenticate the worker endpoint.
pub fn provider_spec(home: &Path, home_id: &str, record: &MachineRecord) -> Result<Spec> {
    let name = record.name();
    let kind = record.platform()?;
    let lasting = record.field("lasting");
    Ok(Spec {
        name: name.to_owned(),
        kind,
        vm_name: record.provider_name(home_id),
        cpus: int(record, "cpus", DEFAULT_CPUS) as u32,
        memory: int(record, "memory", DEFAULT_MEMORY) as u64,
        disk: int(record, "disk", DEFAULT_DISK) as u64,
        // the machine's own Sandbox image archive, copied in at start: the
        // record's `image_archive`, or an `image` that names a file (an
        // `image` digest is one the bundle's archive holds)
        image: record
            .text("image_archive")
            .or(record.image().filter(|i| i.starts_with('/')))
            .map(PathBuf::from),
        // the docker machine's worker container runs its Sandbox image, as
        // 1.1.0's macOS worker did
        os_image: record
            .text("os_image")
            .or(record.image().filter(|_| kind == Platform::Docker))
            .map(str::to_owned),
        state: Lasting {
            port: lasting.and_then(|l| l.get_int("port")).map(|p| p as u16),
            gateway: lasting
                .and_then(|l| l.get_text("gateway"))
                .and_then(|g| g.parse().ok()),
        },
        worker: None,
        work_dir: paths::machine_dir(home, name),
        tools: tools(home)?,
    })
}

/// The endpoint in `worker.json`, if the machine has one.
pub fn worker(home: &Path, name: &str) -> Result<Option<Endpoint>> {
    let worker = layerfs_home::secrets::worker_secret(home, name)?;
    Ok(worker.map(|worker| Endpoint {
        address: worker.address,
        secret: worker.secret,
    }))
}

/// Writes `worker.json` and the lasting state of a started machine; the
/// record write that says `running` drops the `op` of the create or start
/// that made it so, and what an earlier failure left (`reason`, `error`).
pub fn save_running(home: &Path, name: &str, running: &Running) -> Result<()> {
    save_started(home, name, running, None)
}

/// Retains a started provider's endpoint and lasting state without
/// claiming readiness: the machine is recorded broken for `reason` (its
/// worker is of another build, or does not answer), with `error` as the
/// sentence behind it, and the `op` is dropped.
pub fn save_unready(
    home: &Path,
    name: &str,
    running: &Running,
    reason: Reason,
    error: &str,
) -> Result<()> {
    save_started(home, name, running, Some((reason, error)))
}

fn save_started(
    home: &Path,
    name: &str,
    running: &Running,
    broken: Option<(Reason, &str)>,
) -> Result<()> {
    let endpoint = &running.endpoint;
    write_record(
        &paths::worker_record(home, name),
        &SdkValue::map([
            ("address", SdkValue::text(endpoint.address.to_string())),
            ("secret", SdkValue::text(&endpoint.secret)),
            ("started", SdkValue::Int(home::now_ms())),
        ]),
    )?;
    let mut lasting = Vec::new();
    if let Some(port) = running.lasting.port {
        lasting.push(("port".to_owned(), SdkValue::Int(i64::from(port))));
    }
    if let Some(gateway) = running.lasting.gateway {
        lasting.push(("gateway".to_owned(), SdkValue::text(gateway.to_string())));
    }
    update(home, name, |record| {
        let record = record.with("lasting", SdkValue::Map(lasting));
        let record = record.with("state", SdkValue::text("running"));
        match broken {
            Some((reason, error)) => record.broken_as(reason, error).without_op(),
            None => record.mended().without_op(),
        }
    })
}

/// Rewrites machine `name`'s record through `change`.
pub fn update(
    home: &Path,
    name: &str,
    change: impl FnOnce(MachineRecord) -> MachineRecord,
) -> Result<()> {
    let record = MachineRecord::read(home, name)?
        .ok_or_else(|| refused("NotFound", format!("no machine named {name}")))?;
    change(record).write(home)
}

/// A machine error as a refusal: its code, message and what is missing.
pub fn refusal(error: MachineError) -> SdkRefusal {
    let mut refusal = refused(error.code.as_str(), error.message);
    refusal.detail = SdkValue::map([
        (
            "missing",
            SdkValue::List(error.missing.into_iter().map(SdkValue::Text).collect()),
        ),
        ("vm_exists", SdkValue::Bool(error.vm_exists)),
    ]);
    refusal
}

/// The backend a machine's Sandboxes run on, on this host, as records
/// store it (`native` for `host_process`, G6).
pub fn backend_name(kind: Platform) -> &'static str {
    kind.backend(HostOs::current())
        .map_or("-", layerfs_platform::Backend::stored_name)
}
