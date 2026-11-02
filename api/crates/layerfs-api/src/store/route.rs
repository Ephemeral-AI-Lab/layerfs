//! The route of one Store call, chosen from records (H §7.7): a PostgreSQL
//! Store and a sqlite Store on `native` run in this process; a sqlite Store
//! on any other machine runs in that machine's worker, started first if it
//! is not running. The caller sends the worker the Store's record with the
//! call, so a guest never reads the home; the worker finds the Store file
//! under its own root.
use crate::calls::Context;
use crate::home::{paths, read_record};
use crate::host::outcome::{refused, Outcome};
use crate::store::store_ref::StoreRef;
use crate::store::{lease, open};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::Path;

/// The Store and project a call names, with their records.
pub(crate) struct Target {
    /// The Store's name.
    pub store: String,
    /// The project's name in its Store.
    pub project: String,
    /// `store/<store>/record.json`.
    pub record: SdkValue,
    /// The record, parsed once.
    pub store_ref: StoreRef,
}

/// Splits `<store>/<project>` and reads both records: no Store is
/// `NoSuchStore`, no project `NoSuchProject` (1.1.0's codes, S §2.9).
pub(crate) fn target(home: &Path, project: &str) -> Result<Target, SdkRefusal> {
    let (store, name) = project.split_once('/').ok_or_else(|| {
        refused(
            "InvalidInput",
            format!("project `{project}` must be written with its Store"),
        )
    })?;
    let record = read_record(&paths::store_record(home, store))?
        .ok_or_else(|| refused("NoSuchStore", format!("no Store named {store}")))?;
    if read_record(&paths::project_record(home, store, name))?.is_none() {
        return Err(refused("NoSuchProject", format!("no project {project}")));
    }
    Ok(Target {
        store: store.to_owned(),
        project: name.to_owned(),
        store_ref: StoreRef::from_record(store, &record)?,
        record,
    })
}

/// The machine a sqlite Store's record names; `native` for a Store with no
/// machine field, refused for one the 1.1.0 conversion could not place.
pub(crate) fn machine(record: &SdkValue, store: &str) -> Result<String, SdkRefusal> {
    StoreRef::from_record(store, record)?
        .machine()
        .map(str::to_owned)
}

/// Refuses `InUse` while a live process moves or removes the Store (its
/// record's `in_progress`), this process included: a move's own steps take
/// no lease and make no Store call, so nothing else in its process may
/// either. A move whose process is gone does not block.
pub(crate) fn moving(record: &SdkValue, store: &str) -> Result<(), SdkRefusal> {
    let Some(progress @ SdkValue::Map(_)) = record.get("in_progress") else {
        return Ok(());
    };
    if lease::recorded_alive(progress) {
        let pid = progress.get_int("pid").unwrap_or(0);
        let doing = match progress.get_text("op") {
            Some("remove") => "removed",
            _ => "moved",
        };
        return Err(refused(
            "InUse",
            format!("Store {store} is being {doing} by process {pid}"),
        ));
    }
    Ok(())
}

/// Runs Store method `op` with `args` on the Store of `target`.
pub(crate) fn store_call(
    context: &mut Context<'_>,
    target: &Target,
    op: &str,
    args: Vec<(String, SdkValue)>,
) -> Outcome {
    let home = context.home.clone();
    moving(&target.record, &target.store)?;
    if let StoreRef::Postgres { .. } = target.store_ref {
        let fields = with_target(args, target, &target.record);
        return postgresql(&home, target, op, fields);
    }
    // the lease is taken under the home lock with the record read there: a
    // move or remove that started since `target` was read refuses it
    let (lease, record) = lease::claim(&home, &target.store)?;
    let mut fields = with_target(args, target, &record);
    let machine = machine(&record, &target.store)?;
    match endpoint(&home, &machine)? {
        None => {
            let file = paths::store_file(&home, &target.store);
            fields.push((
                "storePath".to_owned(),
                SdkValue::text(file.display().to_string()),
            ));
            open::run(op, SdkValue::Map(fields), Some(lease))
        }
        Some(endpoint) => {
            // the host process holds the lease while it sends the call
            let _lease = lease;
            let request =
                SdkValue::map([("op", SdkValue::text(op)), ("args", SdkValue::Map(fields))]);
            crate::worker::call(&endpoint, "store.call", request, &mut |event| {
                context.event(event)
            })
        }
    }
}

