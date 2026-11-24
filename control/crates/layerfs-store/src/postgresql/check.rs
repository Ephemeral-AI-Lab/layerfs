//! A PostgreSQL Store's connection, checked from the side that dials it.
//! A Store's Sandboxes run on any machine whose platform runs PostgreSQL
//! Sandboxes (supersedes the per-machine attachments of 2026-11-12; #536
//! C4 again): each dials the Store's address, or its own where
//! `store.connect --machine` gave it one, kept in the Store's record as
//! `machines.<machine>`, always with the Store's saved login. This module is the Store's part: the check run on
//! the side that dials, and the record writes. Which side dials, and the
//! URL a machine dials, are the caller's (`layerfs-control`, through the
//! Sandbox route).
use crate::open_args::PgSelection;
use crate::postgresql::connect::SCHEMA;
use crate::record::{OwnConnection, StoreKind, StoreRecord};
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

/// Saves `machine`'s own address of Store `name` (an empty one removes
/// it, and forgets its reach observations), under the home lock and the
/// Store's: `Conflict` when the Store was disconnected, or connected again
/// to another database (`binding` differs), since it was read. Nothing else
/// of the record changes: no URL is rewritten (a Store address that may
/// hold a login stays byte for byte), and no old field is dropped.
pub fn save_own(
    home_dir: &Path,
    name: &StoreName,
    machine: &str,
    own: &OwnConnection,
    binding: Option<&[u8]>,
) -> Result<()> {
    let _lock = home::lock::take(home_dir)?;
    let _store = crate::record::lock(home_dir, name)?;
    let record = StoreRecord::read(home_dir, name)?.filter(|r| r.binding() == binding);
    let Some(record) = record else {
        return Err(refused(
            "Conflict",
            format!("Store {name} changed while machine {machine} was checked; connect it again"),
        ));
    };
    record.with_own_connection(machine, own).write(home_dir)?;
    if own.is_empty() {
        crate::observed::forget_locked(home_dir, name, machine)?;
    }
    Ok(())
}

/// Forgets machine `machine` in every PostgreSQL Store of the home (its
/// `machine.remove`): its own connection and its reach observations, the
/// rest of each record as it was (no URL rewritten); a later machine of the
/// same name dials each Store's connection and was never seen. Under the
/// home lock and each Store's.
pub fn forget_machine(home_dir: &Path, machine: &str) -> Result<()> {
    let _lock = home::lock::take(home_dir)?;
    for record in crate::record::stores(home_dir)? {
        if record.kind().ok() != Some(StoreKind::Postgresql) {
            continue;
        }
        let _store = crate::record::lock(home_dir, record.name())?;
        if record.own_connection(machine).is_some() {
            let record = record.clone().without_own_connection(machine);
            record.write(home_dir)?;
        }
        crate::observed::forget_locked(home_dir, record.name(), machine)?;
    }
    Ok(())
}
