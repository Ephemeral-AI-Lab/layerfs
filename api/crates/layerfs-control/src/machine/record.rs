//! A machine's records: `machine/<name>/record.json` (kind, size, images,
//! ssh host, lasting guest state, state) and `machine/<name>/worker.json`
//! (endpoint, secret, live forward; 0600, while the worker runs). Records
//! become the adapters' [`Spec`]; nothing in the adapters reads the home.
use crate::common::home::{self, paths, read_record, with_field, write_record};
use crate::common::outcome::refused;
use crate::store::record::{Placement, StoreRef};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_machine::{
    Endpoint, HostOs, Lasting, MachineError, Platform, Running, Spec, SshSpec, Tools,
};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// Default size of a new VM machine.
pub const DEFAULT_CPUS: i64 = 2;
/// 4 GiB.
pub const DEFAULT_MEMORY: i64 = 4 << 30;
/// 20 GiB.
pub const DEFAULT_DISK: i64 = 20 << 30;

/// `machine/native/` and `machine/docker/` exist from the first call and
/// are never removed. Read first; the home lock is taken only to write one.
pub fn ensure_built_ins(home: &Path) -> Result<()> {
    let missing = |name: &str| -> Result<bool> {
        Ok(read_record(&paths::machine_record(home, name))?.is_none())
    };
    if !crate::common::names::BUILT_IN
        .iter()
        .any(|name| missing(name).unwrap_or(true))
    {
        return Ok(());
    }
    let _lock = home::lock::take(home)?;
    for name in crate::common::names::BUILT_IN {
        let path = paths::machine_record(home, name);
        if missing(name)? {
            write_record(
                &path,
                &SdkValue::map([
                    ("name", SdkValue::text(name)),
                    ("platform", SdkValue::text(name)),
                    ("created", SdkValue::Int(home::now_ms())),
                ]),
            )?;
        }
    }
    Ok(())
}

/// The machine `name`'s record, or `NotFound`.
/// Refused `InUse` while a live process removes the machine (its record's
/// `removing` pid), so no call starts, moves to or runs on it meanwhile.
pub fn machine_of(home: &Path, name: &str) -> Result<SdkValue> {
    let record = raw(home, name)?;
    if let Some(pid) = removing(&record) {
        return Err(refused(
            "InUse",
            format!("machine {name} is being removed by process {pid}"),
        ));
    }
    Ok(record)
}

/// Machine `name`'s record as it is, being removed or not.
pub fn raw(home: &Path, name: &str) -> Result<SdkValue> {
    crate::common::names::name("machine", name).map_err(|why| refused("InvalidInput", why))?;
    read_record(&paths::machine_record(home, name))?
        .ok_or_else(|| refused("NotFound", format!("no machine named {name}")))
}

/// The live process removing a machine, from its record.
pub fn removing(record: &SdkValue) -> Option<i64> {
    let pid = record.get("removing")?.get_int("pid")?;
    crate::common::home::lease::alive(u32::try_from(pid).ok()?).then_some(pid)
}

