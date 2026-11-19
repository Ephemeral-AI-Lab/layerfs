//! What exists in the home and how it relates (#536 §5 "Derived, never
//! stored", §9.2): reads by name, the relationships no record stores
//! (machine → its sqlite Stores, its Sandboxes and the old Firecracker
//! records it hosted; Store → its projects' Sandboxes; project → its
//! Sandboxes), and the checks the calls that need them make. Layout 3
//! keeps each Sandbox at `sandbox/<name>/`: a Sandbox by name is one read;
//! only the relationships (by machine, Store or project) list `sandbox/*`.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::paths;
use layerfs_home::outcome::{failed, refused, with_fix, with_next};
use layerfs_home::state::State;
use layerfs_machine::record::MachineRecord;
use layerfs_store::lease;
use layerfs_store::record::StoreRef;
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

/// The state of machine `name` from the home's records alone, with no
/// probe (call surface 2.0): its record's ([`MachineRecord::status`]), its
/// platform checked against the ones 2.0 removed, the `fix` of an outdated
/// worker forced while Sandboxes are recorded on it; with no record,
/// `native` is ready, `docker` stopped and any other `broken` `missing`.
/// What the home's tree, a Store's or Sandbox's `blocked_by` and the
/// doctor say of a machine, as `machine list` and `machine get` do.
pub fn machine_state(home: &Path, name: &str, record: Option<&MachineRecord>) -> Result<State> {
    let on = match record {
        Some(_) => sandboxes_on(home, name)?.len(),
        None => 0,
    };
    Ok(machine_state_with(name, record, on))
}

/// [`machine_state`] with the number of Sandboxes recorded on it already
/// counted.
pub fn machine_state_with(name: &str, record: Option<&MachineRecord>, on: usize) -> State {
    let Some(record) = record else {
        return layerfs_machine::state::unrecorded(name);
    };
    let platform = record.platform_name().unwrap_or("");
    let removed = layerfs_sandbox_backend::platform_policy::disabled_reason(platform).is_some();
    record.status(removed, on)
}

