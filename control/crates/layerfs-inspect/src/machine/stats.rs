//! `machine.stats` and its inspect kind `machine_stats`
//! (layerfs-project#164): what a machine has and uses now, read where its
//! data is (the home on `native`, the machine's worker otherwise) in one
//! request. The home names the sqlite Stores and the Sandboxes its
//! records place on the machine; that side answers its processors and
//! memory, the filesystem that holds its data, each Store's size and how
//! many of the Sandboxes have a running daemon. The home adds what only it
//! knows: the machine's platform, what each value is a measure of
//! (`scope`) and the provider's name of the data volume.
//!
//! Every value is one `statvfs`, `stat` or small kernel file: no walk, no
//! provider tool, no daemon, and no Store file is opened. A Sandbox's row
//! (layerfs-project#182 L2) is its daemon record, its process's `/proc`
//! entry and its cgroup's small files on the side that runs it, never
//! anything inside the Sandbox's own root or namespaces.
use super::live::{self, Checked, DEADLINE};
use layerfs_sandbox::inspect::process::{self, NOT_OBSERVED};
use {super::kernel, layerfs_sandbox::{inspect::owner::Budget, deploy::Side}};
use layerfs_protocol::provision::install::SdkValue;
use layerfs_home::home::{self, paths};
use layerfs_home::names::{MachineName, StoreName};
use layerfs_home::outcome::Outcome;
use layerfs_machine::record::MachineRecord;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

/// The inspect kind.
pub(super) const KIND: &str = "machine_stats";

/// `machine.stats(machine)`: `{ machine, platform, observed_at, worker,
/// resources, data_disk, stores, sandboxes, complete, reason? }`.
///
/// - `worker`: `{ version, pid, started_at, protocol, bundle }` of the
///   machine's worker; null on a machine whose calls run in this process.
/// - `resources`: `{ cpus, memory_total, memory_used, disk_total,
///   disk_used, disk_free, cpu_busy_usec, cpu_total_usec, scope }`,
///   observed, sizes in bytes; `cpu_*` are the machine's processor time
///   busy and in all since boot, in microseconds (`/proc/stat`; a client
///   divides the two deltas of two reads); `scope` says whose processors
///   and memory they are (`host`, `vm`, `engine_host`). A value that was
///   not read is null, and `reason` (comma-joined words) says why:
///   `memory_not_read` for `memory_*`, `disk_not_read` for `disk_*`,
///   `cpu_unsupported` or `cpu_not_read` for `cpu_*`.
/// - `data_disk`: `{ path, fs, scope, volume, inodes_free }`: the
///   filesystem the `disk_*` values measure, what it is (`volume`,
///   `vm_disk`, `host_fs`) and the provider's name of the volume or disk.
/// - `stores`: `[{ name, bytes, allocated }]` of the sqlite Stores on the
///   machine, from `stat` of `store.sqlite` and its `-wal`.
/// - `sandboxes`: `{ recorded, live, items }`; `items` holds one row per
///   recorded Sandbox, `{ name, live, live_reason?, started_at, usage,
///   usage_reason? }` (`usage` as `sandbox.get`'s). A row the soft budget
///   did not reach stays, null with `deadline`.
/// - `complete`: false with `reason: "deadline"` when the soft budget
///   ended the Store or Sandbox part early.
///
/// A machine that is stopped or does not answer is `Unavailable`; nothing
/// starts it. No lock, lease, Store open or write, here or there.
pub fn machine_stats(home: &Path, machine: &MachineName) -> Outcome {
    let name = machine.as_str();
    let placed = placed(home, name)?;
    let recorded = i64::try_from(placed.sandboxes.len()).unwrap_or(i64::MAX);
    let answer = layerfs_sandbox::inspect::route::ask(home, name, KIND, placed.request())?;
    let (record, platform) = (placed.record, placed.platform);

    let part = |field: &str| answer.get(field).cloned().unwrap_or_default();
    let (measured, holds) = scopes(&platform);
    let resources = match part("resources") {
        SdkValue::Map(mut fields) => {
            // `reason` stays last
            let reason = fields.iter().position(|(field, _)| field == "reason");
            let reason = reason.map(|at| fields.remove(at));
            fields.push(("scope".to_owned(), SdkValue::text(measured)));
            fields.extend(reason);
            SdkValue::Map(fields)
        }
        other => other,
    };
    let found = part("data_disk");
    let of_disk = |field: &str| found.get(field).cloned().unwrap_or_default();
    let data_disk = SdkValue::map([
        ("path", of_disk("path")),
        ("fs", of_disk("fs")),
        ("scope", SdkValue::text(holds)),
        ("volume", SdkValue::opt_text(volume(home, record.as_ref()))),
        ("inodes_free", of_disk("inodes_free")),
    ]);
    let seen = part("sandboxes");
    let mut sandboxes = vec![
        ("recorded".to_owned(), SdkValue::Int(recorded)),
        (
            "live".to_owned(),
            seen.get("live").cloned().unwrap_or_default(),
        ),
    ];
    sandboxes.extend((seen.get("items").cloned()).map(|items| ("items".to_owned(), items)));
    let mut shown = vec![
        ("machine", SdkValue::text(name)),
        ("platform", SdkValue::Text(platform)),
        ("observed_at", part("observed_at")),
        ("worker", part("worker")),
        ("resources", resources),
        ("data_disk", data_disk),
        ("stores", part("stores")),
        ("sandboxes", SdkValue::Map(sandboxes)),
        ("complete", part("complete")),
    ];
    shown.extend(answer.get("reason").cloned().map(|why| ("reason", why)));
    Ok(SdkValue::Map(
        (shown.into_iter())
            .map(|(field, value)| (field.to_owned(), value))
            .collect(),
    ))
}

