//! `store.compact`: one sqlite Store's §7 rebuild, run in this process.
//!
//! The call takes the Store's lease, so a move or remove cannot start, and
//! closes the Store this process holds open: a call still running on it is
//! `InUse`, before any effect. It then opens the Store's own session, runs
//! the §7 (SQLite) or §8 (PostgreSQL) compaction and answers the §11 report.
//! A SQLite Store on another machine is `Unsupported` here: its rebuild runs
//! in that machine's worker.
use crate::identity;
#[cfg(feature = "postgresql")]
use crate::identity::Kind;
use crate::lease;
use crate::open::close;
use crate::open_args::OpenArgs;
use crate::place::{self, Executor};
#[cfg(feature = "postgresql")]
use crate::postgresql;
use crate::record::{StoreRecord, StoreRef};
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::events::Events;
use layerfs_home::home::paths;
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_persistence::compact_metrics::{self, CompactRefusal};
use layerfs_persistence::{
    CompactReport, CompactRequest, PersistenceConfig, SqliteHandles, SqlitePersistenceProfile,
};
#[cfg(feature = "postgresql")]
use layerfs_persistence::PostgresHandles;
use layerfs_store_service::history::{admission, lookup};
use std::path::Path;

/// `store.compact(name)`: reclaim one sqlite Store's dead bytes, answering
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
                format!("store.compact of Store {store} runs on its own machine, not this one"),
            ))
        }
    }
    let path = paths::store_file(&home, store);
    // A cached open in this process must end first; a call running on it is
    // `InUse`, before any effect.
    close(&path)?;
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
        PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        &identity.binding,
        identity.cursor_key,
    )
    .map_err(open_refusal)?;
    let (owner_pid_start, owner_pid_boot) = match layerfs_home::process::me() {
        Some((start, boot)) => (Some(start), Some(boot)),
        None => (None, None),
    };
    let receipt = handles
        .compact(&CompactRequest {
            op_id: admission::op_id(),
            owner_pid: Some(std::process::id()),
            owner_pid_start,
            owner_pid_boot,
            ..CompactRequest::default()
        })
        .map_err(lookup::history)?;
    // One serializer for the shared §11 report: the SQLite receipt projects
    // into `CompactReport`, and T9's PostgreSQL receipt will do the same.
    Ok(crate::report::compact_report(&CompactReport::from(receipt)))
}

/// `store.compact` of a PostgreSQL Store in a build without the `postgresql`
/// feature: no engine is linked, so the call is `Unsupported`.
#[cfg(not(feature = "postgresql"))]
fn compact_postgresql(_record: &StoreRecord, _home: &Path, _store: &str) -> Outcome {
    Err(refused(
        "Unsupported",
        "this LayerFS build has no PostgreSQL engine; build the binding with --features postgresql",
    ))
}

/// The §8 PostgreSQL path: dial the Store's recorded URL, open its schema as
/// this home's authority, and run Phase M/A/B/C. The Store is usable at
/// G + 1 from the swap on; a lost reply is judged by the next call (§9).
///
/// Only built with the `postgresql` feature; the stub above answers
/// `Unsupported` without it.
#[cfg(feature = "postgresql")]
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
    let receipt = handles
        .compact(&CompactRequest {
            op_id: admission::op_id(),
            owner_pid: Some(std::process::id()),
            owner_pid_start,
            owner_pid_boot,
            ..CompactRequest::default()
        })
        .map_err(lookup::history)?;
    Ok(crate::report::compact_report(&CompactReport::from(receipt)))
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
