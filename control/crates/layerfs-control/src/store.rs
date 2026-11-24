//! The Store calls whose checks read who uses a Store (layerfs-project#105):
//! each runs `layerfs-store`'s call with the checks from
//! [`crate::inventory`]. The Store calls proper (`branch`, `commit`, `file`)
//! and the rest (`store.list`, `store.get`, `store.create`) are
//! `layerfs-store`'s own. `store.attach` and `store.detach` also read the
//! machine, and the first reaches the Store from it as a Sandbox would.
use crate::inventory;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::names::{MachineName, StoreName};
use layerfs_home::outcome::shell_word;
use layerfs_home::outcome::{refused, with_fix, Outcome, FIX};
use layerfs_machine::worker::Unreached;
use layerfs_store::place::{self, Executor};
use layerfs_store::postgresql::check;
use layerfs_store::postgresql::connect::{self, StoreConnect};
use layerfs_store::record::{StoreKind, StoreRecord};
use layerfs_store::Related;
use std::path::Path;

pub use layerfs_store::close_idle;

/// `store.compact`: reclaim one Store's dead bytes, answering the
/// flattened report (`layerfs_store::report::compact_report`). A refusal
/// gets the call that clears it as its `fix` (spec v19): starting a sqlite
/// Store's machine whose worker was not reached, or replacing a worker of
/// another build (the stop forced when Sandboxes are on the machine); the
/// same `store compact` after a lost reply or an unresolved earlier run.
pub fn compact(home: &Path, events: &mut Events<'_>, name: &StoreName) -> Outcome {
    let store = name.as_str();
    layerfs_store::compact::compact(home, events, store).map_err(|refusal| {
        if refusal
            .detail
            .get(FIX)
            .is_some_and(|fix| *fix != SdkValue::Null)
        {
            return refusal;
        }
        let machine = || {
            let record = StoreRecord::read(home, store).ok().flatten()?;
            let machine = record.machine().ok()?;
            (record.kind().ok()? == StoreKind::Sqlite && machine != "native").then_some(machine)
        };
        let fix = match refusal.code.as_str() {
            "Unavailable" => match machine() {
                Some(machine) if layerfs_machine::worker::is_mismatch(&refusal) => {
                    let on = inventory::sandboxes_on(home, &machine).map_or(0, |on| on.len());
                    Some(layerfs_machine::state::replace_worker(&machine, on))
                }
                Some(machine) => Some(format!(
                    "layerfs machine start {machine}",
                    machine = shell_word(&machine)
                )),
                None => None,
            },
            "Unknown" | "Unresolved" => Some(format!(
                "layerfs store compact {store}",
                store = shell_word(store)
            )),
            _ => None,
        };
        match fix {
            Some(fix) => with_fix(refusal, fix),
            None => refusal,
        }
    })
}

/// Who uses a Store or a project, read from the Sandbox records.
pub const RELATED: Related = Related {
    store_idle: inventory::store_idle,
    can_remove_project: inventory::can_remove_project,
};

/// A machine's records-only state as the Store reads take it
/// ([`layerfs_store::state::Machines`]): the machine family's one
/// derivation, [`inventory::machine_state`], so a Store's `blocked_by` says
/// what `machine get` says of its machine.
pub fn machine_state(home: &Path, name: &str) -> Result<layerfs_home::state::State, SdkRefusal> {
    match layerfs_machine::record::MachineRecord::read(home, name) {
        Ok(record) => inventory::machine_state(home, name, record.as_ref()),
        // a record that cannot be read blocks the Stores on it, nothing else
        Err(_) => Ok(layerfs_machine::state::unreadable(name)),
    }
}

/// What the Store reads take from the other entities: each machine's
/// state ([`machine_state`]).
pub fn reads() -> layerfs_store::state::Machines<'static> {
    &machine_state
}

/// `store.remove`: a sqlite Store no Sandbox uses, with its file.
pub fn remove(home: &Path, name: &StoreName) -> Outcome {
    layerfs_store::sqlite::remove::remove(home, name, RELATED)
}

/// `store.move`: a sqlite Store from the machine that holds it to
/// `machine`, one of the two being this computer (C13).
pub fn move_to(home: &Path, events: &mut Events<'_>, store: &str, machine: &str) -> Outcome {
    layerfs_store::sqlite::transfer::phases::move_to(home, events, store, machine, RELATED)
}

/// `store.disconnect(store)`: forget a PostgreSQL Store, its saved login
/// and its attachments (its data stays), refused `InUse` while a process
/// holds it or a Sandbox of its projects exists. A sqlite Store is
/// `InvalidInput`, naming `store remove`.
pub fn disconnect(home: &Path, name: &StoreName) -> Outcome {
    connect::disconnect(home, name, RELATED)
}

