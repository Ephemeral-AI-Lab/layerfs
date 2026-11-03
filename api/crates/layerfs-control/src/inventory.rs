//! What exists in the home and how it relates (#536 §5 "Derived, never
//! stored", §9.2): reads by name, the relationships no record stores
//! (machine → its sqlite Stores, its Sandboxes and the Firecracker
//! machines it hosts; Store → its projects' Sandboxes; project → its
//! Sandboxes), and the checks the calls that need them make. Layout 3
//! keeps each Sandbox at `sandbox/<name>/`: a Sandbox by name is one read;
//! only the relationships (by machine, Store or project) list `sandbox/*`.
use crate::common::home::paths;
use crate::common::outcome::{failed, refused};
use crate::machine::record::MachineRecord;
use crate::sandbox::record::SandboxRecord;
use crate::store::record::{Placement, StoreRecord, StoreRef};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The machine that is this host.
const NATIVE: &str = "native";

/// The folder names under `dir`, sorted; none when it is absent.
fn names(dir: &Path) -> Result<Vec<String>> {
    paths::names(dir).map_err(failed)
}

/// Every Store record, by name.
pub fn stores(home: &Path) -> Result<Vec<StoreRecord>> {
    let mut found = Vec::new();
    for store in names(&home.join("store"))? {
        found.extend(StoreRecord::read(home, &store)?);
    }
    Ok(found)
}

/// Every machine record, by name.
pub fn machines(home: &Path) -> Result<Vec<MachineRecord>> {
    let mut found = Vec::new();
    for machine in names(&home.join("machine"))? {
        found.extend(MachineRecord::read(home, &machine)?);
    }
    Ok(found)
}

/// Every Sandbox record, by name.
pub fn sandboxes(home: &Path) -> Result<Vec<SandboxRecord>> {
    let mut found = Vec::new();
    for name in names(&home.join("sandbox"))? {
        found.extend(SandboxRecord::read(home, &name)?);
    }
    Ok(found)
}

/// The Sandbox named `name`: one read of `sandbox/<name>/record.json`.
pub fn sandbox(home: &Path, name: &str) -> Result<Option<SandboxRecord>> {
    SandboxRecord::read(home, name)
}

/// The sqlite Stores whose record names machine `machine`. A Store record
/// of a kind LayerFS does not write is refused, as before.
pub fn stores_on(home: &Path, machine: &str) -> Result<Vec<String>> {
    let mut on = Vec::new();
    for store in stores(home)? {
        if let StoreRef::Sqlite {
            machine: Placement::Machine(placed),
            ..
        } = store.store_ref()?
        {
            if placed == machine {
                on.push(store.name().to_owned());
            }
        }
    }
    Ok(on)
}

/// The Sandboxes whose record names machine `machine`.
pub fn sandboxes_on(home: &Path, machine: &str) -> Result<Vec<String>> {
    let on = sandboxes(home)?.into_iter();
    let on = on.filter(|sandbox| sandbox.machine() == machine);
    Ok(on.map(|sandbox| sandbox.name().to_owned()).collect())
}

/// The Firecracker machines Lima machine `machine` hosts.
pub fn hosted_on(home: &Path, machine: &str) -> Result<Vec<String>> {
    let hosted = machines(home)?.into_iter();
    let hosted = hosted.filter(|record| record.host() == Some(machine));
    Ok(hosted.map(|record| record.name().to_owned()).collect())
}

/// The Sandboxes, of any machine, whose project is in Store `store`.
pub fn sandboxes_of_store(home: &Path, store: &str) -> Result<Vec<String>> {
    let of = sandboxes(home)?.into_iter().filter(|sandbox| {
        let project = sandbox.project().unwrap_or("");
        project.split_once('/').map(|(s, _)| s) == Some(store)
    });
    Ok(of.map(|sandbox| sandbox.name().to_owned()).collect())
}

/// The Sandboxes of project `project` (`<store>/<project>`).
pub fn sandboxes_of_project(home: &Path, project: &str) -> Result<Vec<String>> {
    let of = sandboxes(home)?.into_iter();
    let of = of.filter(|sandbox| sandbox.project() == Some(project));
    Ok(of.map(|sandbox| sandbox.name().to_owned()).collect())
}

/// `machine.remove` and `machine.disconnect`: refused `InUse` while the
/// machine holds Stores, Sandboxes or Firecracker machines.
pub fn can_remove_machine(home: &Path, name: &str) -> Result<()> {
    let stores = stores_on(home, name)?;
    let sandboxes = sandboxes_on(home, name)?;
    let hosted = hosted_on(home, name)?;
    if stores.is_empty() && sandboxes.is_empty() && hosted.is_empty() {
        return Ok(());
    }
    Err(refused(
        "InUse",
        format!(
            "machine {name} holds Stores [{}], Sandboxes [{}] and hosts [{}]",
            stores.join(", "),
            sandboxes.join(", "),
            hosted.join(", ")
        ),
    ))
}

