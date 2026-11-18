//! `store.compact`: one Store's §7 (SQLite) or §8 (PostgreSQL) rebuild, run
//! in this process.
//!
//! The run is recorded as the Store's `op` (name `store.compact`) so every
//! client reads the Store as `busy` while it runs (call surface 2.0): a
//! marker beside the run's own record, written once the Store is this
//! call's to compact, moved as each phase starts and dropped when the run
//! settled. A run that left its own record (its reply was lost, or a
//! residue stayed) leaves the marker with its outcome unknown, so the
//! Store reads `broken`, `unresolved` until the next `store compact`
//! adopts and settles it; a crash leaves the same. The marker refuses
//! nothing: the Store's own admission decides what runs beside a
//! compaction, as before.
//!
//! The call takes the Store's lease, so a move or remove cannot start, and
//! closes the Store this process holds open: a call still running on it is
//! `Busy` in the GC vocabulary (§10 keeps `InUse` for non-GC calls), before
//! any effect. It then opens the Store's own session, runs
//! the §7 (SQLite) or §8 (PostgreSQL) compaction and answers the §11 report.
//! A SQLite Store on another machine compacts there (spec S6): the call sends
//! `{ store, store_record }` to that machine's worker ([`WORKER_OP`]), which
//! runs the same §7 path on the file under its own root ([`in_worker`]) while
//! this process keeps the Store's lease and its marker. Decision D5 in
//! `core/docs/issues/project-41/gc-42-decisions.md` records the change from
//! the earlier local-only refusal.
use crate::identity;
use crate::identity::Kind;
use crate::intent;
use crate::lease;
use crate::open::close;
use crate::open_args::OpenArgs;
use crate::place::{self, Executor};
use crate::postgresql;
use crate::record::{StoreRecord, StoreRef};
use crate::sqlite::family::progress;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_history::GcScopeKind;
use layerfs_home::events::Events;
use layerfs_home::home::paths;
use layerfs_home::op::Op;
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_persistence::compact_metrics::{self, CompactRefusal};
use layerfs_persistence::PostgresHandles;
use layerfs_persistence::{
    CompactFailpoint, CompactRequest, PersistenceConfig, SqliteHandles, SqlitePersistenceProfile,
};
use layerfs_store_service::history::{admission, lookup};
use std::path::Path;

/// The T-G65 crash harness's gate: the real `layerfs store compact` process it
/// starts carries this socket, and the SQLite engine stops at the failpoint
/// below instead of reporting an unknown outcome. Ordinary callers set
/// neither variable, so a run carries no failpoint.
const CRASH_GATE_ENV: &str = "LAYERFS_COMPACT_CRASH_GATE";
/// The stop point the crash harness names; honored only beside the gate.
const CRASH_FAILPOINT_ENV: &str = "LAYERFS_COMPACT_CRASH_FAILPOINT";

/// The failpoint the crash harness named, or `None` in every ordinary run.
///
/// A failpoint alone would stop the run in-process; the gate makes the stop
/// observable to the harness, which then `SIGKILL`s this process. Requiring
/// both keeps the hook inert for production callers and for builds without
/// the engine's Unix-socket gate.
#[cfg(unix)]
fn crash_failpoint() -> Option<CompactFailpoint> {
    std::env::var_os(CRASH_GATE_ENV)?;
    match std::env::var(CRASH_FAILPOINT_ENV).ok()?.as_str() {
        "BeforeSeal" => Some(CompactFailpoint::BeforeSeal),
        "AfterSeal" => Some(CompactFailpoint::AfterSeal),
        "BeforeLoad" => Some(CompactFailpoint::BeforeLoad),
        "AfterBuild" => Some(CompactFailpoint::AfterBuild),
        "DuringVerify" => Some(CompactFailpoint::DuringVerify),
        "BeforeInstall" => Some(CompactFailpoint::BeforeInstall),
        "DuringRename" => Some(CompactFailpoint::DuringRename),
        "AfterInstall" => Some(CompactFailpoint::AfterInstall),
        "DuringSettle" => Some(CompactFailpoint::DuringSettle),
        _ => None,
    }
}

/// No gate, no failpoint: a stop would hang the run in-process.
#[cfg(not(unix))]
fn crash_failpoint() -> Option<CompactFailpoint> {
    None
}

/// The run's marker in the Store's record, if one was written.
struct Marker<'a> {
    home: &'a Path,
    store: &'a str,
    op: Option<Op>,
}

impl<'a> Marker<'a> {
    /// Records the compaction as the Store's `op`, its outcome unknown
    /// until the run settles (a crash leaves the run's own record). None is
    /// kept when a live process's operation is on the Store, or when the
    /// record cannot be written: the run goes on either way.
    fn begin(home: &'a Path, store: &'a str) -> Self {
        let op = intent::mark(home, store, intent::COMPACT, "mark", SdkValue::Null)
            .ok()
            .flatten()
            .map(Op::as_uncertain);
        if let Some(op) = &op {
            let _ = intent::replace(home, store, op.id(), Some(op));
        }
        Marker { home, store, op }
    }