/// `store.connect(store, url, { user, password, synchronous_commit })`:
/// a new PostgreSQL Store, or a connected one's address, login or setting,
/// checked from this process before anything is saved.
pub fn connect(home: &Path, request: &StoreConnect) -> Outcome {
    connect::run(home, request)
}

/// The PostgreSQL Store `name` a machine attaches to or detaches from: a
/// sqlite Store is `InvalidInput` naming `store move` (it lives on one
/// machine).
fn attachable(home: &Path, name: &StoreName, machine: &str) -> Result<StoreRecord, SdkRefusal> {
    let record = StoreRecord::of(home, name)?;
    if record.kind()? == StoreKind::Sqlite {
        let on = record.machine().unwrap_or_default();
        return Err(refused(
            "InvalidInput",
            format!(
                "Store {name} is a sqlite Store on machine {on}; machines attach to PostgreSQL Stores only, and `layerfs store move {name} {machine}` moves it"
            ),
        ));
    }
    Ok(record)
}

/// `store.attach(store, { machine })`: lets machine `machine` run Sandboxes
/// of PostgreSQL Store `store`. The machine must exist and its platform
/// run PostgreSQL Sandboxes; the Store is checked from it with the URL and
/// login this computer saved, as its Sandboxes dial them, before it is
/// recorded in the Store's `attached` and its reach observed. Attaching an
/// attached machine checks it again (`outcome: checked`, its reach
/// recorded either way). Answers `{ ok, outcome: attached | checked,
/// store, machine, latency_ms, server_version }`.
pub fn attach(
    home: &Path,
    events: &mut Events<'_>,
    name: &StoreName,
    machine: &MachineName,
) -> Outcome {
    let machine = machine.as_str();
    let record = attachable(home, name, machine)?;
    runs_postgresql_sandboxes(home, name, machine)?;
    let attached = record.is_attached(machine);
    let checked = check_from(home, events, &record, machine).1;
    if attached || checked.is_ok() {
        let seen = layerfs_store::observed::Reach::of(&checked);
        layerfs_store::observed::record(home, name, Some(machine), &seen);
    }
    let checked = checked?;
    let new = check::attach(home, name, machine, record.binding())?;
    let outcome = if new { "attached" } else { "checked" };
    let copied = |key: &str| checked.get(key).cloned().unwrap_or_default();
    Ok(SdkValue::map([
        ("ok", SdkValue::Bool(true)),
        ("outcome", SdkValue::text(outcome)),
        ("store", SdkValue::text(name.as_str())),
        ("machine", SdkValue::text(machine)),
        ("latency_ms", copied("latency_ms")),
        ("server_version", copied("server_version")),
    ]))
}

/// `store.detach(store, { machine })`: machine `machine` no longer runs
/// Sandboxes of PostgreSQL Store `store`, and its reach is forgotten.
/// Refused `InUse` while a Sandbox of the Store's projects is on it,
/// `NotFound` when it is not attached. Answers `{ ok, outcome: detached,
/// store, machine }`.
pub fn detach(home: &Path, name: &StoreName, machine: &MachineName) -> Outcome {
    let machine = machine.as_str();
    attachable(home, name, machine)?;
    let idle = |home: &Path| inventory::store_idle_on(home, name, machine);
    check::detach(home, name, machine, &idle)?;
    Ok(SdkValue::map([
        ("ok", SdkValue::Bool(true)),
        ("outcome", SdkValue::text("detached")),
        ("store", SdkValue::text(name.as_str())),
        ("machine", SdkValue::text(machine)),
    ]))
}

/// The refusal of a call naming machine `machine` for PostgreSQL Store
/// `store` it is not attached to, with the attach as its fix.
pub fn not_attached(store: &str, machine: &str) -> SdkRefusal {
    with_fix(
        refused(
            "InvalidInput",
            format!("machine {machine} is not attached to PostgreSQL Store {store}"),
        ),
        format!(
            "layerfs store attach {} --machine {}",
            shell_word(store),
            shell_word(machine)
        ),
    )
}

/// Refused unless machine `machine` exists and its platform runs
/// PostgreSQL Sandboxes on this host (the support table's reason, word for
/// word), as `sandbox.create` refuses one.
fn runs_postgresql_sandboxes(home: &Path, store: &str, machine: &str) -> Result<(), SdkRefusal> {
    #[cfg(feature = "machine")]
    {
        use layerfs_machine::platform::{sandbox_allowed, HostOs};
        let platform = layerfs_machine::machine_of(home, machine)?.platform()?;
        sandbox_allowed(platform, HostOs::current(), StoreKind::Postgresql).map_err(|reason| {
            refused(
                "Unsupported",
                format!(
                    "a Sandbox of postgresql Store {store} cannot run on machine {machine} ({}): {reason}",
                    platform.name()
                ),
            )
        })
    }
    #[cfg(not(feature = "machine"))]
    {
        let _ = store;
        match layerfs_machine::record::MachineRecord::read(home, machine)? {
            Some(_) => Ok(()),
            None => Err(refused("NotFound", format!("no machine named {machine}"))),
        }
    }
}

