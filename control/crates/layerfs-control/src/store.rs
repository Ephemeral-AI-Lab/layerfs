//! The Store calls whose checks read who uses a Store (layerfs-project#105):
//! each runs `layerfs-store`'s call with the checks from
//! [`crate::inventory`]. The Store calls proper (`branch`, `commit`, `file`)
//! and the rest are `layerfs-store`'s own. `store.attach` also reads the
//! machine and reaches it as a Sandbox call would.
use crate::inventory;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::names::{MachineName, StoreName};
use layerfs_home::outcome::{refused, Outcome};
use layerfs_machine::worker::Unreached;
use layerfs_store::place::{self, Executor};
use layerfs_store::postgresql::{attach, connect};
use layerfs_store::record::{Attachment, StoreKind};
use layerfs_store::sqlite::family::StorePush;
use layerfs_store::Related;
use std::path::Path;

pub use layerfs_store::close_idle;

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

/// `store.disconnect`: forget a PostgreSQL Store; its data stays.
pub fn disconnect(home: &Path, name: &StoreName) -> Outcome {
    layerfs_store::postgresql::connect::disconnect(home, name, RELATED)
}

/// `store.attach(name, machine, { url, password })`. No `Debug`: it names a
/// password reference.
#[derive(Clone)]
pub struct StoreAttach {
    /// The PostgreSQL Store.
    pub name: StoreName,
    /// The machine that may run its Sandboxes.
    pub machine: MachineName,
    /// The URL the machine dials (no password), when not the Store's.
    pub url: Option<String>,
    /// The password's reference, when not the Store's.
    pub password: Option<String>,
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
/// the server was not reached. A worker from a bundle without the check
/// says how to update it.
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
                "machine {machine}'s worker predates store.attach; `layerfs machine stop {machine}` and `layerfs machine start {machine}` update it"
            ),
        ),
        "Unavailable" => {
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

/// `store.attach`: checks machine `machine`'s connection to PostgreSQL
/// Store `name` from that machine (in this process, or in its worker), then
/// records the attachment; on a machine already attached it checks the new
/// connection and replaces the old one. Every refusal of the input comes
/// before any connection; a failed check records nothing. Answers the
/// Store as `store.get` shows it.
pub fn attach(home: &Path, events: &mut Events<'_>, request: &StoreAttach) -> Outcome {
    let (name, machine) = (&request.name, request.machine.as_str());
    let record = attach::postgresql_record(home, name)?;
    runs_postgresql_sandboxes(home, name, machine)?;
    if let Some(url) = &request.url {
        connect::refuse_password(url)?;
        connect::target(url)?;
    }
    if let Some(password) = &request.password {
        layerfs_home::secrets::resolve(password)?;
    }
    let attachment = Attachment {
        url: request.url.clone(),
        password: request.password.clone(),
        checked: None,
    };
    // the URL the machine dials, built as its Sandboxes' calls build it
    let trial = record.clone().with_attachment(machine, &attachment);
    let (postgresql, hint) = layerfs_sandbox::route::reached(home, machine, &trial)?;
    let (host, _, _) = connect::target(postgresql.url()?)?;
    let checked = match place::on_machine(home, machine)? {
        Executor::Local => attach::check(&record, &postgresql),
        Executor::Worker(mut worker) => {
            let request = SdkValue::map([
                ("store", SdkValue::text(name.as_str())),
                ("store_record", record.to_record().clone()),
                ("postgresql", postgresql.encode()),
            ]);
            let mut sink = |event| events.event(event);
            match worker.attempt("store.reach", request, &mut sink) {
                Ok(outcome) => outcome,
                Err(Unreached(refusal)) => Err(refusal),
            }
        }
    }
    .map_err(|refusal| from_machine(refusal, (name, machine, &host), hint))?;
    let attachment = Attachment {
        checked: Some(checked),
        ..attachment
    };
    attach::record(home, name, machine, &attachment, record.binding())?;
    layerfs_store::sqlite::family::get(home, name)
}

/// `store.reach` in a machine's worker: `{ store, store_record, postgresql }`
/// from the caller, which resolved the password and the URL this machine
/// dials. Answers the check `store.attach` records.
pub fn reach_in_worker(args: &SdkValue) -> Outcome {
    let store = args.get_text("store").unwrap_or("");
    StoreName::parse(store)?;
    let record = layerfs_store::record::StoreRecord::from_record(
        store,
        args.get("store_record").cloned().unwrap_or_default(),
    );
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
    attach::check(&record, &postgresql)
}

/// `store.detach`: forgets PostgreSQL Store `name`'s attachment to
/// `machine`; refused `InUse` while a Sandbox of the Store is recorded
/// there. Answers the Store as `store.get` shows it.
pub fn detach(home: &Path, name: &StoreName, machine: &MachineName) -> Outcome {
    attach::detach(home, name, machine.as_str(), inventory::store_idle_on)?;
    layerfs_store::sqlite::family::get(home, name)
}
