//! A machine's records become the adapters' [`Spec`]: its
//! [`MachineRecord`] and `machine/<name>/worker.json` (endpoint and
//! secret; 0600, while the worker runs). Nothing in the adapters
//! reads the home.
use super::record::MachineRecord;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::{self, paths, write_record};
use layerfs_home::outcome::refused;
use layerfs_platform::{Endpoint, HostOs, Lasting, MachineError, Platform, Running, Spec, Tools};
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
    write_built_ins(home)?;
    ENSURED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key);
    Ok(())
}

fn write_built_ins(home: &Path) -> Result<()> {
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
            MachineRecord::built_in(name).write(home)?;
        }
    }
    Ok(())
}

/// The machine `name`'s record, or `NotFound`, for an operation on it.
/// Refused `Busy` while a live process creates or removes it (its record's
/// `op`), so no call starts, moves to or runs
/// on it meanwhile; `Unresolved` once a gone one left that outcome unknown.
pub fn machine_of(home: &Path, name: &str) -> Result<MachineRecord> {
    let record = MachineRecord::of(home, name)?;
    record.not_held()?;
    Ok(record)
}

/// The provider's name for a machine's VM: `lfs-<home id prefix>-<machine>`,
/// so two homes on one host never share a VM (O33).
pub fn vm_name(home_id: &str, name: &str) -> String {
    let plain: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    format!("lfs-{}-{plain}", &home_id[..8])
}

fn int(record: &MachineRecord, name: &str, default: i64) -> i64 {
    record.int(name).unwrap_or(default)
}

/// How this process reaches its tools, with the docker machine's socket.
pub fn tools(home: &Path) -> Result<Tools> {
    let mut tools = Tools::from_env();
    if let Some(record) = MachineRecord::read(home, "docker")? {
        if let Some(socket) = record.docker() {
            tools.docker_socket = PathBuf::from(socket);
        }
    }
    Ok(tools)
}

/// The adapters' description of the machine `record` describes.
pub fn spec(home: &Path, home_id: &str, record: &MachineRecord) -> Result<Spec> {
    let name = record.name();
    let kind = record.platform()?;
    let lasting = record.field("lasting");
    Ok(Spec {
        name: name.to_owned(),
        kind,
        vm_name: record
            .text("vm_name")
            .map(str::to_owned)
            .unwrap_or_else(|| vm_name(home_id, name)),
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
        worker: worker(home, name)?,
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
/// record write that says `running` drops a create's `op` (a
/// start runs under the machine's start lock, where no other one lives).
pub fn save_running(home: &Path, name: &str, running: &Running) -> Result<()> {
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
        record.with("state", SdkValue::text("running")).without_op()
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
