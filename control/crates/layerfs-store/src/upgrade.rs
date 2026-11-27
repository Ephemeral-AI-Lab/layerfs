//! `store.upgrade` (#804): brings an older-format Store in place, in one
//! transaction, to the latest format this build writes. The conversion is
//! the engine's (`layerfs_persistence::upgrade`); this file records the
//! call and routes it.
//!
//! The call is the Store's exclusive operation (`intent::UPGRADE`):
//! written under the home lock and the Store's lock once the Store is idle
//! (`sqlite::family::begin`, `postgresql::family::begin`), adopting an
//! unresolved upgrade a gone process left. Past the locks it closes this
//! process's own open copy and takes the Store's lease, held for the whole
//! run, so a move, a remove or a disconnect is refused `InUse` even if a
//! read drops the operation. Each phase goes out as `{ progress: { phase,
//! done: null, total: null } }` as it starts and is written to the
//! operation. A known outcome (upgraded, current, a refusal, a cancel)
//! drops the operation, or puts back the unresolved one it adopted when the
//! run did not settle it; an unknown one (a lost reply) leaves it
//! unresolved, so the Store reads `broken` `unresolved` with
//! `layerfs store upgrade <store>` as its fix, the call that adopts it.
//!
//! Where it runs follows the Store's record, as compaction's: a sqlite
//! Store whose calls run here in this process; one on another machine in
//! that machine's worker ([`WORKER_OP`], [`in_worker`]), the worker being
//! of this build, with a cancel sent on a second connection
//! ([`CANCEL_OP`]); a PostgreSQL Store from this process.
use crate::host_maintenance;
use crate::intent;
use crate::lease;
use crate::place::{self, Executor, Worker};
use crate::record::{StoreRecord, StoreRef};
use crate::sqlite::family::{self, progress};
use crate::Related;
use layerfs_protocol::provision::install::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::home::paths;
use layerfs_home::op::{Op, Standing};
use layerfs_home::outcome::{refused, Outcome};
use layerfs_persistence::upgrade::{
    upgrade_postgresql, upgrade_sqlite, UpgradeError, UpgradeReport, UpgradeRequest,
};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The worker op that upgrades a sqlite Store on that machine:
/// `{ store, store_record, op }`; its phases go out as events.
pub const WORKER_OP: &str = "store.upgrade";
/// The worker op that cancels a running upgrade there: `{ op }`.
pub const CANCEL_OP: &str = "store.cancel";
/// The worker op that reads a sqlite Store's format there: `{ store }`.
pub const FORMAT_OP: &str = "store.format";
/// The formats this build writes, as `--version --json` shows them.
pub const FORMATS: [(&str, i64); 2] = [
    ("sqlite", layerfs_persistence::upgrade::SQLITE_LATEST),
    (
        "postgresql",
        layerfs_persistence::upgrade::POSTGRESQL_LATEST,
    ),
];

/// The message of a cancelled upgrade (internal code `Cancelled`, folded
/// to `Failed` at the call boundary).
pub const CANCELLED: &str = "cancelled before COMMIT; nothing changed";

/// The test harness's gate and stop point (as compaction's
/// `LAYERFS_COMPACT_CRASH_GATE`): the failpoint is honoured only beside the
/// gate, whose socket the engine signals before it blocks.
const CRASH_GATE_ENV: &str = "LAYERFS_UPGRADE_CRASH_GATE";
const CRASH_FAILPOINT_ENV: &str = "LAYERFS_UPGRADE_CRASH_FAILPOINT";

/// The failpoint the crash harness named, or `None` in every ordinary run.
fn crash_failpoint() -> Option<u32> {
    std::env::var_os(CRASH_GATE_ENV)?;
    std::env::var(CRASH_FAILPOINT_ENV).ok()?.parse().ok()
}

/// How often a worker upgrade's watcher looks at the cancel flag.
const WATCH: Duration = Duration::from_millis(25);

/// The use check of a caller that reads no Sandbox records: other live
/// processes' leases alone. `layerfs-control` passes its own
/// ([`upgrade_with`]), which adds the Sandboxes of the Store's projects.
fn leases_only(home: &Path, store: &str) -> Result<()> {
    check_leases(home, store, None)
}

fn check_leases(home: &Path, store: &str, owner: Option<&lease::Owner>) -> Result<()> {
    let holders = lease::holders_except(home, store, owner)?;
    if holders.is_empty() {
        return Ok(());
    }
    let pids: Vec<String> = holders.iter().map(u32::to_string).collect();
    Err(refused(
        "InUse",
        format!("Store {store} is open in process(es) {}", pids.join(", ")),
    ))
}

const LEASES_ONLY: Related = Related {
    store_idle: leases_only,
    store_idle_except: Some(|home, store, owner| check_leases(home, store, Some(owner))),
    can_remove_project: |_, _| Ok(()),
};

