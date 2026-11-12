//! A PostgreSQL Store's machines (supersedes #536 C4, takes up H9): each
//! machine that may run its Sandboxes is attached in the Store's record,
//! with the Store's connection or one of its own (a URL without a password
//! and a password reference). `store connect` attaches `native`;
//! `store attach` checks a machine's connection from that machine before
//! it records it, and `store detach` forgets it. This module is the
//! Store's part: the check run on the side that dials, and the record
//! writes. Which side dials, and the URL a machine dials, are the caller's
//! (`layerfs-control`, through the Sandbox route).
use crate::open_args::PgSelection;
use crate::postgresql::connect::SCHEMA;
use crate::record::{Attachment, StoreKind, StoreRecord};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home;
use layerfs_home::names::StoreName;
use layerfs_home::outcome::refused;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// An `InUse` check on Store `store`'s Sandboxes on machine `machine`.
pub type MachineIdle = fn(home: &Path, store: &str, machine: &str) -> Result<()>;

/// The PostgreSQL Store `name`'s record: `NotFound` when there is none,
/// `InvalidInput` naming `store push` for a sqlite Store.
pub fn postgresql_record(home: &Path, name: &str) -> Result<StoreRecord> {
    let record = StoreRecord::of(home, name)?;
    match record.kind()? {
        StoreKind::Postgresql => Ok(record),
        StoreKind::Sqlite => Err(refused(
            "InvalidInput",
            format!("Store {name} is a sqlite Store; `store push {name} <machine>` places it"),
        )),
    }
}

/// How [`check`] begins the refusal of a server that answered but holds
/// another Store (or none): reached, so no reach hint applies.
pub const ANOTHER_STORE: &str = "the PostgreSQL server holds another Store than";

/// The check of one machine's connection, run on the side that dials it:
/// the schema at `postgresql` must hold Store `record`'s binding, and the
/// role must have every privilege a Sandbox's writes need there. Answers
/// the check to record, `{ at, server_version, latency_ms }`. The server
/// not reached is `Unavailable`, a login refused or a privilege missing
/// `Denied`, another Store's schema `Unavailable` naming it, and a build
/// without the PostgreSQL engine `Unsupported`.
pub fn check(record: &StoreRecord, postgresql: &PgSelection) -> Result<SdkValue> {
    #[cfg(feature = "postgresql")]
    {
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
    #[cfg(not(feature = "postgresql"))]
    {
        let _ = (record, postgresql, SCHEMA);
        Err(refused(
            "Unsupported",
            "this LayerFS build has no PostgreSQL engine; build the binding with --features postgresql",
        ))
    }
}

/// Records Store `name`'s attachment to `machine` once its check passed,
/// under the home lock and the Store's: `Conflict` when the Store was
/// disconnected, or connected again to another database (`binding`
/// differs), since the check.
pub fn record(
    home_dir: &Path,
    name: &StoreName,
    machine: &str,
    attachment: &Attachment,
    binding: Option<&[u8]>,
) -> Result<()> {
    let _lock = home::lock::take(home_dir)?;
    let _store = crate::record::lock(home_dir, name)?;
    let record = StoreRecord::read(home_dir, name)?.filter(|r| r.binding() == binding);
    let Some(record) = record else {
        return Err(refused(
            "Conflict",
            format!("Store {name} changed while machine {machine} was checked; attach it again"),
        ));
    };
    record.with_attachment(machine, attachment).write(home_dir)
}

/// `store.detach`: forgets Store `name`'s attachment to `machine`, under
/// the home lock and the Store's. `InvalidInput` when it is not attached
/// there; `InUse` (from `idle`) while a Sandbox of the Store is recorded on
/// that machine. Its data and its other machines stay.
pub fn detach(home_dir: &Path, name: &StoreName, machine: &str, idle: MachineIdle) -> Result<()> {
    let _lock = home::lock::take(home_dir)?;
    let _store = crate::record::lock(home_dir, name)?;
    let record = postgresql_record(home_dir, name)?;
    if record.attachment(machine).is_none() {
        return Err(refused(
            "InvalidInput",
            format!("Store {name} is not attached to machine {machine}"),
        ));
    }
    idle(home_dir, name, machine)?;
    record.without_attachment(machine).write(home_dir)
}
