//! A PostgreSQL Store's connection, checked from the side that dials it
//! (owner, 2026-11-24): the host saves a Store's URL and login, and a
//! machine attached to the Store (`store.attach`, its record's
//! `attached`) runs its Sandboxes, dialling that URL with that login; no
//! machine has an address of its own. This module is the Store's part: the
//! check run on the side that dials, and the attachment writes. Which side
//! dials is the caller's (`layerfs-control`, through the Sandbox route).
use layerfs_persistence::diagnostic::ConnectionStep;
use crate::open_args::PgSelection;
use crate::postgresql::connect::SCHEMA;
use {layerfs_protocol::StoreKind, crate::record::StoreRecord};
use layerfs_protocol::provision::install::{SdkRefusal, SdkValue};
use layerfs_home::home;
use layerfs_home::names::StoreName;
use layerfs_home::outcome::{public_code, refused, with_next, NEXT};
use layerfs_persistence::PostgresAccess;
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

/// The key a reach refusal names the connection step it stopped at by
/// (one of [`ConnectionStep`]'s words), answered flat beside `error` and
/// `message`, as the doctor's `store.connection.<step>`.
pub const STEP: &str = "step";

/// `refusal` naming the connection step it stopped at.
pub fn at_step(refusal: SdkRefusal, step: ConnectionStep) -> SdkRefusal {
    with_next(refusal, STEP, SdkValue::text(step.name()))
}

/// The step a reach refusal names, when it names one.
pub fn step_of(refusal: &SdkRefusal) -> Option<&str> {
    refusal
        .detail
        .get(NEXT)
        .and_then(|next| next.get_text(STEP))
}

/// A check as a dry run answers it: `{ ok: true, latency_ms,
/// server_version }`, or `{ ok: false, step, code, message }`, `code` the
/// public refusal code and `step` null when nothing was dialled.
pub fn tested(checked: &Result<SdkValue>) -> SdkValue {
    match checked {
        Ok(answer) => {
            let copied = |key: &str| answer.get(key).cloned().unwrap_or_default();
            SdkValue::map([
                ("ok", SdkValue::Bool(true)),
                ("latency_ms", copied("latency_ms")),
                ("server_version", copied("server_version")),
            ])
        }
        Err(refusal) => SdkValue::map([
            ("ok", SdkValue::Bool(false)),
            (
                "step",
                SdkValue::opt_text(step_of(refusal).map(str::to_owned)),
            ),
            ("code", SdkValue::text(public_code(&refusal.code))),
            ("message", SdkValue::text(&refusal.message)),
        ]),
    }
}

/// One dial of the server at `postgresql`: what its role may do in the
/// Store's schema, or the refusal naming the step the connection stopped
/// at, as the doctor's connection check names it. The server not reached
/// is `Unavailable`, a login or database refused `Denied`.
fn access(postgresql: &PgSelection) -> Result<PostgresAccess> {
    use layerfs_persistence::PostgresStatusError as E;
    let url =
        (postgresql.url()).map_err(|refusal| at_step(refusal, ConnectionStep::Configuration))?;
    layerfs_persistence::postgresql_access(url, SCHEMA).map_err(|error| {
        let code = match error {
            E::Denied(..) => "Denied",
            E::Unreachable(..) => "Unavailable",
            E::Failed(_) => "Failed",
        };
        at_step(refused(code, error.to_string()), error.step())
    })
}

/// What a passed check answers: `{ at, server_version, latency_ms }`.
fn reached(access: &PostgresAccess) -> SdkValue {
    SdkValue::map([
        ("at", SdkValue::Int(home::now_ms())),
        ("server_version", SdkValue::text(&access.server_version)),
        (
            "latency_ms",
            SdkValue::Int(i64::try_from(access.latency.as_millis()).unwrap_or(i64::MAX)),
        ),
    ])
}

/// The binding the Store schema at `postgresql` holds, `None` when the
/// database has no such schema. A read that fails is the identity's step,
/// or the schema's when the role may not use the schema.
fn binding_of(access: &PostgresAccess, postgresql: &PgSelection) -> Result<Option<Vec<u8>>> {
    if !access.schema {
        return Ok(None);
    }
    let usage = format!("USAGE on schema {SCHEMA}");
    let step = match access.missing.contains(&usage) {
        true => ConnectionStep::Schema,
        false => ConnectionStep::Identity,
    };
    let binding = crate::postgresql::open::read_binding(postgresql);
    binding.map(Some).map_err(|refusal| at_step(refusal, step))
}

/// The refusal of a database that holds another Store than `record`, or
/// none (the schema's step when it has no Store schema, else the
/// identity's).
fn another(record: &StoreRecord, step: ConnectionStep) -> SdkRefusal {
    let message = format!("{ANOTHER_STORE} {}", record.name());
    at_step(refused("Unavailable", message), step)
}

/// The check of one machine's connection, run on the side that dials it:
/// the schema at `postgresql` must hold Store `record`'s binding, and the
/// role must have every privilege a Sandbox's writes need there. Answers
/// the check, `{ at, server_version, latency_ms }`, which its caller
/// records ([`crate::observed`]). The server not reached is
/// `Unavailable`, a login refused or a privilege missing `Denied`, and
/// another Store's schema `Unavailable` naming it; each refusal names its
/// step ([`STEP`]).
pub fn check(record: &StoreRecord, postgresql: &PgSelection) -> Result<SdkValue> {
    let access = access(postgresql)?;
    let denied = |first: &str| {
        let message = format!(
            "role {} lacks {first} ({} privilege(s) missing in all)",
            access.role,
            access.missing.len()
        );
        at_step(refused("Denied", message), ConnectionStep::Schema)
    };
    if !access.schema {
        return Err(another(record, ConnectionStep::Schema));
    }
    // a role that may not use the schema cannot read its binding either
    if let Some(first) = access.missing.first().filter(|m| m.contains(" on schema ")) {
        return Err(denied(first));
    }
    let binding = binding_of(&access, postgresql)?;
    if record.binding() != binding.as_deref() {
        return Err(another(record, ConnectionStep::Identity));
    }
    if let Some(first) = access.missing.first() {
        return Err(denied(first));
    }
    Ok(reached(&access))
}

/// The light check of a connection the Store's own calls use (this
/// computer's, which `store.connect` makes): the server at `postgresql` is
/// reached and its schema holds Store `record`'s binding. A role that may
/// only read passes. Answers as [`check`] does, each refusal naming its
/// step.
pub fn same_store(record: &StoreRecord, postgresql: &PgSelection) -> Result<SdkValue> {
    let access = access(postgresql)?;
    let binding = binding_of(&access, postgresql)?;
    let binding = binding.ok_or_else(|| another(record, ConnectionStep::Schema))?;
    if record.binding() != Some(binding.as_slice()) {
        return Err(another(record, ConnectionStep::Identity));
    }
    Ok(reached(&access))
}

/// The check of the database a Store's first connect names, from this
/// computer: answers as [`check`] does, with the binding its Store schema
/// holds (`None`: an empty database, which the connect gives the schema).
pub fn first(postgresql: &PgSelection) -> Result<(SdkValue, Option<Vec<u8>>)> {
    let access = access(postgresql)?;
    let binding = binding_of(&access, postgresql)?;
    Ok((reached(&access), binding))
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