/// `machine.stop`: refused `InUse` while a Sandbox is recorded on machine
/// `name` or on a Firecracker machine it hosts; `detail.related` lists
/// them as `{kind: "sandbox", name}`. Under the home lock.
pub fn can_stop_machine(home: &Path, name: &str) -> Result<()> {
    let mut sandboxes = sandboxes_on(home, name)?;
    for hosted in hosted_on(home, name)? {
        sandboxes.extend(sandboxes_on(home, &hosted)?);
    }
    if sandboxes.is_empty() {
        return Ok(());
    }
    let mut refusal = refused(
        "InUse",
        format!(
            "machine {name} runs Sandboxes [{}]; close them first",
            sandboxes.join(", ")
        ),
    );
    let related = sandboxes
        .into_iter()
        .map(|sandbox| {
            SdkValue::map([
                ("kind", SdkValue::text("sandbox")),
                ("name", SdkValue::Text(sandbox)),
            ])
        })
        .collect();
    refusal.detail = SdkValue::map([("related", SdkValue::List(related))]);
    Err(refusal)
}

/// `machine.update`: refused `InUse` while Sandboxes run on machine `name`
/// (a running Sandbox keeps the image and settings it started with).
pub fn can_change_machine(home: &Path, name: &str) -> Result<()> {
    let running = sandboxes_on(home, name)?;
    if running.is_empty() {
        return Ok(());
    }
    Err(refused(
        "InUse",
        format!(
            "Sandboxes [{}] run on {name}; close them before changing it",
            running.join(", ")
        ),
    ))
}

/// `store.push`: refused `InvalidInput` unless `record` is a sqlite Store
/// on `native` and `to` another machine; `NotFound` for an unknown `to`.
/// Its in-use half (leases, Sandbox records) runs under the Store's lock.
pub fn can_push(home: &Path, record: &StoreRecord, to: &str) -> Result<()> {
    let name = record.name();
    let on = sqlite_machine(record)?;
    if on != NATIVE {
        return Err(refused(
            "InvalidInput",
            format!("Store {name} is on machine {on}; `layerfs store pull {name}` brings it to {NATIVE} first"),
        ));
    }
    if to == NATIVE {
        return Err(refused(
            "InvalidInput",
            format!("Store {name} is already on {NATIVE}"),
        ));
    }
    if MachineRecord::read(home, to)?.is_none() {
        return Err(refused("NotFound", format!("no machine named {to}")));
    }
    Ok(())
}

/// `store.pull`: refused `InvalidInput` unless `record` is a sqlite Store
/// on a machine other than `native`; answers that machine. Its in-use half
/// runs under the Store's lock, as `store.push`'s.
pub fn can_pull(record: &StoreRecord) -> Result<String> {
    let on = sqlite_machine(record)?;
    if on == NATIVE {
        return Err(refused(
            "InvalidInput",
            format!("Store {} is already on {NATIVE}", record.name()),
        ));
    }
    Ok(on)
}

/// The machine of sqlite Store `record`; a PostgreSQL Store never moves.
fn sqlite_machine(record: &StoreRecord) -> Result<String> {
    if let StoreRef::Postgres { .. } = record.store_ref()? {
        return Err(refused(
            "InvalidInput",
            format!(
                "Store {} is a PostgreSQL database; it does not move",
                record.name()
            ),
        ));
    }
    record.machine()
}

/// `store.push`, `store.pull`, `store.remove` and `backup.restore`: refused
/// `InUse` while a Sandbox of one of Store `store`'s projects runs.
pub fn can_retire_store(home: &Path, store: &str) -> Result<()> {
    let sandboxes = sandboxes_of_store(home, store)?;
    if sandboxes.is_empty() {
        return Ok(());
    }
    Err(refused(
        "InUse",
        format!(
            "Store {store} has running Sandboxes: {}",
            sandboxes.join(", ")
        ),
    ))
}

/// `project.remove`: refused `InUse` while a Sandbox of `project` runs
/// (the first one found is named).
pub fn can_remove_project(home: &Path, project: &str) -> Result<()> {
    match sandboxes_of_project(home, project)?.first() {
        None => Ok(()),
        Some(sandbox) => Err(refused(
            "InUse",
            format!("Sandbox {sandbox} of {project} is running"),
        )),
    }
}
