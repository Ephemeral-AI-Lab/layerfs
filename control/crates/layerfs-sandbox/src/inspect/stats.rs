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
//! provider tool, no daemon, and no Store file is opened.
use super::live::{self, DEADLINE};
use super::{host, Budget, Side};
use layerfs_bridge::control::SdkValue;
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
///   disk_used, disk_free, scope }`, observed, in bytes; `scope` says whose
///   processors and memory they are (`host`, `vm`, `engine_host`). A value
///   that was not read is null, and `reason` says why.
/// - `data_disk`: `{ path, fs, scope, volume, inodes_free }`: the
///   filesystem the `disk_*` values measure, what it is (`volume`,
///   `vm_disk`, `host_fs`) and the provider's name of the volume or disk.
/// - `stores`: `[{ name, bytes, allocated }]` of the sqlite Stores on the
///   machine, from `stat` of `store.sqlite` and its `-wal`.
/// - `sandboxes`: `{ recorded, live }`.
/// - `complete`: false with `reason: "deadline"` when the soft budget
///   ended the Store or Sandbox part early.
///
/// A machine that is stopped or does not answer is `Unavailable`; nothing
/// starts it. No lock, lease, Store open or write, here or there.
pub fn machine_stats(home: &Path, machine: &MachineName) -> Outcome {
    let name = machine.as_str();
    let record = MachineRecord::read(home, name)?;
    let platform = (record.as_ref())
        .and_then(MachineRecord::platform_name)
        .unwrap_or(if name == "native" { "native" } else { "" })
        .to_owned();
    let names =
        |names: Vec<String>| SdkValue::List(names.into_iter().map(SdkValue::Text).collect());
    let stores = (layerfs_store::record::stores(home)?.iter())
        .filter(|store| store.machine().ok().as_deref() == Some(name))
        .map(|store| store.name().to_owned())
        .collect();
    let sandboxes: Vec<String> = (crate::record::sandboxes(home)?.iter())
        .filter(|sandbox| sandbox.machine() == name)
        .map(|sandbox| sandbox.name().to_owned())
        .collect();
    let recorded = i64::try_from(sandboxes.len()).unwrap_or(i64::MAX);
    let request = SdkValue::map([("stores", names(stores)), ("sandboxes", names(sandboxes))]);
    let answer = super::ask(home, name, KIND, request)?;

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
    let live = part("sandboxes").get("live").cloned().unwrap_or_default();
    let mut shown = vec![
        ("machine", SdkValue::text(name)),
        ("platform", SdkValue::Text(platform)),
        ("observed_at", part("observed_at")),
        ("worker", part("worker")),
        ("resources", resources),
        ("data_disk", data_disk),
        ("stores", part("stores")),
        (
            "sandboxes",
            SdkValue::map([("recorded", SdkValue::Int(recorded)), ("live", live)]),
        ),
        ("complete", part("complete")),
    ];
    shown.extend(answer.get("reason").cloned().map(|why| ("reason", why)));
    Ok(SdkValue::Map(
        (shown.into_iter())
            .map(|(field, value)| (field.to_owned(), value))
            .collect(),
    ))
}

/// What a machine of `platform`'s values measure: whose processors and
/// memory (`resources.scope`), and what the filesystem holding its data is
/// (`data_disk.scope`).
///
/// | platform | resources | data disk |
/// | --- | --- | --- |
/// | `native` | `host` | `host_fs`: the home's filesystem |
/// | `lima`, `apple-container` | `vm` | `volume`: the machine's own disk |
/// | `docker` | `engine_host` | `vm_disk` on a macOS or Windows host, where the Engine is in a VM whose one disk every volume and image shares; `host_fs` on a Linux host, where a volume is a folder of the host's filesystem |
///
/// The docker row goes by this host's system, which the caller knows and
/// a worker in a container cannot tell: Docker Desktop on Linux, which
/// also runs a VM, reads as `host_fs`.
fn scopes(platform: &str) -> (&'static str, &'static str) {
    match platform {
        "native" => ("host", "host_fs"),
        "docker" if cfg!(target_os = "linux") => ("engine_host", "host_fs"),
        "docker" => ("engine_host", "vm_disk"),
        _ => ("vm", "volume"),
    }
}

/// The provider's name of the volume or disk that holds the machine's
/// data, from its record alone; none on `native`.
#[cfg(feature = "machine")]
fn volume(home: &Path, record: Option<&MachineRecord>) -> Option<String> {
    let id = home::opened(home).ok()?.id;
    let spec = layerfs_machine::machine_provider_spec(home, &id, record?).ok()?;
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
/// data_disk: { path, fs, inodes_free }, stores, sandboxes: { live },
/// complete, reason? }`. `worker` is what the worker's `inspect` operation
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
    let memory = host::memory();
    let disk = home::filesystem(root).ok();
    let mut reasons: Vec<&str> = memory.reason.into_iter().collect();
    if disk.is_none() {
        reasons.push("disk_not_read");
    }
    let mut resources = vec![
        ("cpus", int(host::cpus())),
        ("memory_total", int(memory.total)),
        ("memory_used", int(memory.used)),
        ("disk_total", size(disk.as_ref().map(|disk| disk.total))),
        ("disk_used", size(disk.as_ref().map(|disk| disk.used))),
        ("disk_free", size(disk.as_ref().map(|disk| disk.free))),
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
    let (checked, all) = live::checked(root, &sandboxes, budget);
    complete &= all;
    let running = checked.iter().filter(|(live, _)| *live == Some(true));
    let live = all.then(|| i64::try_from(running.count()).unwrap_or(i64::MAX));

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
        ("sandboxes", SdkValue::map([("live", int(live))])),
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