/// A record's kind.
pub fn kind_of(record: &SdkValue) -> Result<Platform> {
    record
        .get_text("platform")
        .and_then(Platform::parse)
        .ok_or_else(|| refused("Failed", "a machine record names no known platform"))
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

fn int(record: &SdkValue, name: &str, default: i64) -> i64 {
    record.get_int(name).unwrap_or(default)
}

/// How this process reaches its tools, with the docker machine's socket.
pub fn tools(home: &Path) -> Result<Tools> {
    let mut tools = Tools::from_env();
    if let Some(record) = read_record(&paths::machine_record(home, "docker"))? {
        if let Some(socket) = record.get_text("docker") {
            tools.docker_socket = PathBuf::from(socket);
        }
    }
    Ok(tools)
}

/// The adapters' description of machine `name`.
pub fn spec(home: &Path, home_id: &str, name: &str, record: &SdkValue) -> Result<Spec> {
    let kind = kind_of(record)?;
    let host = match record.get_text("host") {
        Some(host) if kind == Platform::Firecracker => {
            let host_record = machine_of(home, host)?;
            Some(Box::new(spec(home, home_id, host, &host_record)?))
        }
        _ => None,
    };
    let ssh = match record.get("ssh") {
        Some(ssh @ SdkValue::Map(_)) => Some(SshSpec {
            host: ssh.get_text("host").unwrap_or("").to_owned(),
            user: ssh.get_text("user").unwrap_or("root").to_owned(),
            port: ssh.get_int("port").unwrap_or(22) as u16,
            identity: ssh.get_text("identity").map(PathBuf::from),
            known_hosts: paths::machine_dir(home, name).join("known_hosts"),
            data_root: ssh
                .get_text("root")
                .unwrap_or(layerfs_machine::DATA)
                .to_owned(),
        }),
        _ => None,
    };
    let lasting = record.get("lasting");
    Ok(Spec {
        name: name.to_owned(),
        kind,
        vm_name: record
            .get_text("vm_name")
            .map(str::to_owned)
            .unwrap_or_else(|| vm_name(home_id, name)),
        cpus: int(record, "cpus", DEFAULT_CPUS) as u32,
        memory: int(record, "memory", DEFAULT_MEMORY) as u64,
        disk: int(record, "disk", DEFAULT_DISK) as u64,
        // the machine's own Sandbox image archive, copied in at start: the
        // record's `image_archive`, or an `image` that names a file (an
        // `image` digest is one the bundle's archive holds)
        image: record
            .get_text("image_archive")
            .or(record.get_text("image").filter(|i| i.starts_with('/')))
            .map(PathBuf::from),
        // the docker machine's worker container runs its Sandbox image, as
        // 1.1.0's macOS worker did
        os_image: record
            .get_text("os_image")
            .or(record
                .get_text("image")
                .filter(|_| kind == Platform::Docker))
            .map(str::to_owned),
        nested: matches!(record.get("nested"), Some(SdkValue::Bool(true))),
        host,
        ssh,
        state: Lasting {
            port: lasting.and_then(|l| l.get_int("port")).map(|p| p as u16),
            network: lasting.and_then(|l| l.get_int("network")).map(|n| n as u8),
        },
        worker: worker(home, name)?,
        work_dir: paths::machine_dir(home, name),
        tools: tools(home)?,
    })
}

/// The endpoint in `worker.json`, if the machine has one.
pub fn worker(home: &Path, name: &str) -> Result<Option<Endpoint>> {
    let Some(value) = read_record(&paths::worker_record(home, name))? else {
        return Ok(None);
    };
    let address = value
        .get_text("address")
        .and_then(|a| a.parse().ok())
        .ok_or_else(|| {
            refused(
                "Failed",
                format!("machine {name}'s worker.json has no address"),
            )
        })?;
    Ok(Some(Endpoint {
        address,
        secret: value.get_text("secret").unwrap_or("").to_owned(),
        forward_pid: value.get_int("forward_pid").map(|p| p as u32),
    }))
}

/// Writes `worker.json` and the lasting state of a started machine.
pub fn save_running(home: &Path, name: &str, running: &Running) -> Result<()> {
    let endpoint = &running.endpoint;
    write_record(
        &paths::worker_record(home, name),
        &SdkValue::map([
            ("address", SdkValue::text(endpoint.address.to_string())),
            ("secret", SdkValue::text(&endpoint.secret)),
            (
                "forward_pid",
                endpoint
                    .forward_pid
                    .map(|p| SdkValue::Int(i64::from(p)))
                    .unwrap_or_default(),
            ),
            ("started", SdkValue::Int(home::now_ms())),
        ]),
    )?;
    let mut lasting = Vec::new();
    if let Some(port) = running.lasting.port {
        lasting.push(("port".to_owned(), SdkValue::Int(i64::from(port))));
    }
    if let Some(network) = running.lasting.network {
        lasting.push(("network".to_owned(), SdkValue::Int(i64::from(network))));
    }
    update(home, name, |record| {
        let record = with_field(record, "lasting", SdkValue::Map(lasting));
        with_field(record, "state", SdkValue::text("running"))
    })
}

/// Rewrites machine `name`'s record through `change`.
pub fn update(home: &Path, name: &str, change: impl FnOnce(SdkValue) -> SdkValue) -> Result<()> {
    let path = paths::machine_record(home, name);
    let record = read_record(&path)?
        .ok_or_else(|| refused("NotFound", format!("no machine named {name}")))?;
    write_record(&path, &change(record))
}

/// The Stores whose record names machine `name`.
pub fn stores_on(home: &Path, name: &str) -> Result<Vec<String>> {
    let mut stores = Vec::new();
    for store in paths::names(&home.join("store")).map_err(crate::common::outcome::failed)? {
        if let Some(record) = read_record(&paths::store_record(home, &store))? {
            if let StoreRef::Sqlite {
                machine: Placement::Machine(machine),
                ..
            } = StoreRef::from_record(&store, &record)?
            {
                if machine == name {
                    stores.push(store);
                }
            }
        }
    }
    Ok(stores)
}

/// The Sandboxes recorded under machine `name`.
pub fn sandboxes_on(home: &Path, name: &str) -> Result<Vec<String>> {
    paths::names(&paths::machine_dir(home, name).join("sandbox"))
        .map_err(crate::common::outcome::failed)
}

/// The Firecracker machines a Lima machine hosts.
pub fn hosted_on(home: &Path, name: &str) -> Result<Vec<String>> {
    let mut hosted = Vec::new();
    for machine in paths::names(&home.join("machine")).map_err(crate::common::outcome::failed)? {
        if let Some(record) = read_record(&paths::machine_record(home, &machine))? {
            if record.get_text("host") == Some(name) {
                hosted.push(machine);
            }
        }
    }
    Ok(hosted)
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

/// The backend a machine's Sandboxes run on, on this host.
pub fn backend_name(kind: Platform) -> &'static str {
    use layerfs_machine::Backend;
    match kind.backend(HostOs::current()) {
        Some(Backend::HostProcess) => "native",
        Some(Backend::Namespaces) => "namespaces",
        Some(Backend::Docker) => "docker",
        None => "-",
    }
}