/// `store.upgrade(store)`: answers `{ outcome, store, engine, format_before,
/// format_after, steps, warnings }`; `cancel` is read until `commit`. The
/// Store's users are its leases ([`upgrade_with`] for a caller that knows
/// its Sandboxes).
pub fn upgrade(home: &Path, events: &mut Events<'_>, store: &str, cancel: &AtomicBool) -> Outcome {
    upgrade_with(home, events, store, cancel, LEASES_ONLY)
}

/// [`upgrade`], refused `InUse` by `related.store_idle` (a lease of another
/// process, a Sandbox of one of its projects) before anything runs.
pub fn upgrade_with(
    home: &Path,
    events: &mut Events<'_>,
    store: &str,
    cancel: &AtomicBool,
    related: Related,
) -> Outcome {
    layerfs_home::names::StoreName::parse(store)?;
    // read before the locks only to refuse early with nothing written: a
    // stopped machine is `Unavailable`, naming its start
    host_maintenance::recover(home, store, intent::UPGRADE, None)?;
    let seen = StoreRecord::of(home, store)?;
    let postgres = matches!(seen.store_ref()?, StoreRef::Postgres { .. });
    if let Executor::Worker(mut worker) = place::plan(home, seen.store_ref()?)? {
        worker.ready()?;
    }
    let op = Op::begin(intent::UPGRADE, "check", SdkValue::Null)?;
    // the record as re-read under the locks decides everything after:
    // where the run goes, the URL and schema, the session closed
    let check = |record: &StoreRecord| adopt(record).map(|previous| (previous, record.clone()));
    let ((previous, record), worker) = if postgres {
        (
            crate::postgresql::family::begin(home, store, &op, related, check)?,
            None,
        )
    } else {
        let worker = host_maintenance::observe(home, &seen);
        family::begin_maintenance(home, store, &op, related, worker, check)?
    };
    host_maintenance::finish(
        home,
        store,
        &op,
        worker.as_ref(),
        Some(related),
        previous.as_ref(),
    )?;
    let mut run = Run {
        home,
        store,
        op,
        previous,
    };
    let outcome = (|| {
        let store_ref = record.store_ref()?;
        let mut executor = place::plan(home, store_ref)?;
        if let Executor::Worker(worker) = &mut executor {
            worker.ready()?;
        }
        let _lease = match store_ref {
            StoreRef::Postgres { .. } => crate::postgresql::family::settle(home, &record, related)?,
            StoreRef::Sqlite { .. } => family::settle(home, store, related, &run.op)?,
        };
        match (store_ref, executor) {
            (StoreRef::Postgres { .. }, _) => run.postgresql(&record, events, cancel),
            (StoreRef::Sqlite { .. }, Executor::Local) => {
                // the file here: a compaction live beside it is `Busy`; a
                // worker checks the file under its own root
                crate::compact::refuse_while_running(&paths::store_file(home, store), store)?;
                run.local(events, cancel)
            }
            (StoreRef::Sqlite { .. }, Executor::Worker(worker)) => {
                run.worker(worker, &record, events, cancel)
            }
        }
    })();
    run.end(&outcome);
    outcome
}

/// The check `begin` makes of the record: an upgrade a gone process left
/// unresolved is adopted (answered, so a run that does not settle it puts
/// it back); any other operation refuses (`Busy` while it runs,
/// `Unresolved` with its own clearing call).
fn adopt(record: &StoreRecord) -> Result<Option<Op>> {
    match record.op() {
        Some(held) if held.name() == intent::UPGRADE && held.standing() == Standing::Unresolved => {
            Ok(Some(held))
        }
        _ => record.unheld().map(|()| None),
    }
}

/// One upgrade's operation in the Store's record.
struct Run<'a> {
    home: &'a Path,
    store: &'a str,
    op: Op,
    /// The unresolved upgrade this run adopted, if any.
    previous: Option<Op>,
}

