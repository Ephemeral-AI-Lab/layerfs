//! `home.get --with tree` (call surface 2.0): everything in the home, from
//! its records only. One read of the record files; no worker, Docker or
//! network is asked, so a state here is what the records say: a machine's
//! live state, a Store's holders and a Sandbox whose runtime ended behind
//! LayerFS's back are not seen (`machine get --with live` and the doctors
//! ask). `machines` holds each machine (one with a record, or one a Store
//! or Sandbox names) with its sqlite `stores` and its `sandboxes`; `stores` the PostgreSQL Stores, which are on no machine;
//! `buckets` the buckets. Each node holds what its record holds, with its
//! `state` and that state's companion. A Workspace is in the home's records
//! only on `native` and for a Sandbox reached directly (C10): any other
//! Sandbox answers `workspaces` null and `workspaces_not_listed` true.
//! One record that cannot be read does not fail the tree (conformance R4
//! F5): an unreadable machine record is a `broken` `failed` machine, a Store
//! record whose machine is not known is in `stores` with its state
//! (`broken` `failed` when unreadable), and an unreadable Sandbox or
//! Workspace record, whose place is not known, is left out. Each kind's
//! own `.record` doctor check names the record; the others are listed.
use crate::inventory;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::paths;
use layerfs_home::outcome::{failed, Outcome};
use layerfs_home::state::{Reason, State};
use layerfs_machine::record::MachineRecord;
use layerfs_sandbox::record::{SandboxRecord, WorkspaceRecord};
use layerfs_store::record::{StoreRecord, StoreRef};
use std::collections::BTreeSet;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The names of a Store's projects the home records; null when its
/// folder cannot be read.
fn projects(home: &Path, store: &str) -> SdkValue {
    match paths::names(&paths::store_dir(home, store).join("project")) {
        Ok(names) => SdkValue::List(names.into_iter().map(SdkValue::Text).collect()),
        Err(_) => SdkValue::Null,
    }
}

/// What a record that cannot be read is: `broken` `failed`.
fn unreadable() -> State {
    State::broken(Reason::Failed, None)
}

/// One Store: `{ store, engine, projects, connections, state.. }`. `under`
/// is its machine's name and state, for a sqlite Store; a PostgreSQL
/// Store's `connections` are every side that connects to it, as `store
/// get` answers them (`layerfs_store::postgresql::connect::connections`,
/// spec v23), with `sandboxes` the machines with a Sandbox of it; a
/// sqlite Store has none (null). No URL holds a login. Its state is the
/// Store's one records-only derivation; `engine` is null for a kind it
/// does not read.
fn store(
    home: &Path,
    record: &StoreRecord,
    under: Option<(&str, &State)>,
    sandboxes: &[String],
) -> SdkValue {
    let state = layerfs_store::state::recorded_under(record, under.map(|(_, state)| state))
        .unwrap_or_else(|_| unreadable());
    let connections = match under {
        Some(_) => SdkValue::Null,
        None => layerfs_store::postgresql::connect::connections(record, sandboxes),
    };
    let engine = record.kind().ok().map(|kind| kind.as_str().to_owned());
    state.write(SdkValue::map([
        ("store", SdkValue::text(record.name())),
        ("engine", SdkValue::opt_text(engine)),
        ("projects", projects(home, record.name())),
        ("connections", connections),
    ]))
}

/// A Store whose record cannot be read: `broken` `failed`, on no machine.
fn unreadable_store(home: &Path, name: &str) -> SdkValue {
    unreadable().write(SdkValue::map([
        ("store", SdkValue::text(name)),
        ("engine", SdkValue::Null),
        ("projects", projects(home, name)),
        ("connections", SdkValue::Null),
    ]))
}

/// A Sandbox's Workspaces the home records: `{ workspace, branch }`, the
/// handle `<sandbox>:<name>` and the Branch it has mounted; none when
/// their folder cannot be read, and a record that cannot be read is left
/// out.
fn workspaces(home: &Path, sandbox: &str) -> Option<Vec<SdkValue>> {
    let dir = paths::workspace_dir(home, sandbox);
    let mut found: Vec<(String, Option<String>)> = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Some(Vec::new()),
        Err(_) => return None,
    };
    for entry in entries {
        let path = entry.ok()?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(Some(record)) = WorkspaceRecord::read(&path) {
            if let Some(name) = record.name().or(record.path()) {
                let branch = record.branch().map(str::to_owned);
                found.push((format!("{sandbox}:{name}"), branch));
            }
        }
    }
    found.sort();
    let node = |(workspace, branch): (String, Option<String>)| {
        SdkValue::map([
            ("workspace", SdkValue::Text(workspace)),
            ("branch", SdkValue::opt_text(branch)),
        ])
    };
    Some(found.into_iter().map(node).collect())
}

