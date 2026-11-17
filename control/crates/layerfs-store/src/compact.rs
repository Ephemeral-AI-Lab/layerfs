//! `store.compact`: one Store's §7 (SQLite) or §8 (PostgreSQL) rebuild, run
//! in this process.
//!
//! The call takes the Store's lease, so a move or remove cannot start, and
//! closes the Store this process holds open: a call still running on it is
//! `Busy` in the GC vocabulary (§10 keeps `InUse` for non-GC calls), before
//! any effect. It then opens the Store's own session, runs
//! the §7 (SQLite) or §8 (PostgreSQL) compaction and answers the §11 report.
//! A SQLite Store on another machine is `Unsupported` and is never sent to
//! that machine's worker. Spec S6 says the rebuild runs on the machine that
//! holds the file; 2.0 implements that only through `place::plan`'s `Local`
//! route. This local-only reading is the documented contract deviation D5 in
//! `core/docs/issues/project-41/gc-42-decisions.md` (with the API and control
//! docs); run the call on that machine.
use crate::identity;
use crate::identity::Kind;
use crate::lease;
use crate::open::close;
use crate::open_args::OpenArgs;
use crate::place::{self, Executor};
use crate::postgresql;
use crate::record::{StoreRecord, StoreRef};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_history::GcScopeKind;
use layerfs_home::events::Events;
use layerfs_home::home::paths;
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

/// `store.compact(name)`: reclaim one Store's dead bytes, answering
/// the §11 report.
pub fn compact(home: &Path, _events: &mut Events<'_>, store: &str) -> Outcome {
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
        return compact_postgresql(&record, &home, store);
    }
    match place::plan(&home, &store_ref)? {
        Executor::Local => {}
        Executor::Worker(_) => {
            return Err(refused(
                "Unsupported",
                format!(
                    "store.compact of Store {store} is local-only: run it on that Store's machine (S6 deviation #42 D5; the worker has no compaction route)"
                ),
            ))
        }
    }
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
        home: Some(home),
        store: Some(store.to_owned()),
        project: None,
        store_path: Some(path.display().to_string()),
        store_record: Some(record.to_record().clone()),
        postgresql: None,
    };
    let identity = identity::identity(&args)?;
    let handles = SqliteHandles::open_writable(
        PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Normal),
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
        .compact(&CompactRequest {
            op_id,
            owner_pid: Some(std::process::id()),
            owner_pid_start,
            owner_pid_boot,
            owner_runtime: Some(owner_runtime),
            owner_boot: Some(owner_boot),
            owner_live: Some(layerfs_home::process::recorded_owner_alive),
            failpoint: crash_failpoint(),
            ..CompactRequest::default()
        })
        .map_err(|error| {
            lookup::history_in(error, GcScopeKind::Store, store, None, Some(&op_id))
        })?;
    Ok(crate::report::compact_report(&receipt))
}

/// The §8 PostgreSQL path: dial the Store's recorded URL, open its schema as
/// this home's authority, and run Phase M/A/B/C. The Store is usable at
/// G + 1 from the swap on; a lost reply is judged by the next call (§9).
///
/// The PostgreSQL engine is always built beside SQLite (#726).
fn compact_postgresql(record: &StoreRecord, home: &Path, store: &str) -> Outcome {
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
        .compact(&CompactRequest {
            op_id,
            owner_pid: Some(std::process::id()),
            owner_pid_start,
            owner_pid_boot,
            owner_runtime: Some(owner_runtime),
            owner_boot: Some(owner_boot),
            ..CompactRequest::default()
        })
        .map_err(|error| {
            lookup::history_in(error, GcScopeKind::Store, store, None, Some(&op_id))
        })?;
    Ok(crate::report::compact_report(&receipt))
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
/// stays for the non-GC calls (`store.push`, `store.pull`, `store.remove`,
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
