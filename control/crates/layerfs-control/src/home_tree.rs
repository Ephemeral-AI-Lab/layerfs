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
//! Sandbox answers `workspaces` null.
use crate::inventory;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::paths;
use layerfs_home::op::{Op, Standing};
use layerfs_home::outcome::{failed, Outcome};
use layerfs_home::state::{Reason, State};
use layerfs_machine::record::MachineRecord;
use layerfs_sandbox::record::{SandboxRecord, WorkspaceRecord};
use layerfs_store::record::{Placement, StoreRecord, StoreRef};
use std::collections::BTreeSet;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// What a recorded intent says of its entity: busy while its owner lives,
/// unresolved once a gone owner left its outcome unknown.
fn held(op: Option<Op>) -> Option<State> {
    let op = op?;
    match op.standing() {
        Standing::Running => Some(State::busy(&op)),
        Standing::Unresolved => Some(State::broken(Reason::Unresolved, None)),
        Standing::Stale => None,
    }
}

/// The names of a Store's projects the home records.
fn projects(home: &Path, store: &str) -> Result<SdkValue> {
    let names = paths::names(&paths::store_dir(home, store).join("project")).map_err(failed)?;
    Ok(SdkValue::List(
        names.into_iter().map(SdkValue::Text).collect(),
    ))
}

/// One Store: `{ store, engine, state.., projects, connections }`. `under`
/// is its machine's name and state, for a sqlite Store; `connections` are
/// the machines a PostgreSQL Store has an address of its own for, null for
/// a sqlite Store. Its state is the Store's one records-only derivation.
fn store(home: &Path, record: &StoreRecord, under: Option<(&str, &State)>) -> Result<SdkValue> {
    let state = layerfs_store::state::recorded_under(home, record, under.map(|(_, state)| state))?;
    let connections = match under {
        Some(_) => SdkValue::Null,
        None => {
            let own = record.own_connections().into_iter();
            SdkValue::List(own.map(|(machine, _)| SdkValue::Text(machine)).collect())
        }
    };
    Ok(state.write(SdkValue::map([
        ("store", SdkValue::text(record.name())),
        ("engine", SdkValue::text(record.kind()?.as_str())),
        ("projects", projects(home, record.name())?),
        ("connections", connections),
    ])))
}

