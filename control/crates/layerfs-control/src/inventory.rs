//! What exists in the home and how it relates (#536 §5 "Derived, never
//! stored", §9.2): reads by name, the relationships no record stores
//! (machine → its sqlite Stores, its Sandboxes and the old Firecracker
//! records it hosted; Store → its projects' Sandboxes; project → its
//! Sandboxes), and the checks the calls that need them make. Layout 3
//! keeps each Sandbox at `sandbox/<name>/`: a Sandbox by name is one read;
//! only the relationships (by machine, Store or project) list `sandbox/*`.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::paths;
use layerfs_home::outcome::{failed, refused};
use layerfs_machine::record::MachineRecord;
use layerfs_store::lease;
use layerfs_store::record::{Placement, StoreRef};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The folder names under `dir`, sorted; none when it is absent.
fn names(dir: &Path) -> Result<Vec<String>> {
    paths::names(dir).map_err(failed)
}

pub use layerfs_store::record::stores;

/// Every machine record, by name.
pub fn machines(home: &Path) -> Result<Vec<MachineRecord>> {
    let mut found = Vec::new();
    for machine in names(&home.join("machine"))? {
        found.extend(MachineRecord::read(home, &machine)?);
    }
    Ok(found)
}

pub use layerfs_sandbox::record::{sandbox, sandboxes};

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

/// The old Firecracker machine records (a platform removed in 2.0) that
/// name Lima machine `machine` as their host: their microVMs' folders and
/// data disks are still on its data disk.
pub fn hosted_on(home: &Path, machine: &str) -> Result<Vec<String>> {
    let hosted = machines(home)?.into_iter().filter(
        |record| matches!(record.field("host"), Some(SdkValue::Text(host)) if host == machine),
    );
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

/// `machine.remove`: refused `InUse` while the machine holds Stores,
/// Sandboxes or old Firecracker records it hosted (removing it would delete
/// their microVMs' disks; forgetting those records first lets it go);
/// `detail.related` lists them (#536 §10b).
pub fn can_remove_machine(home: &Path, name: &str) -> Result<()> {
    let stores = stores_on(home, name)?;
    let sandboxes = sandboxes_on(home, name)?;
    let hosted = hosted_on(home, name)?;
    if stores.is_empty() && sandboxes.is_empty() && hosted.is_empty() {
        return Ok(());
    }
    let message = format!(
        "machine {name} holds Stores [{}], Sandboxes [{}] and hosts [{}]",
        stores.join(", "),
        sandboxes.join(", "),
        hosted.join(", ")
    );
    let mut held = related("store", &stores);
    held.extend(related("sandbox", &sandboxes));
    held.extend(related("machine", &hosted));
    Err(in_use(message, held))
}

/// `machine.stop`: refused `InUse` while a Sandbox is recorded on machine
/// `name`; `detail.related` lists them as `{kind: "sandbox", name}`. Under
/// the home lock.
pub fn can_stop_machine(home: &Path, name: &str) -> Result<()> {
    let sandboxes = sandboxes_on(home, name)?;
    if sandboxes.is_empty() {
        return Ok(());
    }
    let message = format!(
        "machine {name} runs Sandboxes [{}]; close them first",
        sandboxes.join(", ")
    );
    Err(in_use(message, related("sandbox", &sandboxes)))
}

/// `{kind, name}` for each of `names`, as `detail.related` lists them.
fn related(kind: &str, names: &[String]) -> Vec<SdkValue> {
    let entry = |name: &String| {
        SdkValue::map([
            ("kind", SdkValue::text(kind)),
            ("name", SdkValue::text(name)),
        ])
    };
    names.iter().map(entry).collect()
}

/// `InUse` with `message`, `detail.related` listing what holds the entity.
fn in_use(message: String, related: Vec<SdkValue>) -> SdkRefusal {
    let mut refusal = refused("InUse", message);
    refusal.detail = SdkValue::map([("related", SdkValue::List(related))]);
    refusal
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

/// `store.push`, `store.pull`, `store.remove`, `store.disconnect` and
/// `backup.restore`: refused `InUse` while
/// another live process holds Store `store` (its lease) or a Sandbox of one
/// of its projects is recorded; `detail.related` lists them as `{kind:
/// "lease", name: <pid>}` and `{kind: "sandbox", name}`. Under the home
/// lock and the Store's lock (it removes stale leases).
pub fn store_idle(home: &Path, store: &str) -> Result<()> {
    let holders = lease::holders(home, store)?;
    let sandboxes = sandboxes_of_store(home, store)?;
    let mut why = Vec::new();
    if !holders.is_empty() {
        why.push(format!("Store {store} is open in process {holders:?}"));
    }
    if !sandboxes.is_empty() {
        let names = sandboxes.join(", ");
        why.push(format!("Store {store} has running Sandboxes: {names}"));
    }
    if why.is_empty() {
        return Ok(());
    }
    let pids: Vec<String> = holders.iter().map(u32::to_string).collect();
    let mut held = related("lease", &pids);
    held.extend(related("sandbox", &sandboxes));
    Err(in_use(why.join("; "), held))
}

/// `project.remove`: refused `InUse` while a Sandbox of `project` is
/// recorded; `detail.related` lists them as `{kind: "sandbox", name}`.
pub fn can_remove_project(home: &Path, project: &str) -> Result<()> {
    let sandboxes = sandboxes_of_project(home, project)?;
    let message = match sandboxes.as_slice() {
        [] => return Ok(()),
        [sandbox] => format!("Sandbox {sandbox} of {project} is running"),
        many => format!("Sandboxes {} of {project} are running", many.join(", ")),
    };
    Err(in_use(message, related("sandbox", &sandboxes)))
}