/// A call's arguments with its project, Store and Store record.
fn with_target(
    args: Vec<(String, SdkValue)>,
    target: &Target,
    record: &SdkValue,
) -> Vec<(String, SdkValue)> {
    let mut fields = args;
    fields.extend([
        ("project".to_owned(), SdkValue::text(&target.project)),
        ("store".to_owned(), SdkValue::text(&target.store)),
        ("store_record".to_owned(), record.clone()),
    ]);
    fields
}

/// A PostgreSQL Store's call in this process: its URL from the record, the
/// password reference resolved now (H §7.4).
fn postgresql(
    home: &Path,
    target: &Target,
    op: &str,
    mut fields: Vec<(String, SdkValue)>,
) -> Outcome {
    let url = crate::family::postgres::url(&target.record, &target.store)?;
    fields.push((
        "postgresql".to_owned(),
        SdkValue::map([("url", SdkValue::text(url))]),
    ));
    // one open service per PostgreSQL Store, keyed by a path no file has
    let key = paths::store_dir(home, &target.store).join("postgresql");
    fields.push((
        "storePath".to_owned(),
        SdkValue::text(key.display().to_string()),
    ));
    open::run(op, SdkValue::Map(fields), None)
}

/// The worker of `machine`, started first if it is stopped, or `None` when
/// its Stores open in this process (H §7.2, step 6).
#[cfg(feature = "machine")]
pub(crate) fn endpoint(
    home: &Path,
    machine: &str,
) -> Result<Option<crate::worker::Endpoint>, SdkRefusal> {
    crate::machine::running(home, machine)
}

/// Without the machine adapters (the guest worker's build): `native` is in
/// process; any other machine's worker is the one its `worker.json` records.
#[cfg(not(feature = "machine"))]
pub(crate) fn endpoint(
    home: &Path,
    machine: &str,
) -> Result<Option<crate::worker::Endpoint>, SdkRefusal> {
    if machine == "native" {
        return Ok(None);
    }
    recorded(home, machine)
}

/// The worker of `machine` as its records say now, starting nothing: `None`
/// when its Stores open in this process, `Unavailable` when it has no
/// worker recorded. A recorded worker that is gone fails when called.
pub(crate) fn endpoint_now(
    home: &Path,
    machine: &str,
) -> Result<Option<crate::worker::Endpoint>, SdkRefusal> {
    if machine == "native" {
        return Ok(None);
    }
    #[cfg(feature = "machine")]
    {
        use layerfs_machine::{HostOs, Platform, Route};
        let record = read_record(&paths::machine_record(home, machine))?
            .ok_or_else(|| refused("NotFound", format!("no machine named {machine}")))?;
        let kind = record.get_text("platform").and_then(Platform::parse);
        if kind.map(|kind| kind.route(HostOs::current())) == Some(Route::InProcess) {
            return Ok(None);
        }
    }
    recorded(home, machine)
}

/// The worker `machine`'s `worker.json` records.
fn recorded(home: &Path, machine: &str) -> Result<Option<crate::worker::Endpoint>, SdkRefusal> {
    let Some(value) = read_record(&paths::worker_record(home, machine))? else {
        return Err(refused(
            "Unavailable",
            format!("machine {machine} is not running"),
        ));
    };
    let address = value
        .get_text("address")
        .and_then(|a| a.parse().ok())
        .ok_or_else(|| {
            refused(
                "Failed",
                format!("machine {machine}'s worker.json has no address"),
            )
        })?;
    Ok(Some(crate::worker::Endpoint {
        address,
        secret: value.get_text("secret").unwrap_or("").to_owned(),
    }))
}
