//! A PostgreSQL Store's connection, checked from the side that dials it
//! (owner, 2026-11-24): the host saves a Store's URL and login, and a
//! machine attached to the Store (`store.attach`, its record's
//! `attached`) runs its Sandboxes, dialling that URL with that login; no
//! machine has an address of its own. This module is the Store's part: the
//! check run on the side that dials, and the attachment writes. Which side
//! dials is the caller's (`layerfs-control`, through the Sandbox route).
use crate::open_args::PgSelection;
use crate::postgresql::connect::SCHEMA;
use crate::record::{StoreKind, StoreRecord};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home;
use layerfs_home::names::StoreName;
use layerfs_home::outcome::refused;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The PostgreSQL Store `name`'s record: `NotFound` when there is none,
/// `InvalidInput` for a sqlite Store, which has no connection.
pub fn postgresql_record(home: &Path, name: &str) -> Result<StoreRecord> {
    let record = StoreRecord::of(home, name)?;
    match record.kind()? {
        StoreKind::Postgresql => Ok(record),
        StoreKind::Sqlite => Err(refused(
            "InvalidInput",
            format!(
                "Store {name} is a sqlite Store, which has no connection; `layerfs store remove {name}` deletes it"
            ),
        )),
    }
}

/// How [`check`] begins the refusal of a server that answered but holds
/// another Store (or none): reached, so no reach hint applies.
pub const ANOTHER_STORE: &str = "the PostgreSQL server holds another Store than";

/// The check of one machine's connection, run on the side that dials it:
/// the schema at `postgresql` must hold Store `record`'s binding, and the
/// role must have every privilege a Sandbox's writes need there. Answers
/// the check, `{ at, server_version, latency_ms }`, which its caller
/// records ([`crate::observed`]). The server
/// not reached is `Unavailable`, a login refused or a privilege missing
/// `Denied`, and another Store's schema `Unavailable` naming it.
pub fn check(record: &StoreRecord, postgresql: &PgSelection) -> Result<SdkValue> {
    use layerfs_persistence::PostgresStatusError as E;
    let access =
        layerfs_persistence::postgresql_access(postgresql.url()?, SCHEMA).map_err(|error| {
            match error {
                E::Denied(_) => refused("Denied", error.to_string()),
                E::Unreachable(_) => refused("Unavailable", error.to_string()),
                E::Failed(_) => refused("Failed", error.to_string()),
            }
        })?;
    let another = || refused("Unavailable", format!("{ANOTHER_STORE} {}", record.name()));
    let denied = |first: &str| {
        refused(
            "Denied",
            format!(
                "role {} lacks {first} ({} privilege(s) missing in all)",
                access.role,
                access.missing.len()
            ),
        )
    };
    if !access.schema {
        return Err(another());
    }
    // a role that may not use the schema cannot read its binding either
    if let Some(first) = access.missing.first().filter(|m| m.contains(" on schema ")) {
        return Err(denied(first));
    }
    let binding = crate::postgresql::open::read_binding(postgresql)?;
    if record.binding() != Some(binding.as_slice()) {
        return Err(another());
    }
    if let Some(first) = access.missing.first() {
        return Err(denied(first));
    }
    Ok(SdkValue::map([
        ("at", SdkValue::Int(home::now_ms())),
        ("server_version", SdkValue::text(access.server_version)),
        (
            "latency_ms",
            SdkValue::Int(i64::try_from(access.latency.as_millis()).unwrap_or(i64::MAX)),
        ),
    ]))
}

/// The light check of a connection the Store's own calls use: the server
/// at `postgresql` is reached and its schema holds Store `record`'s
/// binding. A role that may only read passes, as it does `store.connect`.
pub fn same_store(record: &StoreRecord, postgresql: &PgSelection) -> Result<()> {
    let binding = crate::postgresql::open::read_binding(postgresql)?;
    if record.binding() != Some(binding.as_slice()) {
        return Err(refused(
            "Unavailable",
            format!("{ANOTHER_STORE} {}", record.name()),
        ));
    }
    Ok(())
}

/// The record of Store `name` under the home lock and the Store's, read
/// again: `Conflict` when the Store was disconnected, or connected again to
/// another database (`binding` differs), since it was read.
fn current(
    home_dir: &Path,
    name: &str,
    binding: Option<&[u8]>,
    doing: &str,
) -> Result<StoreRecord> {
    let record = StoreRecord::read(home_dir, name)?.filter(|r| r.binding() == binding);
    record.ok_or_else(|| {
        refused(
            "Conflict",
            format!("Store {name} changed while {doing}; run the call again"),
        )
    })
}

/// Attaches `machine` to Store `name`, checked meanwhile, under the home
/// lock and the Store's: whether it was not attached before. Nothing else
/// of the record changes.
pub fn attach(
    home_dir: &Path,
    name: &StoreName,
    machine: &str,
    binding: Option<&[u8]>,
) -> Result<bool> {
    let _lock = home::lock::take(home_dir)?;
    let _store = crate::record::lock(home_dir, name)?;
    let doing = format!("machine {machine} was checked");
    let record = current(home_dir, name, binding, &doing)?;
    if record.is_attached(machine) {
        return Ok(false);
    }
    record.with_attached(machine, true).write(home_dir)?;
    Ok(true)
}

/// Detaches `machine` from Store `name` and forgets its reach, under the
/// home lock and the Store's, once `idle` (run under both: no Sandbox of
/// the Store is on the machine) allows it. `NotFound` when it is not
/// attached.
pub fn detach(
    home_dir: &Path,
    name: &StoreName,
    machine: &str,
    idle: &dyn Fn(&Path) -> Result<()>,
) -> Result<()> {
    let _lock = home::lock::take(home_dir)?;
    let _store = crate::record::lock(home_dir, name)?;
    let record = StoreRecord::of(home_dir, name)?;
    if !record.is_attached(machine) {
        return Err(refused(
            "NotFound",
            format!("machine {machine} is not attached to Store {name}"),
        ));
    }
    idle(home_dir)?;
    record.with_attached(machine, false).write(home_dir)?;
    crate::observed::forget_locked(home_dir, name, machine)?;
    Ok(())
}

/// Forgets machine `machine` in every PostgreSQL Store of the home (its
/// `machine.remove`): its attachment and its reach observations, the rest
/// of each record as it was; a later machine of the same name is attached
/// to none. Under the home lock and each Store's.
pub fn forget_machine(home_dir: &Path, machine: &str) -> Result<()> {
    let _lock = home::lock::take(home_dir)?;
    for record in crate::record::stores(home_dir)? {
        if record.kind().ok() != Some(StoreKind::Postgresql) {
            continue;
        }
        let _store = crate::record::lock(home_dir, record.name())?;
        if record.is_attached(machine) {
            let record = record.clone().with_attached(machine, false);
            record.write(home_dir)?;
        }
        crate::observed::forget_locked(home_dir, record.name(), machine)?;
    }
    Ok(())
}