/// A Sandbox's Workspaces the home records: `{ workspace, branch }`, the
/// handle `<sandbox>:<name>` and the Branch it has mounted.
fn workspaces(home: &Path, sandbox: &str) -> Result<Vec<SdkValue>> {
    let dir = paths::workspace_dir(home, sandbox);
    let mut found: Vec<(String, Option<String>)> = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(failed(error)),
    };
    for entry in entries {
        let path = entry.map_err(failed)?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Some(record) = WorkspaceRecord::read(&path)? {
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
    Ok(found.into_iter().map(node).collect())
}

/// One Sandbox: `{ sandbox, project, image, state.., workspaces }`, under
/// its machine's state. `workspaces` is null when the home does not record
/// them: the Sandbox is blocked, or it is on a machine reached through its
/// worker.
fn sandbox(home: &Path, record: &SandboxRecord, machine: &State) -> Result<SdkValue> {
    let name = record.name();
    let state = match record.state(false) {
        "creating" | "closing" => held(record.op()).unwrap_or(State::Ready),
        "unresolved" => State::broken(Reason::Unresolved, None),
        // it ended, or its create never recorded a runtime: only removing
        // its record is left
        "gone" => State::broken(
            Reason::Missing,
            Some(&format!("layerfs sandbox close {name}")),
        ),
        _ => machine
            .blocks("machine", record.machine())
            .unwrap_or(State::Ready),
    };
    let listed = match state {
        State::Blocked(_) => SdkValue::Null,
        _ => {
            let found = workspaces(home, name)?;
            // only `native` and a Sandbox reached directly record them here
            match found.is_empty() && record.machine() != "native" {
                true => SdkValue::Null,
                false => SdkValue::List(found),
            }
        }
    };
    Ok(state.write(SdkValue::map([
        ("sandbox", SdkValue::text(name)),
        ("project", SdkValue::opt_text(record.project())),
        ("image", SdkValue::opt_text(record.image())),
        ("workspaces", listed),
    ])))
}

/// The buckets the home records: `{ bucket, provider }`.
fn buckets(home: &Path) -> Result<Vec<SdkValue>> {
    let mut found = Vec::new();
    for bucket in paths::names(&home.join("backup")).map_err(failed)? {
        let record = layerfs_home::home::read_record(&paths::bucket_record(home, &bucket))?;
        let Some(record) = record else {
            continue;
        };
        found.push(SdkValue::map([
            ("bucket", SdkValue::text(&bucket)),
            ("provider", SdkValue::opt_text(record.get_text("provider"))),
        ]));
    }
    Ok(found)
}

fn size(recorded: Option<i64>) -> SdkValue {
    recorded.map_or(SdkValue::Null, SdkValue::Int)
}

/// The machine a sqlite Store's record places it on, if it names one.
fn placed(record: &StoreRecord) -> Result<Option<String>> {
    Ok(match record.store_ref()? {
        StoreRef::Sqlite {
            machine: Placement::Machine(on),
            ..
        } => Some(on.clone()),
        _ => None,
    })
}

/// One machine: `{ machine, platform, cpus, memory_bytes, state.., stores,
/// sandboxes }`. `record` is none for a machine only a Store or Sandbox
/// names: `native` and `docker` exist with or without a record (ready, and
/// never started), any other is `broken` `missing`.
fn machine(
    home: &Path,
    name: &str,
    record: Option<&MachineRecord>,
    stores: &[StoreRecord],
    sandboxes: &[SandboxRecord],
) -> Result<SdkValue> {
    let built_in = matches!(name, "native" | "docker").then_some(name);
    let state = inventory::machine_state(home, name, record)?;
    let mut held_stores = Vec::new();
    for store_record in stores {
        if placed(store_record)?.as_deref() == Some(name) {
            held_stores.push(store(home, store_record, Some((name, &state)))?);
        }
    }
    let mut run = Vec::new();
    for record in sandboxes.iter().filter(|s| s.machine() == name) {
        run.push(sandbox(home, record, &state)?);
    }
    let platform = record.map_or(built_in, |record| record.platform_name());
    Ok(state.write(SdkValue::map([
        ("machine", SdkValue::text(name)),
        ("platform", SdkValue::opt_text(platform)),
        ("cpus", size(record.and_then(|record| record.int("cpus")))),
        (
            "memory_bytes",
            size(record.and_then(|record| record.int("memory"))),
        ),
        ("stores", SdkValue::List(held_stores)),
        ("sandboxes", SdkValue::List(run)),
    ])))
}

/// The tree: `{ machines, stores, buckets }`. A machine is listed when it
/// has a record or when a sqlite Store or a Sandbox names it, so nothing in
/// the home is left out for want of its machine's record.
pub fn tree(home: &Path) -> Outcome {
    let stores = inventory::stores(home)?;
    let sandboxes = inventory::sandboxes(home)?;
    let recorded = inventory::machines(home)?;
    let mut names: BTreeSet<String> = recorded.iter().map(|m| m.name().to_owned()).collect();
    for record in &stores {
        names.extend(placed(record)?);
    }
    names.extend(sandboxes.iter().map(|s| s.machine().to_owned()));
    let mut machines = Vec::new();
    for name in &names {
        let record = recorded.iter().find(|record| record.name() == name);
        machines.push(machine(home, name, record, &stores, &sandboxes)?);
    }
    let mut postgresql = Vec::new();
    for record in &stores {
        if matches!(record.store_ref()?, StoreRef::Postgres { .. }) {
            postgresql.push(store(home, record, None)?);
        }
    }
    Ok(SdkValue::map([
        ("machines", SdkValue::List(machines)),
        ("stores", SdkValue::List(postgresql)),
        ("buckets", SdkValue::List(buckets(home)?)),
    ]))
}