/// The sqlite Stores whose record names machine `machine`. A Store record
/// of a kind LayerFS does not write is refused, as before.
pub fn stores_on(home: &Path, machine: &str) -> Result<Vec<String>> {
    let mut on = Vec::new();
    for store in stores(home)? {
        if let StoreRef::Sqlite {
            machine: placed, ..
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
/// `detail.related` lists them (#536 §10b), and the next fields `stores`,
/// `sandboxes` and `machines` name each kind that holds it. Held by
/// Sandboxes alone, its `fix` removes them, then the machine; a stopped
/// machine (its record says so) gets none, since `sandbox remove` cannot
/// reach a daemon on it. A PostgreSQL Store is on no machine and holds none.
pub fn can_remove_machine(home: &Path, name: &str) -> Result<()> {
    let stores = stores_on(home, name)?;
    let sandboxes = sandboxes_on(home, name)?;
    let hosted = hosted_on(home, name)?;
    if stores.is_empty() && sandboxes.is_empty() && hosted.is_empty() {
        return Ok(());
    }
    let parts = [
        ("Stores", "stores", &stores),
        ("Sandboxes", "sandboxes", &sandboxes),
        ("old Firecracker machines", "machines", &hosted),
    ];
    let holds: Vec<String> = (parts.iter())
        .filter(|(_, _, names)| !names.is_empty())
        .map(|(kind, _, names)| format!("{kind} {}", names.join(", ")))
        .collect();
    let mut message = format!("machine {name} holds {}", holds.join("; "));
    // a stopped machine's Sandboxes cannot be removed: `sandbox remove`
    // reaches their daemons only while it runs, so no removal is offered
    let stopped = MachineRecord::read(home, name)?
        .is_some_and(|machine| machine.recorded_state() == Some("stopped"));
    if stopped && !sandboxes.is_empty() {
        message.push_str(
            "; it is stopped, and `layerfs sandbox remove` reaches a Sandbox's daemon only while it runs",
        );
    }
    let mut held = related("store", &stores);
    held.extend(related("sandbox", &sandboxes));
    held.extend(related("machine", &hosted));
    let mut refusal = in_use(message, held);
    for (_, field, names) in parts.iter().filter(|(_, _, names)| !names.is_empty()) {
        refusal = with_next(refusal, field, listed(names));
    }
    // Sandboxes alone: removing them lets the machine go, when it runs
    if stores.is_empty() && hosted.is_empty() && !stopped {
        refusal = with_fix(
            refusal,
            then(&sandboxes, &format!("layerfs machine remove {name}")),
        );
    }
    Err(refusal)
}

/// `machine.stop`: refused `InUse` while a Sandbox is recorded on machine
/// `name`; `detail.related` lists them as `{kind: "sandbox", name}`, the
/// next field `sandboxes` names them, and the `fix` removes each (a
/// `sandbox remove` refuses while Workspaces are mounted) and stops the
/// machine. Under the home lock.
pub fn can_stop_machine(home: &Path, name: &str) -> Result<()> {
    let sandboxes = sandboxes_on(home, name)?;
    if sandboxes.is_empty() {
        return Ok(());
    }
    let message = format!(
        "machine {name} runs {}: remove {} first, or stop it with --force",
        sandboxes_named(&sandboxes),
        if sandboxes.len() == 1 { "it" } else { "them" }
    );
    let refusal = in_use(message, related("sandbox", &sandboxes));
    let refusal = with_next(refusal, "sandboxes", listed(&sandboxes));
    Err(with_fix(
        refusal,
        then(&sandboxes, &format!("layerfs machine stop {name}")),
    ))
}

/// `Sandbox a` or `Sandboxes a, b`.
fn sandboxes_named(sandboxes: &[String]) -> String {
    match sandboxes {
        [one] => format!("Sandbox {one}"),
        many => format!("Sandboxes {}", many.join(", ")),
    }
}

/// `names` as a list value.
fn listed(names: &[String]) -> SdkValue {
    SdkValue::List(names.iter().map(SdkValue::text).collect())
}

/// One shell line: `layerfs sandbox remove` of each of `sandboxes`, then
/// `last` when there is one, joined by `&&`.
fn then(sandboxes: &[String], last: &str) -> String {
    let removes = sandboxes
        .iter()
        .map(|s| layerfs_sandbox::state::remove_fix(s));
    let steps: Vec<String> = removes
        .chain((!last.is_empty()).then(|| last.to_owned()))
        .collect();
    steps.join(" && ")
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
/// (a running Sandbox keeps the image and settings it started with); the
/// next field `sandboxes` names them and the `fix` removes them.
pub fn can_change_machine(home: &Path, name: &str) -> Result<()> {
    let running = sandboxes_on(home, name)?;
    if running.is_empty() {
        return Ok(());
    }
    let refusal = refused(
        "InUse",
        match running.as_slice() {
            [one] => format!("Sandbox {one} runs on machine {name}: remove it before changing it"),
            many => format!(
                "Sandboxes {} run on machine {name}: remove them before changing it",
                many.join(", ")
            ),
        },
    );
    let refusal = with_next(refusal, "sandboxes", listed(&running));
    Err(with_fix(refusal, then(&running, "")))
}

/// `project.delete`: refused `Busy` while a Sandbox of `project`
/// (`<store>/<project>`) is recorded, so a live Sandbox cannot outlive its
/// project's Store rows. The `detail` is spec §10's `{entity: {kind:
/// "project", name}, phase: null, op: null}`, and `related` lists the
/// Sandboxes as `{kind, name}`; an unreachable Sandbox is still a live user
/// and keeps holding.
pub fn can_remove_project(home: &Path, project: &str) -> Result<()> {
    let sandboxes = sandboxes_of_project(home, project)?;
    if sandboxes.is_empty() {
        return Ok(());
    }
    let message = match sandboxes.as_slice() {
        [sandbox] => format!("Sandbox {sandbox} of {project} is running"),
        many => format!("Sandboxes {} of {project} are running", many.join(", ")),
    };
    let mut refusal = refused("Busy", message);
    refusal.detail = SdkValue::map([
        (
            "entity",
            SdkValue::map([
                ("kind", SdkValue::text("project")),
                ("name", SdkValue::text(project)),
            ]),
        ),
        ("phase", SdkValue::Null),
        ("op", SdkValue::Null),
        ("related", SdkValue::List(related("sandbox", &sandboxes))),
    ]);
    Err(refusal)
}

/// `store.move`, `store.remove`, `store.disconnect` and
/// `backup.restore`: refused `InUse` while
/// another live process holds Store `store` (its lease) or a Sandbox of one
/// of its projects is recorded; `detail.related` lists them as `{kind:
/// "lease", name: <pid>}` and `{kind: "sandbox", name}`, the next field
/// `sandboxes` names the Sandboxes, and when nothing else holds it the
/// `fix` removes them. Under the home lock and the Store's lock (it
/// removes stale leases).
pub fn store_idle(home: &Path, store: &str) -> Result<()> {
    let holders = lease::holders(home, store)?;
    let sandboxes = sandboxes_of_store(home, store)?;
    let mut why = Vec::new();
    if !holders.is_empty() {
        let pids: Vec<String> = holders.iter().map(u32::to_string).collect();
        why.push(match pids.as_slice() {
            [one] => format!("Store {store} is open in process {one}"),
            many => format!("Store {store} is open in processes {}", many.join(", ")),
        });
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
    let mut refusal = in_use(why.join("; "), held);
    if !sandboxes.is_empty() {
        refusal = with_next(refusal, "sandboxes", listed(&sandboxes));
    }
    // Sandboxes alone: removing them frees the Store
    if holders.is_empty() {
        refusal = with_fix(refusal, then(&sandboxes, ""));
    }
    Err(refusal)
}