/// What the home records on a machine, as `machine.stats` and
/// `machine.space` send it to the side that holds its data.
pub(super) struct Placed {
    pub(super) record: Option<MachineRecord>,
    /// Its platform's name (`native` for the built-in machine with no
    /// record; empty when unknown).
    pub(super) platform: String,
    /// The sqlite Stores and the Sandboxes the home places on it.
    pub(super) stores: Vec<String>,
    pub(super) sandboxes: Vec<String>,
}

impl Placed {
    /// The request `{ stores, sandboxes }`.
    pub(super) fn request(&self) -> SdkValue {
        let names = |names: &[String]| SdkValue::List(names.iter().map(SdkValue::text).collect());
        SdkValue::map([
            ("stores", names(&self.stores)),
            ("sandboxes", names(&self.sandboxes)),
        ])
    }
}

/// What the home records on machine `name`.
pub(super) fn placed(
    home: &Path,
    name: &str,
) -> Result<Placed, layerfs_protocol::provision::install::SdkRefusal> {
    let record = MachineRecord::read(home, name)?;
    // a built-in machine with no record yet has the platform of its name
    let built_in = layerfs_home::names::built_in_platform(name);
    let platform = (record.as_ref())
        .and_then(MachineRecord::platform_name)
        .or(built_in)
        .unwrap_or("")
        .to_owned();
    let stores = (layerfs_store::record::stores(home)?.iter())
        .filter(|store| store.machine().ok().as_deref() == Some(name))
        .map(|store| store.name().to_owned())
        .collect();
    let sandboxes = (layerfs_sandbox::record::sandboxes(home)?.iter())
        .filter(|sandbox| sandbox.machine() == name)
        .map(|sandbox| sandbox.name().to_owned())
        .collect();
    Ok(Placed {
        record,
        platform,
        stores,
        sandboxes,
    })
}

