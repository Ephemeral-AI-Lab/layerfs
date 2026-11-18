//! The Store calls whose checks read who uses a Store (layerfs-project#105):
//! each runs `layerfs-store`'s call with the checks from
//! [`crate::inventory`]. The Store calls proper (`branch`, `commit`, `file`)
//! and the rest (`store.list`, `store.get`, `store.create`) are
//! `layerfs-store`'s own. `store.connect --machine` and
//! `store.disconnect --machine` also read the machine, and the first
//! reaches it as a Sandbox call would.
use crate::inventory;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::names::{MachineName, StoreName};
use layerfs_home::outcome::{refused, with_fix, Outcome, FIX};
use layerfs_machine::worker::Unreached;
use layerfs_store::place::{self, Executor};
use layerfs_store::postgresql::check;
use layerfs_store::postgresql::connect::{self, StoreConnect};
use layerfs_store::record::{OwnConnection, StoreKind, StoreRecord};
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
                Some(machine) => Some(format!("layerfs machine start {machine}")),
                None => None,
            },
            "Unknown" | "Unresolved" => Some(format!("layerfs store compact {store}")),
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
    let record = layerfs_machine::record::MachineRecord::read(home, name)?;
    inventory::machine_state(home, name, record.as_ref())
}

/// `store.remove`: a sqlite Store no Sandbox uses, with its file.
pub fn remove(home: &Path, name: &StoreName) -> Outcome {
    layerfs_store::sqlite::family::remove(home, name, RELATED)
}

/// `store.move`: a sqlite Store from the machine that holds it to
/// `machine`, one of the two being this computer (C13).
pub fn move_to(home: &Path, events: &mut Events<'_>, store: &str, machine: &str) -> Outcome {
    layerfs_store::sqlite::family::move_to(home, events, store, machine, RELATED)
}

/// `store.disconnect(store, { machine })`: without `machine`, forget a
/// PostgreSQL Store (its data stays), refused `InUse` while a process
/// holds it or a Sandbox of its projects exists; with it, remove that
/// machine's own address, after which it dials the Store's again
/// (`NotFound` when it has none). A sqlite Store is `InvalidInput`, naming
/// `store remove`.
pub fn disconnect(home: &Path, name: &StoreName, machine: Option<&MachineName>) -> Outcome {
    let Some(machine) = machine.map(|m| m.as_str()) else {
        return connect::disconnect(home, name, RELATED);
    };
    let record = check::postgresql_record(home, name)?;
    if record.own_connection(machine).is_none() {
        return Err(refused(
            "NotFound",
            format!("machine {machine} has no address of its own for Store {name}"),
        ));
    }
    check::save_own(
        home,
        name,
        machine,
        &OwnConnection::default(),
        record.binding(),
    )?;
    Ok(connect::disconnected(name, Some(machine)))
}

/// `store.connect(store, url, { auth, machine, synchronous_commit })`. A
/// Store not connected yet is connected from this process (`--auth`
/// required, `--machine` refused). On a connected Store the URL becomes
/// the address every machine dials, proven from this process, or with
/// `machine` that machine's own, proven from that machine; either refuses
/// `--auth` and `--synchronous-commit`. A sqlite Store of the name is
/// `Exists`.
pub fn connect(
    home: &Path,
    events: &mut Events<'_>,
    request: &StoreConnect,
    machine: Option<&MachineName>,
) -> Outcome {
    let name = request.store.as_str();
    let Some(record) = StoreRecord::read(home, name)? else {
        if let Some(machine) = machine {
            return Err(refused(
                "InvalidInput",
                format!(
                    "Store {name} is not connected: connect it first without --machine, then `layerfs store connect {name} <url> --machine {machine}`"
                ),
            ));
        }
        return connect::connect(home, request);
    };
    if record.kind()? != StoreKind::Postgresql {
        return Err(refused(
            "Exists",
            format!("a sqlite Store named {name} exists"),
        ));
    }
    let Some(machine) = machine.map(|m| m.as_str()) else {
        return connect::readdress(home, request);
    };
    if request.auth.is_some() {
        let uses = record.text("auth").unwrap_or("no saved login");
        return Err(refused(
            "InvalidInput",
            format!("a machine's own connection takes no --auth; {name} uses {uses}"),
        ));
    }
    connect::refuse_settings(name, request)?;
    connect::refuse_login(&request.url)?;
    connect::target(&request.url)?;
    runs_postgresql_sandboxes(home, name, machine)?;
    // the machine's whole own connection: a password reference an older
    // entry held goes with it
    let own = OwnConnection {
        url: Some(request.url.clone()),
        password: None,
    };
    let trial = record.clone().with_own_connection(machine, &own);
    check_from(home, events, &trial, machine).1?;
    check::save_own(home, &request.store, machine, &own, record.binding())?;
    let saved = StoreRecord::of(home, name)?;
    Ok(connect::answer(&saved, "updated", None))
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
/// the host it dialled (never the URL), with the platform's `hint` when
/// the server was not reached (a server that answered with another Store
/// was reached). A worker from a bundle without the check says how to
/// update it.
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
            let hint = hint.map(|hint| format!("; {hint}")).unwrap_or_default();
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
/// `record` (its own connection where the record names one, else the
/// Store's), run from that machine: in this process, or in its worker as
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
        Err(refusal) => return (None, Err(refusal)),
    };
    let host = match postgresql.url().and_then(connect::target) {
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
    let checked = checked.map_err(|refusal| from_machine(refusal, (name, machine, &host), hint));
    (Some(host), checked)
}

/// `store.reach` in a machine's worker: `{ store, store_record, postgresql }`
/// from the caller, which put the saved login into the URL this machine
/// dials. Answers the check `store.connect --machine` and `store.doctor`
/// ask for.
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
