//! The Store calls whose checks read who uses a Store (layerfs-project#105):
//! each runs `layerfs-store`'s call with the checks from
//! [`crate::inventory`]. The Store calls proper (`branch`, `commit`, `file`)
//! and the rest (`store.list`, `store.get`, `store.create`) are
//! `layerfs-store`'s own. `store.update --machine` also reads the machine
//! and reaches it as a Sandbox call would.
use crate::inventory;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::names::{MachineName, StoreName};
use layerfs_home::outcome::{refused, Outcome};
use layerfs_machine::worker::Unreached;
use layerfs_store::place::{self, Executor};
use layerfs_store::postgresql::{check, connect, update as store_update};
use layerfs_store::record::{OwnConnection, StoreKind, StoreRecord};
use layerfs_store::Related;
use std::path::Path;

pub use layerfs_store::close_idle;

/// `store.compact`: reclaim one Store's dead bytes, answering the
/// flattened report (`layerfs_store::report::compact_report`).
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

/// `store.move`: a sqlite Store from the machine that holds it to
/// `machine`, one of the two being this computer (C13).
pub fn move_to(home: &Path, events: &mut Events<'_>, store: &str, machine: &str) -> Outcome {
    layerfs_store::sqlite::family::move_to(home, events, store, machine, RELATED)
}

/// `store.disconnect`: forget a PostgreSQL Store; its data stays.
pub fn disconnect(home: &Path, name: &StoreName) -> Outcome {
    layerfs_store::postgresql::connect::disconnect(home, name, RELATED)
}

/// `store.update(name, { machine, url, password, synchronous_commit,
/// clear })`. No `Debug`: it names a password reference.
#[derive(Clone)]
pub struct StoreUpdate {
    /// The PostgreSQL Store.
    pub name: StoreName,
    /// The machine whose own connection is set or cleared; none for the
    /// Store's own.
    pub machine: Option<MachineName>,
    /// The new URL (no password).
    pub url: Option<String>,
    /// The password's new reference.
    pub password: Option<String>,
    /// The Store's new `synchronous_commit`.
    pub synchronous_commit: Option<String>,
    /// Remove the machine's own connection.
    pub clear: bool,
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
fn check_from(
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

/// `store.update`: changes PostgreSQL Store `name`'s own connection
/// (`layerfs-store`'s part), or with `machine` the connection that machine
/// alone dials: `url` and `password` replace the ones it has (either may
/// be given alone), checked from that machine before they are saved, and
/// `clear` removes its own connection, dialling nothing. Every refusal of
/// the input comes before any connection; a failed check saves nothing.
/// Answers the Store as `store.get` shows it.
pub fn update(home: &Path, events: &mut Events<'_>, request: &StoreUpdate) -> Outcome {
    let name = &request.name;
    let Some(machine) = request.machine.as_ref().map(|m| m.as_str()) else {
        if request.clear {
            return Err(refused(
                "InvalidInput",
                "--clear removes a machine's own connection; name it with --machine",
            ));
        }
        return store_update::update(
            home,
            &store_update::StoreUpdate {
                name: name.clone(),
                url: request.url.clone(),
                password: request.password.clone(),
                synchronous_commit: request.synchronous_commit.clone(),
            },
        );
    };
    let record = check::postgresql_record(home, name)?;
    if request.synchronous_commit.is_some() {
        return Err(refused(
            "InvalidInput",
            "synchronous_commit is the Store's setting; update it without --machine",
        ));
    }
    let given = request.url.is_some() || request.password.is_some();
    let current = record.own_connection(machine);
    if request.clear {
        if given {
            return Err(refused(
                "InvalidInput",
                "--clear takes no --url or --password",
            ));
        }
        if current.is_none() {
            return Err(refused(
                "InvalidInput",
                format!("machine {machine} has no connection of its own to Store {name}"),
            ));
        }
        check::save_own(
            home,
            name,
            machine,
            &OwnConnection::default(),
            record.binding(),
        )?;
        return layerfs_store::sqlite::family::get(home, name);
    }
    if !given {
        return Err(refused(
            "InvalidInput",
            "nothing to update: give --url or --password for the machine, or --clear",
        ));
    }
    runs_postgresql_sandboxes(home, name, machine)?;
    if let Some(url) = &request.url {
        connect::refuse_password(url)?;
        connect::target(url)?;
    }
    if let Some(password) = &request.password {
        layerfs_home::secrets::resolve(password)?;
    }
    let current = current.unwrap_or_default();
    let own = OwnConnection {
        url: request.url.clone().or(current.url),
        password: request.password.clone().or(current.password),
    };
    let trial = record.clone().with_own_connection(machine, &own);
    check_from(home, events, &trial, machine).1?;
    check::save_own(home, name, machine, &own, record.binding())?;
    layerfs_store::sqlite::family::get(home, name)
}

/// `store.reach` in a machine's worker: `{ store, store_record, postgresql }`
/// from the caller, which resolved the password and the URL this machine
/// dials. Answers the check `store.update --machine` asks for.
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