/// What a machine of `platform`'s values measure: whose processors and
/// memory (`resources.scope`), and what the filesystem holding its data is
/// (`data_disk.scope`).
///
/// | platform | resources | data disk |
/// | --- | --- | --- |
/// | no provider (`native`) | `host` | `host_fs`: the home's filesystem |
/// | `lima`, `apple-container` | `vm` | `volume`: the machine's own disk |
/// | `docker` | `engine_host` | `vm_disk` on a macOS or Windows host, where the Engine is in a VM whose one disk every volume and image shares; `host_fs` on a Linux host, where a volume is a folder of the host's filesystem |
///
/// The docker row goes by this host's system, which the caller knows and
/// a worker in a container cannot tell: Docker Desktop on Linux, which
/// also runs a VM, reads as `host_fs`.
pub(super) fn scopes(platform: &str) -> (&'static str, &'static str) {
    match platform {
        // no provider holds it: Sandboxes run on the host's own OS
        own_os if !layerfs_machine::provided(own_os) => ("host", "host_fs"),
        "docker" if cfg!(target_os = "linux") => ("engine_host", "host_fs"),
        "docker" => ("engine_host", "vm_disk"),
        _ => ("vm", "volume"),
    }
}

/// The provider's name of the volume or disk that holds the machine's
/// data, from its record alone; none on `native`.
#[cfg(feature = "machine")]
fn volume(home: &Path, record: Option<&MachineRecord>) -> Option<String> {
    let spec = layerfs_machine::machine_provider_spec(home, record?).ok()?;
    layerfs_machine::platform::data_volume(&spec)
}

/// Without the machine adapters (a guest worker's build) no provider is
/// known.
#[cfg(not(feature = "machine"))]
fn volume(_: &Path, _: Option<&MachineRecord>) -> Option<String> {
    None
}

/// The request on the side that holds the machine's data, rooted at the
/// home or the worker's root: `{ observed_at, worker, resources,
/// data_disk: { path, fs, inodes_free }, stores, sandboxes: { live, items
/// }, complete, reason? }`. `worker` is what the worker's `inspect` operation
/// put on the request (who it is and since when); this process is no
/// worker on `Side::Home`.
pub(super) fn serve(side: &Side<'_>, request: &SdkValue, budget: &Budget) -> Outcome {
    let (root, worker) = match side {
        Side::Home(root) => (*root, SdkValue::Null),
        Side::Worker(root) => (*root, request.get("worker").cloned().unwrap_or_default()),
    };
    let stores = live::listed(request, "stores")?;
    for store in &stores {
        StoreName::parse(store)?;
    }
    let sandboxes = live::names(request)?;

    let int = |value: Option<i64>| value.map_or(SdkValue::Null, SdkValue::Int);
    let size = |bytes: Option<u64>| int(bytes.and_then(|bytes| i64::try_from(bytes).ok()));
    let memory = kernel::memory();
    let disk = home::filesystem(root).ok();
    let (kernel, cpu_reason) = kernel::cpu();
    let mut reasons: Vec<&str> = memory.reason.into_iter().collect();
    if disk.is_none() {
        reasons.push("disk_not_read");
    }
    reasons.extend(cpu_reason);
    let mut resources = vec![
        ("cpus", int(kernel::cpus())),
        ("memory_total", int(memory.total)),
        ("memory_used", int(memory.used)),
        ("disk_total", size(disk.as_ref().map(|disk| disk.total))),
        ("disk_used", size(disk.as_ref().map(|disk| disk.used))),
        ("disk_free", size(disk.as_ref().map(|disk| disk.free))),
        ("cpu_busy_usec", int(kernel.busy_usec)),
        ("cpu_total_usec", int(kernel.total_usec)),
    ];
    if !reasons.is_empty() {
        resources.push(("reason", SdkValue::Text(reasons.join(", "))));
    }
    let data_disk = SdkValue::map([
        ("path", SdkValue::Text(root.display().to_string())),
        (
            "fs",
            SdkValue::opt_text(disk.as_ref().map(|disk| disk.kind.clone())),
        ),
        (
            "inodes_free",
            size(disk.as_ref().and_then(|disk| disk.inodes_free)),
        ),
    ]);

    // the parts made of many units: each checks the budget between them
    let mut complete = true;
    let stores: Vec<SdkValue> = (stores.iter())
        .map(|store| {
            let sized = match budget.spent() {
                true => None,
                false => Some(store_bytes(root, store)),
            };
            complete &= sized.is_some();
            let (bytes, allocated) = sized.unwrap_or_default();
            SdkValue::map([
                ("name", SdkValue::text(store)),
                ("bytes", size(bytes)),
                ("allocated", size(allocated)),
            ])
        })
        .collect();
    let cgroups = Path::new(process::CGROUP_ROOT);
    let (checked, all) = live::checked(root, &sandboxes, budget, |daemon| {
        (
            process::start_of(daemon, kernel.boot),
            process::usage(daemon, cgroups),
        )
    });
    complete &= all;
    let running = checked.iter().filter(|((live, _), _)| *live == Some(true));
    let live = all.then(|| i64::try_from(running.count()).unwrap_or(i64::MAX));
    let items = sandboxes
        .iter()
        .zip(checked)
        .map(|(name, row)| item(name, row));

    let mut answer = vec![
        ("observed_at", SdkValue::Int(home::now_ms())),
        ("worker", worker),
        (
            "resources",
            SdkValue::Map(
                (resources.into_iter())
                    .map(|(field, value)| (field.to_owned(), value))
                    .collect(),
            ),
        ),
        ("data_disk", data_disk),
        ("stores", SdkValue::List(stores)),
        (
            "sandboxes",
            SdkValue::map([
                ("live", int(live)),
                ("items", SdkValue::List(items.collect())),
            ]),
        ),
        ("complete", SdkValue::Bool(complete)),
    ];
    if !complete {
        answer.push(("reason", SdkValue::text(DEADLINE)));
    }
    Ok(SdkValue::Map(
        (answer.into_iter())
            .map(|(field, value)| (field.to_owned(), value))
            .collect(),
    ))
}

