//! Raw engine evidence when schema/authority validation could not open handles.
use super::{emit, Observation, Options, Outcome};
use crate::backend::{
    records::BackendError,
    sqlite::session::{connection::open_admitted_read_only, rows},
};
use layerfs_storage::port::PersistenceError;
use rusqlite::{Error, ErrorCode};
use std::{path::Path, time::Duration};

/// Audits SQLite structure without assuming a supported LayerFS schema.
/// Used after a diagnostic handle open fails: only engine-confirmed structural
/// evidence is a failure; permission, contention and unsupported schemas are
/// not evidence of database corruption. No authority/content checks are claimed.
pub fn sqlite_structure(
    path: &Path,
    options: Options,
    observer: &mut dyn FnMut(Observation),
) -> Result<(), PersistenceError> {
    options.check()?;
    // The Store open rule: with no `-shm` (no connection holds the Store),
    // the lane is waited for and held through the first read, which creates
    // the sidecars, then released before the check runs.
    let (connection, writer, _) =
        open_admitted_read_only(path, &mut |error| evidence(error, observer))?;
    if let Some(writer) = writer {
        writer.release().map_err(|_| BackendError::Unknown)?;
    }
    let interrupt = connection.get_interrupt_handle();
    let (done, ended) = std::sync::mpsc::channel::<()>();
    std::thread::scope(|scope| {
        let watcher = (options.deadline.is_some() || options.cancelled.is_some()).then(|| {
            scope.spawn(move || loop {
                if options.check().is_err() {
                    interrupt.interrupt();
                    break;
                }
                let wait = options
                    .deadline
                    .map_or(Duration::from_millis(20), |deadline| {
                        deadline
                            .saturating_duration_since(std::time::Instant::now())
                            .min(Duration::from_millis(20))
                    });
                match ended.recv_timeout(wait) {
                    Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                }
            })
        });
        let result = (|| {
            options.check()?;
            connection.execute_batch("PRAGMA query_only=ON; PRAGMA cache_size=-2048; PRAGMA mmap_size=0; PRAGMA temp_store=FILE; BEGIN;")
                .map_err(|error| classify(error, observer))?;
            let sql = if options.quick {
                "PRAGMA quick_check(100)"
            } else {
                "PRAGMA integrity_check(100)"
            };
            let mut statement = connection
                .prepare(sql)
                .map_err(|error| classify(error, observer))?;
            let mut result = statement
                .query([])
                .map_err(|error| classify(error, observer))?;
            let mut failures = 0;
            let mut checked = 0;
            while let Some(row) = result.next().map_err(|error| classify(error, observer))? {
                options.check()?;
                checked += 1;
                let message: String = row.get(0).map_err(|error| classify(error, observer))?;
                if message != "ok" {
                    failures += 1;
                    emit(observer, "store.database.structure", Outcome::Fail, format!("supplemental physical observation: {message}; original logical snapshot unverified"));
                }
            }
            options.check()?;
            if checked == 0 {
                emit(
                    observer,
                    "store.database.structure.coverage",
                    Outcome::Unknown,
                    "supplemental SQLite structural probe returned no evidence",
                );
            } else if failures == 0 {
                emit(observer,"store.database.structure",Outcome::Pass,"supplemental separate read-only SQLite structural check completed; original logical snapshot/schema/authority/data coverage remain unverified");
            } else if failures >= 100 {
                emit(
                    observer,
                    "store.database.structure.coverage",
                    Outcome::Unknown,
                    "SQLite structural findings reached limit 100; additional damage may exist",
                );
            }
            Ok::<(), PersistenceError>(())
        })();
        let _ = done.send(());
        if watcher.is_some_and(|thread| thread.join().is_err()) {
            return Err(PersistenceError::Uncertain);
        }
        options.check()?;
        result
    })
}
fn classify(error: Error, observer: &mut dyn FnMut(Observation)) -> PersistenceError {
    evidence(error, observer).into()
}
/// The engine error, after emitting a structural failure SQLite confirmed.
fn evidence(error: Error, observer: &mut dyn FnMut(Observation)) -> BackendError {
    if let Error::SqliteFailure(code, _) = &error {
        let message=match code.code {
            ErrorCode::DatabaseCorrupt=>Some("SQLite reported SQLITE_CORRUPT while reading the original Store"),
            ErrorCode::NotADatabase=>Some("SQLite reported SQLITE_NOTADB: the selected Store file is not a readable SQLite database"),
            _=>None,
        };
        if let Some(message) = message {
            emit(observer, "store.database.structure", Outcome::Fail, format!("supplemental physical observation: {message}; original logical snapshot unverified"));
        }
    }
    rows::error(error)
}
