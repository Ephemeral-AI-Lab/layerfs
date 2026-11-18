//! The Store calls whose checks read who uses a Store (layerfs-project#105):
//! each runs `layerfs-store`'s call with the checks from
//! [`crate::inventory`]. The Store calls proper (`branch`, `commit`, `file`)
//! and the rest are `layerfs-store`'s own. `store.connect --machine`,
//! `store.disconnect --machine` and `store.status --machine` also read the
//! machine and reach it as a Sandbox call would.
use crate::inventory;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::names::{MachineName, StoreName};
use layerfs_home::outcome::{refused, Outcome};
use layerfs_machine::worker::Unreached;
use layerfs_store::place::{self, Executor};
use layerfs_store::postgresql::check;
use layerfs_store::postgresql::connect::{self, StoreConnect};
use layerfs_store::record::{OwnConnection, StoreKind, StoreRecord};
use layerfs_store::sqlite::family::StorePush;
use layerfs_store::Related;
use std::path::Path;

pub use layerfs_store::close_idle;

/// `store.compact`: reclaim one Store's dead bytes, answering the
/// §11 report.
pub fn compact(home: &Path, events: &mut Events<'_>, name: &StoreName) -> Outcome {
    layerfs_store::compact::compact(home, events, name.as_str())
}

/// Who uses a Store or a project, read from the Sandbox records.
pub const RELATED: Related = Related {
    store_idle: inventory::store_idle,
    can_remove_project: inventory::can_remove_project,
};

/// `store.remove`: a sqlite Store no Sandbox uses, with its file.
pub fn remove(home: &Path, name: &StoreName) -> Outcome {
    layerfs_store::sqlite::family::remove(home, name, RELATED)
}

/// `store.push`: a sqlite Store from native to a machine.
pub fn push(home: &Path, events: &mut Events<'_>, request: &StorePush) -> Outcome {
    layerfs_store::sqlite::family::push(home, events, request, RELATED)
}

/// `store.pull`: a sqlite Store from its machine to native.
pub fn pull(home: &Path, events: &mut Events<'_>, name: &str) -> Outcome {
    layerfs_store::sqlite::family::pull(home, events, name, RELATED)
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
    let own = OwnConnection {
        url: Some(request.url.clone()),
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

/// `store.status`: a Store's live state; with `machine`, also the check of
/// that machine's connection to a PostgreSQL Store, run from the machine
/// now and recorded nowhere: `check: { machine, host, ok: true, at,
/// server_version, latency_ms }`, or `ok: false` with the refusal as
/// `error: { code, message }`. A sqlite Store, an unknown machine or a
/// platform that runs no PostgreSQL Sandboxes refuses the call.
pub fn status(
    home: &Path,
    events: &mut Events<'_>,
    name: &StoreName,
    machine: Option<&MachineName>,
) -> Outcome {
    let Some(machine) = machine.map(|m| m.as_str()) else {
        return layerfs_store::sqlite::family::status(home, name);
    };
    let record = check::postgresql_record(home, name)?;
    runs_postgresql_sandboxes(home, name, machine)?;
    let status = layerfs_store::sqlite::family::status(home, name)?;
    let (host, checked) = check_from(home, events, &record, machine);
    let mut fields = vec![
        ("machine".to_owned(), SdkValue::text(machine)),
        (
            "host".to_owned(),
            host.map(SdkValue::Text).unwrap_or_default(),
        ),
        ("ok".to_owned(), SdkValue::Bool(checked.is_ok())),
    ];
    match checked {
        Ok(SdkValue::Map(checked)) => fields.extend(checked),
        Ok(_) => {}
        Err(refusal) => fields.push((
            "error".to_owned(),
            SdkValue::map([
                ("code", SdkValue::Text(refusal.code)),
                ("message", SdkValue::Text(refusal.message)),
            ]),
        )),
    }
    let SdkValue::Map(mut out) = status else {
        return Ok(status);
    };
    out.push(("check".to_owned(), SdkValue::Map(fields)));
    Ok(SdkValue::Map(out))
}

/// `store.reach` in a machine's worker: `{ store, store_record, postgresql }`
/// from the caller, which put the saved login into the URL this machine
/// dials. Answers the check `store.connect --machine`, `store.status
/// --machine` and `store.doctor` ask for.
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