    /// The run entered `phase`.
    fn at(&mut self, phase: &str) {
        if let Some(op) = &mut self.op {
            let _ = intent::advance(self.home, self.store, op, phase);
        }
    }

    /// The run ended: the marker goes when the run settled (`left` false),
    /// and stays, unresolved, when the run's own record is still there.
    fn end(self, left: bool, outcome: &Outcome) {
        let Some(op) = &self.op else {
            return;
        };
        let refused_busy = matches!(outcome, Err(refusal) if refusal.code == "Busy");
        if left && !refused_busy {
            let message = match outcome {
                Ok(_) => "the compaction left a residue",
                Err(refusal) => refusal.message.as_str(),
            };
            intent::leave(self.home, self.store, op, message);
        } else {
            let _ = intent::clear(self.home, self.store, op);
        }
    }
}

/// `store.compact(store)`: reclaim one Store's dead bytes, answering
/// the report ([`crate::report::compact_report`]). Each phase is sent to
/// `events` as it starts, as `{ phase, done, total }` with null counts (the
/// shape `store.move` sends) and written to the Store's `op`; the run's own
/// record, which a live read shows, is written by the run, not from here.
pub fn compact(home: &Path, events: &mut Events<'_>, store: &str) -> Outcome {
    let home = home.to_path_buf();
    // A live operation is `Busy`, a dead owner's record `Unresolved`; the
    // refusal is a bounded metric label and never changes what is returned.
    let (_lease, record) = match lease::claim(&home, store) {
        Ok(claimed) => claimed,
        Err(refusal) => {
            note_refusal(&refusal);
            return Err(refusal);
        }
    };
    let store_ref = record.store_ref()?.clone();
    if matches!(store_ref, StoreRef::Postgres { .. }) {
        return compact_postgresql(&record, &home, events, store);
    }
    let executor = place::plan(&home, &store_ref)?;
    // The marker is this home's, whichever machine runs the rebuild: it is
    // written here, beside the caller's record, never under a worker's root.
    let mut marker = Marker::begin(&home, store);
    let (outcome, left) = match executor {
        Executor::Local => {
            let outcome =
                compact_sqlite(&home, events, store, &record, &mut |phase| marker.at(phase));
            // the run's own record beside the Store says whether it settled
            let path = paths::store_file(&home, store);
            let left = layerfs_persistence::compact_op_status(&path).is_some();
            (outcome, left)
        }
        Executor::Worker(mut worker) => {
            // S6: the rebuild runs on the machine that holds the file. This
            // process keeps the Store's lease while the worker runs it, so a
            // move or remove cannot start; the worker's phases come back as
            // they are sent. A lost answer is `Unknown`, never resent: the
            // run's own op record lets the next compaction judge it (§9).
            let request = SdkValue::map([
                ("store", SdkValue::text(store)),
                ("store_record", record.to_record().clone()),
            ]);
            let outcome = worker.call(WORKER_OP, request, &mut |event| {
                let phase = event.get("progress").and_then(|p| p.get_text("phase"));
                if let Some(phase) = phase {
                    marker.at(phase);
                }
                events.event(event)
            });
            // the worker's record is not readable here: a lost answer, or a
            // run that reports a residue, leaves the outcome unresolved
            let left = match &outcome {
                Ok(report) => report.get_int("residue_bytes").unwrap_or(0) > 0,
                Err(refusal) => refusal.code == "Unknown",
            };
            (outcome, left)
        }
    };
    marker.end(left, &outcome);
    outcome
}

/// The worker operation that compacts a sqlite Store on its machine.
pub const WORKER_OP: &str = "store.compact";

/// Whether the process a compaction record names still runs on this machine.
/// The kernel start time and boot id, when recorded, keep a reused id from
/// passing as the owner.
pub fn owner_alive(op: &layerfs_persistence::CompactOpStatus) -> bool {
    let Some(pid) = op.owner_pid else {
        return false;
    };
    layerfs_home::process::alive_as(pid, op.owner_pid_start, op.owner_pid_boot)
}