/// One Sandbox: `{ sandbox, project, image, workspaces,
/// workspaces_not_listed, state.. }`, under its machine's state.
/// `workspaces` is null when the home does not record them: the Sandbox is
/// blocked, or it is on a machine reached through its worker, which
/// `workspaces_not_listed` says, or their folder cannot be read.
fn sandbox(home: &Path, record: &SandboxRecord, machine: &State) -> SdkValue {
    let name = record.name();
    let state = record.status(machine);
    // only `native` and a Sandbox reached directly record them here
    let recorded = record.machine() == "native" || layerfs_sandbox::direct::forwarded(record);
    let (listed, not_listed) = match state {
        State::Blocked(_) => (SdkValue::Null, false),
        _ if !recorded => (SdkValue::Null, true),
        _ => workspaces(home, name).map_or((SdkValue::Null, true), |w| (SdkValue::List(w), false)),
    };
    state.write(SdkValue::map([
        ("sandbox", SdkValue::text(name)),
        ("project", SdkValue::opt_text(record.project())),
        ("image", SdkValue::opt_text(record.image())),
        ("workspaces", listed),
        ("workspaces_not_listed", SdkValue::Bool(not_listed)),
    ]))
}

/// The buckets the home records: `{ bucket, provider }`; `provider` is
/// null for a record that cannot be read.
fn buckets(home: &Path) -> Result<Vec<SdkValue>> {
    let mut found = Vec::new();
    for bucket in paths::names(&home.join("backup")).map_err(failed)? {
        let provider = match layerfs_home::home::read_record(&paths::bucket_record(home, &bucket)) {
            Ok(None) => continue,
            Ok(Some(record)) => record.get_text("provider").map(str::to_owned),
            Err(_) => None,
        };
        found.push(SdkValue::map([
            ("bucket", SdkValue::text(&bucket)),
            ("provider", SdkValue::opt_text(provider)),
        ]));
    }
    Ok(found)
}

fn size(recorded: Option<i64>) -> SdkValue {
    recorded.map_or(SdkValue::Null, SdkValue::Int)
}

/// The machine a sqlite Store's record places it on, if it names one
/// this build reads.
/// What the records already read say is on machine `name`, as
/// [`inventory::Held`]: its Sandboxes, and for a record with no 2.0 name
/// its sqlite Stores and the old Firecracker records it hosted too, whose
/// removes its fix runs first.
fn held(
    name: &str,
    record: Option<&MachineRecord>,
    (machines, stores, sandboxes): (&[MachineRecord], &[StoreRecord], &[SandboxRecord]),
) -> inventory::Held {
    let on = sandboxes.iter().filter(|s| s.machine() == name);
    let mut held = inventory::Held {
        sandboxes: on.map(|s| s.name().to_owned()).collect(),
        ..inventory::Held::default()
    };
    if record.is_some_and(MachineRecord::gone) {
        let on = stores.iter().filter(|s| placed(s).as_deref() == Some(name));
        held.stores = on.map(|s| s.name().to_owned()).collect();
        let hosted = machines
            .iter()
            .filter(|m| matches!(m.field("host"), Some(SdkValue::Text(host)) if host == name));
        held.hosted = hosted.map(|m| m.name().to_owned()).collect();
    }
    held
}

fn placed(record: &StoreRecord) -> Option<String> {
    match record.store_ref() {
        Ok(StoreRef::Sqlite { machine: on, .. }) => Some(on.clone()),
        _ => None,
    }
}