/// What a Sandbox's row read: when its daemon process started, and its
/// usage with why that is null.
type Read = (Option<i64>, (SdkValue, Option<&'static str>));

/// Sandbox `name`'s row: `{ name, live, live_reason?, started_at, usage,
/// usage_reason? }`. With no daemon record read (the budget ended first,
/// the record is unreadable or names no daemon) its process is not
/// observed: `started_at` and `usage` are null, and `usage_reason` is the
/// check's reason (`deadline`, `record_unreadable`), else `not_observed`.
fn item(name: &str, ((live, why), read): (Checked, Option<Read>)) -> SdkValue {
    let mut fields = vec![
        ("name".to_owned(), SdkValue::text(name)),
        (
            "live".to_owned(),
            live.map_or(SdkValue::Null, SdkValue::Bool),
        ),
    ];
    fields.extend(why.map(|why| ("live_reason".to_owned(), SdkValue::text(why))));
    let (started, (usage, usage_reason)) = match read {
        Some(read) => read,
        None => (None, (SdkValue::Null, Some(why.unwrap_or(NOT_OBSERVED)))),
    };
    fields.push((
        "started_at".to_owned(),
        started.map_or(SdkValue::Null, SdkValue::Int),
    ));
    fields.push(("usage".to_owned(), usage));
    fields.extend(usage_reason.map(|why| ("usage_reason".to_owned(), SdkValue::text(why))));
    SdkValue::Map(fields)
}

/// The size of sqlite Store `store` under `root`: `store.sqlite` plus its
/// `-wal`, each by one `lstat` of its path and never opened (closing a
/// descriptor on a Store file would drop its owner's locks). `bytes` is
/// the files' length, `allocated` their blocks (`st_blocks` × 512): a
/// sparse file shows in the difference, and a clone or reflink counts in
/// full in both, since `stat` cannot see sharing. Both `None` when the
/// Store has no file here.
fn store_bytes(root: &Path, store: &str) -> (Option<u64>, Option<u64>) {
    let file = paths::store_file(root, store);
    let sized = |path: &Path| match std::fs::symlink_metadata(path) {
        Ok(found) if found.is_file() => Some((found.len(), found.blocks().saturating_mul(512))),
        _ => None,
    };
    let Some((bytes, allocated)) = sized(&file) else {
        return (None, None);
    };
    let mut wal = file.into_os_string();
    wal.push("-wal");
    let (more, more_allocated) = sized(Path::new(&wal)).unwrap_or_default();
    (
        Some(bytes.saturating_add(more)),
        Some(allocated.saturating_add(more_allocated)),
    )
}