impl Run<'_> {
    /// The run entered `phase`: written to the operation, then sent.
    fn at(&mut self, events: &mut Events<'_>, phase: &str) {
        let _ = intent::advance(self.home, self.store, &mut self.op, phase);
        progress(events, phase, None);
    }

    /// The run ended with `outcome`: an answer drops the operation; a
    /// refusal puts back the upgrade it adopted (or drops it); an unknown
    /// outcome leaves it unresolved, with no owner.
    fn end(&self, outcome: &Outcome) {
        match outcome {
            Ok(_) => {
                let _ = intent::clear(self.home, self.store, &self.op);
            }
            Err(refusal) if refusal.code == "Unknown" => {
                intent::leave(self.home, self.store, &self.op, &refusal.message)
            }
            Err(_) => {
                let previous = self.previous.as_ref();
                let _ = family::replace_op(self.home, self.store, self.op.id(), previous);
            }
        }
    }

    /// A sqlite Store whose calls run in this process.
    fn local(&mut self, events: &mut Events<'_>, cancel: &AtomicBool) -> Outcome {
        let path = paths::store_file(self.home, self.store);
        let request = UpgradeRequest {
            cancel,
            failpoint: crash_failpoint(),
        };
        let report = upgrade_sqlite(&path, &request, &mut |phase| self.at(events, phase));
        report
            .map(|report| answer(self.store, &report))
            .map_err(refusal)
    }

    /// A PostgreSQL Store, from this process: its URL with its login and
    /// its schema.
    fn postgresql(
        &mut self,
        record: &StoreRecord,
        events: &mut Events<'_>,
        cancel: &AtomicBool,
    ) -> Outcome {
        let url = crate::record::url(record)?;
        let schema = postgresql_schema(record)?;
        let request = UpgradeRequest {
            cancel,
            failpoint: None,
        };
        let report =
            upgrade_postgresql(&url, &schema, &request, &mut |phase| self.at(events, phase));
        report
            .map(|report| answer(self.store, &report))
            .map_err(refusal)
    }

    /// A sqlite Store on another machine: one worker request, its events
    /// passed on. While it runs a watcher looks at `cancel`; once it is set
    /// and the worker has begun (its first phase came back, so the worker
    /// holds the run's flag), it sends [`CANCEL_OP`] once on a second
    /// connection. The first connection keeps reading the answer, which
    /// decides the outcome.
    fn worker(
        &mut self,
        mut worker: Worker,
        record: &StoreRecord,
        events: &mut Events<'_>,
        cancel: &AtomicBool,
    ) -> Outcome {
        if cancel.load(Ordering::SeqCst) {
            return Err(refused("Cancelled", CANCELLED));
        }
        let op = self.op.id().to_owned();
        let request = SdkValue::map([
            ("store", SdkValue::text(self.store)),
            ("store_record", record.to_record().clone()),
            ("op", SdkValue::text(&op)),
        ]);
        let (begun, done) = (AtomicBool::new(false), AtomicBool::new(false));
        let mut canceller = worker.clone();
        std::thread::scope(|scope| {
            scope.spawn(|| {
                while !done.load(Ordering::SeqCst) {
                    if begun.load(Ordering::SeqCst) && cancel.load(Ordering::SeqCst) {
                        let args = SdkValue::map([("op", SdkValue::text(&op))]);
                        let _ = canceller.call(CANCEL_OP, args, &mut |_| {});
                        return;
                    }
                    std::thread::sleep(WATCH);
                }
            });
            let outcome = worker.call(WORKER_OP, request, &mut |event| {
                let phase = event.get("progress").and_then(|p| p.get_text("phase"));
                if let Some(phase) = phase {
                    let _ = intent::advance(self.home, self.store, &mut self.op, phase);
                    begun.store(true, Ordering::SeqCst);
                }
                events.event(event);
            });
            done.store(true, Ordering::SeqCst);
            outcome
        })
    }
}

/// PostgreSQL Store `record`'s schema, from its record.
pub fn postgresql_schema(record: &StoreRecord) -> Result<String> {
    let schema = match record.store_ref()? {
        StoreRef::Postgres { server, .. } => server.schema.clone(),
        StoreRef::Sqlite { .. } => None,
    };
    schema.ok_or_else(|| {
        refused(
            "InvalidInput",
            format!("Store {} records no PostgreSQL schema", record.name()),
        )
    })
}

/// The answer of an upgrade of Store `store`.
fn answer(store: &str, report: &UpgradeReport) -> SdkValue {
    let texts = |items: &[String]| SdkValue::List(items.iter().map(SdkValue::text).collect());
    SdkValue::map([
        ("outcome", SdkValue::text(report.outcome.word())),
        ("store", SdkValue::text(store)),
        ("engine", SdkValue::text(report.engine)),
        ("format_before", SdkValue::Int(report.format_before)),
        ("format_after", SdkValue::Int(report.format_after)),
        ("steps", texts(&report.steps)),
        ("warnings", texts(&report.warnings)),
    ])
}

/// The engine's refusal under its code; a cancel is the internal
/// `Cancelled`, folded to `Failed` with `detail.code` `Cancelled`.
fn refusal(error: UpgradeError) -> SdkRefusal {
    match error {
        UpgradeError::InUse(message) => refused("InUse", message),
        UpgradeError::Busy(message) => refused("Busy", message),
        UpgradeError::Unsupported(message) => refused("Unsupported", message),
        UpgradeError::Failed(message) => refused("Failed", message),
        UpgradeError::Cancelled => refused("Cancelled", CANCELLED),
        UpgradeError::Unknown(message) => refused("Unknown", message),
    }
}