/// Refuses `Busy` while `<store>.compact-op` beside Store file `file`
/// names a run that is live on this machine (its owner runs and it has not
/// returned). The refusal carries the run as `op`, `{ name: compact, phase,
/// started_at }`. A step that reads, replaces or deletes the file (a move's
/// or remove's) asks this first: after a lost reply the caller's lease is
/// gone, so only the record the run keeps beside the file can stop it.
pub fn refuse_while_running(file: &Path, store: &str) -> Result<(), SdkRefusal> {
    let Some(op) = layerfs_persistence::compact_op_status(file) else {
        return Ok(());
    };
    let status = op.status(owner_alive(&op));
    if status.state != "busy" {
        return Ok(());
    }
    let refusal = refused(
        "Busy",
        format!(
            "Store {store} is being compacted on this machine (phase {}); it can be moved or \
             removed once that run ends",
            status.phase
        ),
    );
    let op = SdkValue::map([
        ("name", SdkValue::text("compact")),
        ("phase", SdkValue::text(&status.phase)),
        (
            "started_at",
            SdkValue::Text(layerfs_home::time::iso(status.started)),
        ),
    ]);
    Err(layerfs_home::outcome::with_next(refusal, "op", op))
}

/// The worker's side of `store.compact`: `{ store, store_record }`, the
/// Store file under the worker's `root` (the machine's data disk), never a
/// path the caller sends. The worker holds the Store's lease under its root
/// for the run, as `store.call` does while it keeps a Store open. An event
/// that cannot be written does not end the run (#177): a caller that went
/// away gets `Unknown`, and the run still finishes or aborts on its own terms.
pub fn in_worker(
    root: &Path,
    args: &SdkValue,
    events: &mut dyn FnMut(SdkValue) -> Result<(), SdkRefusal>,
) -> Outcome {
    let store = args
        .get_text("store")
        .ok_or_else(|| refused("InvalidInput", "store.compact names its Store"))?;
    layerfs_home::names::StoreName::parse(store)?;
    let fields = args
        .get("store_record")
        .cloned()
        .ok_or_else(|| refused("InvalidInput", "store.compact sends the Store's record"))?;
    let record = StoreRecord::from_record(store, fields);
    if !matches!(record.store_ref()?, StoreRef::Sqlite { .. }) {
        return Err(refused(
            "InvalidInput",
            format!("Store {store} is not a sqlite Store"),
        ));
    }
    let _lease = lease::hold_locked(root, store)?;
    let mut sink = |event| {
        let _ = events(event);
    };
    let mut events = Events::new(&mut sink);
    // the caller's home keeps the marker; nothing is written beside it here
    compact_sqlite(root, &mut events, store, &record, &mut |_| {})
}

/// The §7 SQLite path on Store file `store` under `home` (a home, or a
/// worker's root), its lease held by the caller. `on_phase` hears each
/// phase as it starts, before it is sent to `events`.
fn compact_sqlite(
    home: &Path,
    events: &mut Events<'_>,
    store: &str,
    record: &StoreRecord,
    on_phase: &mut dyn FnMut(&str),
) -> Outcome {
    let home = home.to_path_buf();
    let path = paths::store_file(&home, store);
    // A cached open in this process must end first. A call still running on
    // it keeps the Store open and is `Busy` in the GC vocabulary (§10 keeps
    // `InUse` for non-GC calls), naming the Store with no phase or op: no
    // operation holds it, a call does.
    if let Err(refusal) = close(&path) {
        let refusal = gc_refusal(store, refusal);
        note_refusal(&refusal);
        return Err(refusal);
    }
    let args = OpenArgs {
        home: Some(home.clone()),
        store: Some(store.to_owned()),
        project: None,
        store_path: Some(path.display().to_string()),
        store_record: Some(record.to_record().clone()),
        postgresql: None,
    };
    let identity = identity::identity(&args)?;
    let handles = SqliteHandles::open_writable(
        PersistenceConfig::sqlite(&path).with_sqlite_profile(SqlitePersistenceProfile::Normal),
        &identity.binding,
        identity.cursor_key,
    )
    .map_err(open_refusal)?;
    let (owner_pid_start, owner_pid_boot) = match layerfs_home::process::me() {
        Some((start, boot)) => (Some(start), Some(boot)),
        None => (None, None),
    };
    let op_id = admission::op_id();
    let (owner_runtime, owner_boot) = admission::owner_tokens(&format!(
        "compact:{}",
        layerfs_bridge::sdk_values::hex(&op_id)
    ));
    let receipt = handles
        .compact_observed(
            &CompactRequest {
                op_id,
                owner_pid: Some(std::process::id()),
                owner_pid_start,
                owner_pid_boot,
                owner_runtime: Some(owner_runtime),
                owner_boot: Some(owner_boot),
                owner_live: Some(layerfs_home::process::recorded_owner_alive),
                failpoint: crash_failpoint(),
                ..CompactRequest::default()
            },
            &mut |phase| {
                on_phase(phase);
                progress(events, phase, None)
            },
        )
        .map_err(|error| {
            lookup::history_in(error, GcScopeKind::Store, store, None, Some(&op_id))
        })?;
    Ok(crate::report::compact_report(store, &receipt))
}