/// `refusal` of the check from machine `machine`, naming it, the Store and
/// the host it dialled (never the URL), with the platform's `hint` only
/// when no connection was made (a server that answered, with a refused
/// login or another Store, was reached). A worker from a bundle without
/// the check says how to update it.
fn from_machine(
    refusal: SdkRefusal,
    (store, machine, host): (&str, &str, &str),
    hint: Option<&str>,
) -> SdkRefusal {
    let message = refusal.message;
    match refusal.code.as_str() {
        "InvalidInput" if message.contains("no operation store.reach") => refused(
            "Unsupported",
            format!(
                "machine {machine}'s worker predates this check; `layerfs machine stop {machine}` and `layerfs machine start {machine}` update it"
            ),
        ),
        "Unavailable" if !message.starts_with(check::ANOTHER_STORE) => {
            let hint = hint
                .filter(|_| layerfs_store::postgresql::open::not_connected(&message))
                .map(|hint| format!("; {hint}"))
                .unwrap_or_default();
            refused(
                "Unavailable",
                format!(
                    "machine {machine} cannot reach PostgreSQL Store {store}'s server at {host}: {message}{hint}"
                ),
            )
        }
        code => refused(
            code,
            format!("machine {machine}, PostgreSQL Store {store} at {host}: {message}"),
        ),
    }
}

/// The check of machine `machine`'s connection to PostgreSQL Store
/// `record` (the Store's URL with its login), run from that machine: in
/// this process, or in its worker as
/// `store.reach`. The URL is built as the machine's Sandboxes' calls build
/// it. Answers the check, `{ at, server_version, latency_ms }`, or its
/// refusal; either way with the host it dialled, once known.
pub(crate) fn check_from(
    home: &Path,
    events: &mut Events<'_>,
    record: &StoreRecord,
    machine: &str,
) -> (Option<String>, Result<SdkValue, SdkRefusal>) {
    let name = record.name();
    let (postgresql, hint) = match layerfs_sandbox::route::reached(home, machine, record) {
        Ok(reached) => reached,
        #[cfg(feature = "machine")]
        Err(refusal) => {
            let refusal = crate::machine::RELATED.nameless_fix(home, machine, refusal);
            return (None, Err(refusal));
        }
        #[cfg(not(feature = "machine"))]
        Err(refusal) => return (None, Err(refusal)),
    };
    // the host named is read from the address the machine dials, with no
    // login: the URL dialled holds the password
    let address = record.text("url").unwrap_or_default();
    let host = match connect::target(address) {
        Ok((host, _, _)) => host,
        Err(refusal) => return (None, Err(refusal)),
    };
    let executor = match place::on_machine(home, machine) {
        Ok(executor) => executor,
        Err(refusal) => return (Some(host), Err(refusal)),
    };
    let checked = match executor {
        Executor::Local => check::check(record, &postgresql),
        Executor::Worker(mut worker) => {
            let request = SdkValue::map([
                ("store", SdkValue::text(name)),
                ("store_record", record.to_record().clone()),
                ("postgresql", postgresql.encode()),
            ]);
            let mut sink = |event| events.event(event);
            match worker.attempt("store.reach", request, &mut sink) {
                Ok(outcome) => outcome,
                // the machine's worker was not reached: its own refusal,
                // which names the start, not a server the check never dialled
                Err(Unreached(refusal)) => return (Some(host), Err(refusal)),
            }
        }
    };
    let checked =
        checked.map_err(|refusal| from_machine(refusal, (name, machine, &host), hint.as_deref()));
    (Some(host), checked)
}

/// `store.reach` in a machine's worker: `{ store, store_record, postgresql }`
/// from the caller, which put the saved login into the URL this machine
/// dials. Answers the check `store.attach` and `store.doctor` ask for.
pub fn reach_in_worker(args: &SdkValue) -> Outcome {
    let store = args.get_text("store").unwrap_or("");
    StoreName::parse(store)?;
    let record =
        StoreRecord::from_record(store, args.get("store_record").cloned().unwrap_or_default());
    if record.kind()? != StoreKind::Postgresql {
        return Err(refused(
            "InvalidInput",
            format!("Store {store} is not a PostgreSQL Store"),
        ));
    }
    let postgresql = args
        .get("postgresql")
        .map(layerfs_store::open_args::PgSelection::read)
        .ok_or_else(|| {
            refused(
                "InvalidInput",
                "store.reach carries the resolved postgresql value",
            )
        })?;
    check::check(&record, &postgresql)
}