/// The flags of the upgrades running in this worker, by op id.
static RUNNING: Mutex<BTreeMap<String, Arc<AtomicBool>>> = Mutex::new(BTreeMap::new());

/// One upgrade running in this worker: its cancel flag, registered under
/// its op id until it ends. Public for the worker's tests alone.
#[doc(hidden)]
pub struct Running {
    op: String,
    flag: Arc<AtomicBool>,
}

impl Running {
    /// Registers upgrade `op`; `Busy` when one of that id runs already.
    pub fn register(op: &str) -> Result<Self> {
        let mut running = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
        if running.contains_key(op) {
            return Err(refused(
                "Busy",
                format!("upgrade {op} is running on this machine"),
            ));
        }
        let flag = Arc::new(AtomicBool::new(false));
        running.insert(op.to_owned(), flag.clone());
        Ok(Running {
            op: op.to_owned(),
            flag,
        })
    }

    /// Its cancel flag.
    pub fn flag(&self) -> &AtomicBool {
        &self.flag
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let mut running = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
        running.remove(&self.op);
    }
}

/// The worker's side of `store.upgrade`: `{ store, store_record, op }`,
/// the Store file under the worker's `root`, never a path the caller
/// sends. It closes the worker's own open copy, is refused `InUse` while
/// another process holds the Store's lease under the root and `Busy`
/// while a compaction runs on it, then holds the lease for the run and
/// upgrades the file with a flag `store.cancel` sets. An event that cannot
/// be written does not end the run: a caller that went away gets
/// `Unknown`, and the run finishes or rolls back on its own terms.
pub fn in_worker(
    root: &Path,
    args: &SdkValue,
    events: &mut dyn FnMut(SdkValue) -> std::result::Result<(), SdkRefusal>,
) -> Outcome {
    let store = args
        .get_text("store")
        .ok_or_else(|| refused("InvalidInput", "store.upgrade names its Store"))?;
    layerfs_home::names::StoreName::parse(store)?;
    let op = args
        .get_text("op")
        .ok_or_else(|| refused("InvalidInput", "store.upgrade names its op"))?;
    let fields = args
        .get("store_record")
        .cloned()
        .ok_or_else(|| refused("InvalidInput", "store.upgrade sends the Store's record"))?;
    let record = StoreRecord::from_record(store, fields);
    if !matches!(record.store_ref()?, StoreRef::Sqlite { .. }) {
        return Err(refused(
            "InvalidInput",
            format!("Store {store} is not a sqlite Store"),
        ));
    }
    let file = paths::store_file(root, store);
    if !file.is_file() {
        return Err(refused(
            "NotFound",
            format!("no file of Store {store} is on this machine"),
        ));
    }
    crate::open::close(&file)?;
    let holders = lease::holders(root, store)?;
    if !holders.is_empty() || lease::held_here(root, store) {
        let mut pids: Vec<String> = holders.iter().map(u32::to_string).collect();
        if pids.is_empty() {
            pids.push(std::process::id().to_string());
        }
        return Err(refused(
            "InUse",
            format!(
                "Store {store} is open on that machine in process(es) {}",
                pids.join(", ")
            ),
        ));
    }
    crate::compact::refuse_while_running(&file, store)?;
    let _lease = lease::hold_locked(root, store)?;
    let running = Running::register(op)?;
    let mut sink = |event| {
        let _ = events(event);
    };
    let mut events = Events::new(&mut sink);
    let request = UpgradeRequest {
        cancel: running.flag(),
        failpoint: crash_failpoint(),
    };
    upgrade_sqlite(&file, &request, &mut |phase| {
        progress(&mut events, phase, None)
    })
    .map(|report| answer(store, &report))
    .map_err(refusal)
}

/// The worker's `store.cancel { op }`: sets the running upgrade's flag and
/// answers `{}`; `NotFound` when no upgrade of that id runs here (it ended,
/// or never began).
pub fn cancel_in_worker(args: &SdkValue) -> Outcome {
    let op = args
        .get_text("op")
        .ok_or_else(|| refused("InvalidInput", "store.cancel names its op"))?;
    let running = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
    let flag = running.get(op).ok_or_else(|| {
        refused(
            "NotFound",
            format!("no upgrade {op} is running on this machine"),
        )
    })?;
    flag.store(true, Ordering::SeqCst);
    Ok(SdkValue::Map(Vec::new()))
}

/// The worker's `store.format { store }`: the format section of the file
/// under the worker's `root` (`crate::formats`), read from its header.
pub fn format_in_worker(root: &Path, args: &SdkValue) -> Outcome {
    let store = args
        .get_text("store")
        .ok_or_else(|| refused("InvalidInput", "store.format names its Store"))?;
    layerfs_home::names::StoreName::parse(store)?;
    Ok(crate::formats::sqlite_file(
        &paths::store_file(root, store),
        store,
    ))
}