/// The §8 PostgreSQL path: dial the Store's recorded URL, open its schema as
/// this home's authority, and run Phase M/A/B/C. The Store is usable at
/// G + 1 from the swap on; a lost reply is judged by the next call (§9).
///
/// The PostgreSQL engine is always built beside SQLite (#726).
fn compact_postgresql(
    record: &StoreRecord,
    home: &Path,
    events: &mut Events<'_>,
    store: &str,
) -> Outcome {
    let url = crate::record::url(record)?;
    let id = identity::identity(&OpenArgs {
        home: Some(home.to_path_buf()),
        store: Some(store.to_owned()),
        store_record: Some(record.to_record().clone()),
        ..OpenArgs::default()
    })?;
    let Kind::Postgresql {
        schema,
        synchronous_commit,
    } = &id.kind
    else {
        return Err(refused(
            "Failed",
            format!("Store {store} is not a PostgreSQL Store"),
        ));
    };
    let selection = postgresql::open::selection(&url, schema, synchronous_commit);
    let mut marker = Marker::begin(home, store);
    let outcome: Outcome = (|| {
        let handles = PostgresHandles::open_writable(&selection, &id.binding, id.cursor_key)
            .map_err(open_refusal)?;
        let (owner_pid_start, owner_pid_boot) = match layerfs_home::process::me() {
            Some((start, boot)) => (Some(start), Some(boot)),
            None => (None, None),
        };
        let op_id = admission::op_id();
        let (owner_runtime, owner_boot) = admission::owner_tokens(&format!(
            "compact:{}",
            layerfs_bridge::sdk_values::hex(&op_id)
        ));
        let receipt = handles
            .compact_observed(
                &CompactRequest {
                    op_id,
                    owner_pid: Some(std::process::id()),
                    owner_pid_start,
                    owner_pid_boot,
                    owner_runtime: Some(owner_runtime),
                    owner_boot: Some(owner_boot),
                    ..CompactRequest::default()
                },
                &mut |phase| {
                    marker.at(phase);
                    progress(events, phase, None)
                },
            )
            .map_err(|error| {
                lookup::history_in(error, GcScopeKind::Store, store, None, Some(&op_id))
            })?;
        Ok(crate::report::compact_report(store, &receipt))
    })();
    // the Store's own `gc_op` row says whether the run settled; when the
    // server cannot say, only a lost reply leaves the outcome unknown
    let left = match layerfs_persistence::postgresql_compact_status(&url, schema) {
        Ok(None) => false,
        Ok(Some(op)) => op.status().state == "unresolved",
        Err(_) => matches!(&outcome, Err(refusal) if refusal.code == "Unknown"),
    };
    marker.end(left, &outcome);
    outcome
}

/// An open refusal before the run: `Busy` is definite, everything else a
/// failure with its own text.
fn open_refusal(error: layerfs_storage::port::PersistenceError) -> SdkRefusal {
    use layerfs_storage::port::PersistenceError;
    let refusal = match error {
        PersistenceError::Busy => refused("Busy", "the Store is held by another writer"),
        PersistenceError::Refused { status } => refused(
            "Busy",
            format!("the Store refused the compaction: {status}"),
        ),
        other => failed(other),
    };
    note_refusal(&refusal);
    refusal
}

/// Counts a Busy/Unresolved refusal of a call that never reached a run; the
/// measured SQLite run records the refusals it produces itself.
fn note_refusal(refusal: &SdkRefusal) {
    match refusal.code.as_str() {
        "Busy" => compact_metrics::record_refusal(CompactRefusal::Busy),
        "Unresolved" => compact_metrics::record_refusal(CompactRefusal::Unresolved),
        _ => {}
    }
}

/// Maps the open cache's refusal into the GC vocabulary (§10): a call still
/// running on the Store in this process keeps it open and is `Busy`, naming
/// the Store as the entity with a null `phase` and `op` because no operation
/// holds it; everything else is returned unchanged. The cache's own `InUse`
/// stays for the non-GC calls (`store.move`, `store.remove`,
/// `store.disconnect`, `backup.restore`) that close it.
#[doc(hidden)]
pub fn gc_refusal(store: &str, refusal: SdkRefusal) -> SdkRefusal {
    if refusal.code != "InUse" {
        return refusal;
    }
    let mut busy = refused(
        "Busy",
        format!(
            "Store {store} is busy: a call is running on it in this process; retry \
             `store.compact` after that call ends"
        ),
    );
    busy.detail = SdkValue::map([
        (
            "entity",
            SdkValue::map([
                ("kind", SdkValue::text("store")),
                ("name", SdkValue::text(store)),
            ]),
        ),
        ("phase", SdkValue::Null),
        ("op", SdkValue::Null),
    ]);
    busy
}
