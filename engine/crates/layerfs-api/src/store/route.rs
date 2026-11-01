//! The route of one Store call, chosen from records (H §7.7): a PostgreSQL
//! Store and a sqlite Store on `native` run in this process; a sqlite Store
//! on any other machine runs in that machine's worker, started first if it
//! is not running. The caller sends the worker the Store's record with the
//! call, so a guest never reads the home; the worker finds the Store file
//! under its own root.
use crate::calls::Context;
use crate::home::{paths, read_record};
use crate::host::outcome::{refused, Outcome};
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
}

/// Splits `<store>/<project>` and reads both records.
pub(crate) fn target(home: &Path, project: &str) -> Result<Target, SdkRefusal> {
    let (store, name) = project.split_once('/').ok_or_else(|| {
        refused(
            "InvalidInput",
            format!("project `{project}` must be written with its Store"),
        )
    })?;
    let record = read_record(&paths::store_record(home, store))?
        .ok_or_else(|| refused("NotFound", format!("no Store named {store}")))?;
    if read_record(&paths::project_record(home, store, name))?.is_none() {
        return Err(refused("NotFound", format!("no project {project}")));
    }
    Ok(Target {
        store: store.to_owned(),
        project: name.to_owned(),
        record,
    })
}

/// The machine a sqlite Store's record names; `native` for a Store with no
/// machine field, refused for one the 1.1.0 conversion could not place.
pub(crate) fn machine(record: &SdkValue, store: &str) -> Result<String, SdkRefusal> {
    match record.get("machine") {
        None => Ok("native".to_owned()),
        Some(SdkValue::Text(machine)) => Ok(machine.clone()),
        Some(_) => Err(refused(
            "Unavailable",
            format!(
                "Store {store} came from a 1.1.0 home and its machine is not known; \
                 run `layerfs store import-env` or `layerfs store move {store} <machine>`"
            ),
        )),
    }
}

/// Refuses `InUse` while a live process moves the Store (its record's
/// `in_progress`); a move whose process is gone does not block.
pub(crate) fn moving(record: &SdkValue, store: &str) -> Result<(), SdkRefusal> {
    let Some(progress @ SdkValue::Map(_)) = record.get("in_progress") else {
        return Ok(());
    };
    let pid = progress.get_int("pid").unwrap_or(0) as u32;
    if pid != 0 && pid != std::process::id() && lease::alive(pid) {
        return Err(refused(
            "InUse",
            format!("Store {store} is being moved by process {pid}"),
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
    let mut fields = args;
    fields.extend([
        ("project".to_owned(), SdkValue::text(&target.project)),
        ("store".to_owned(), SdkValue::text(&target.store)),
        ("store_record".to_owned(), target.record.clone()),
    ]);
    moving(&target.record, &target.store)?;
    if target.record.get_text("kind") == Some("postgresql") {
        return postgresql(&home, target, op, fields);
    }
    let machine = machine(&target.record, &target.store)?;
    match endpoint(&home, &machine)? {
        None => {
            let file = paths::store_file(&home, &target.store);
            fields.push((
                "storePath".to_owned(),
                SdkValue::text(file.display().to_string()),
            ));
            let lease = lease::hold(&home, &target.store)?;
            open::run(op, SdkValue::Map(fields), Some(lease))
        }
        Some(endpoint) => {
            // the host process holds the lease while it sends the call
            let _lease = lease::hold(&home, &target.store)?;
            let request =
                SdkValue::map([("op", SdkValue::text(op)), ("args", SdkValue::Map(fields))]);
            crate::worker::call(&endpoint, "store.call", request, &mut |event| {
                context.event(event)
            })
        }
    }
}

/// A PostgreSQL Store's call in this process: its URL from the record's
/// `url_env` variable (H §7.4; the URL itself is never written down).
fn postgresql(
    home: &Path,
    target: &Target,
    op: &str,
    mut fields: Vec<(String, SdkValue)>,
) -> Outcome {
    let url = crate::store::route::postgresql_url(&target.record, &target.store)?;
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

/// The URL of a PostgreSQL Store: the environment variable its record names.
pub(crate) fn postgresql_url(record: &SdkValue, store: &str) -> Result<String, SdkRefusal> {
    let name = record
        .get_text("url_env")
        .unwrap_or("LAYERFS_POSTGRESQL_URL");
    match std::env::var(name) {
        Ok(url) if !url.is_empty() => Ok(url),
        _ => Err(refused(
            "Unavailable",
            format!("PostgreSQL Store {store} reads its URL from {name}, which is not set"),
        )),
    }
}

/// The worker of `machine`, or `None` when its Stores open in this
/// process: `native` is in process; any other machine's worker is the one
/// its `worker.json` records.
pub(crate) fn endpoint(
    home: &Path,
    machine: &str,
) -> Result<Option<crate::worker::Endpoint>, SdkRefusal> {
    if machine == "native" {
        return Ok(None);
    }
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