/// One machine: `{ machine, platform, cpus, memory_bytes, state.., stores,
/// sandboxes }`. `record` is none for a machine only a Store or Sandbox
/// names: `native` and `docker` exist with or without a record (ready, and
/// never started), any other is `broken` `missing`; `state` is what its
/// record says.
fn machine(
    home: &Path,
    (name, record, state): (&str, Option<&MachineRecord>, State),
    stores: &[StoreRecord],
    sandboxes: &[SandboxRecord],
) -> SdkValue {
    let built_in = matches!(name, "native" | "docker").then_some(name);
    let held_stores = (stores.iter())
        .filter(|record| placed(record).as_deref() == Some(name))
        .map(|record| store(home, record, Some((name, &state)), &[]))
        .collect();
    let run = (sandboxes.iter())
        .filter(|s| s.machine() == name)
        .map(|record| sandbox(home, record, &state))
        .collect();
    let platform = record.map_or(built_in, |record| record.platform_name());
    state.write(SdkValue::map([
        ("machine", SdkValue::text(name)),
        ("platform", SdkValue::opt_text(platform)),
        ("cpus", size(record.and_then(|record| record.int("cpus")))),
        (
            "memory_bytes",
            size(record.and_then(|record| record.int("memory"))),
        ),
        ("stores", SdkValue::List(held_stores)),
        ("sandboxes", SdkValue::List(run)),
    ]))
}

/// The records under `kind/` of the home that read (`Ok`) and the names of
/// those that cannot (`Err`).
fn records<T>(
    home: &Path,
    kind: &str,
    read: impl Fn(&str) -> Result<Option<T>>,
) -> Result<Vec<std::result::Result<T, String>>> {
    let mut found = Vec::new();
    for name in paths::names(&home.join(kind)).map_err(failed)? {
        match read(&name) {
            Ok(record) => found.extend(record.map(Ok)),
            Err(_) => found.push(Err(name)),
        }
    }
    Ok(found)
}

/// The tree: `{ machines, stores, buckets }`. A machine is listed when it
/// has a record or when a sqlite Store or a Sandbox names it, so nothing in
/// the home is left out for want of its machine's record. Refused only when
/// a folder of the home cannot be listed.
pub fn tree(home: &Path) -> Outcome {
    let (stores, unread_stores): (Vec<_>, Vec<_>) =
        records(home, "store", |name| StoreRecord::read(home, name))?
            .into_iter()
            .partition(std::result::Result::is_ok);
    let stores: Vec<StoreRecord> = stores.into_iter().flatten().collect();
    let sandboxes: Vec<SandboxRecord> =
        records(home, "sandbox", |name| SandboxRecord::read(home, name))?
            .into_iter()
            .flatten()
            .collect();
    let machine_records = records(home, "machine", |name| MachineRecord::read(home, name))?;
    let unread: BTreeSet<String> = (machine_records.iter())
        .filter_map(|read| read.as_ref().err().cloned())
        .collect();
    let recorded: Vec<MachineRecord> = machine_records.into_iter().flatten().collect();
    let mut names: BTreeSet<String> = recorded.iter().map(|m| m.name().to_owned()).collect();
    names.extend(unread.iter().cloned());
    names.extend(stores.iter().filter_map(placed));
    names.extend(sandboxes.iter().map(|s| s.machine().to_owned()));
    let mut machines = Vec::new();
    for name in &names {
        let record = recorded.iter().find(|record| record.name() == name);
        let state = match unread.contains(name) {
            true => unreadable(),
            false => inventory::machine_state_with(
                name,
                record,
                held(name, record, (&recorded, &stores, &sandboxes)).holders(),
            ),
        };
        machines.push(machine(home, (name, record, state), &stores, &sandboxes));
    }
    // a Store on no machine: PostgreSQL, or one whose placement is not read
    let mut elsewhere: Vec<SdkValue> = (stores.iter())
        .filter(|record| !matches!(record.store_ref(), Ok(StoreRef::Sqlite { .. })))
        .map(|record| {
            let on = (sandboxes.iter())
                .filter(|s| {
                    let of = s
                        .project()
                        .and_then(|p| p.split_once('/'))
                        .map(|(st, _)| st);
                    of == Some(record.name())
                })
                .map(|s| s.machine().to_owned());
            store(home, record, None, &on.collect::<Vec<_>>())
        })
        .collect();
    elsewhere.extend(
        (unread_stores.into_iter()).filter_map(|read| Some(unreadable_store(home, &read.err()?))),
    );
    Ok(SdkValue::map([
        ("machines", SdkValue::List(machines)),
        ("stores", SdkValue::List(elsewhere)),
        ("buckets", SdkValue::List(buckets(home)?)),
    ]))
}
