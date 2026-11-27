//! `store upgrade` on a SQLite Store (#804 §5.1): one raw connection with
//! the Normal profile's settings, exclusive file access, the Writer lane
//! taken once, and the whole chain in one `BEGIN IMMEDIATE` transaction
//! checked before COMMIT. A cancel interrupts the running statement until
//! `commit` starts; anything after COMMIT that fails is a warning.
use super::format::{classify, read_header, refusal, with_suffix, Header};
use super::step::{self, error, Context, FLOOR};
use super::{frozen, verify};
use crate::backend::sqlite::lane::writer_admission::Writer;
use crate::upgrade::{
    run as run_chain, FormatEvidence, UpgradeError, UpgradeOutcome, UpgradeReport, UpgradeRequest,
    UpgradeWork, SQLITE_LATEST,
};
use crate::{PersistenceConfig, SqliteHandles, SqlitePersistenceProfile};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::Duration;

/// [`crate::upgrade::sqlite_format_evidence`].
pub(crate) fn evidence(path: &Path) -> Result<FormatEvidence, UpgradeError> {
    let (application_id, user_version) = read_header(path)?;
    let header = classify(application_id, user_version);
    let variant = match header {
        Header::Schema(2) => recorded_variant(path, user_version),
        _ => None,
    };
    Ok(FormatEvidence {
        format: super::format::format(header),
        raw: user_version,
        variant,
    })
}

/// The recorded-source variant of a schema-2 Store, as step 2 -> 3 would
/// recognize it (`"d"` or `"d-appended"`). When the WAL holds no committed
/// frame the main file is the whole state and is read on an immutable
/// connection (no lock, no sidecar); otherwise on a plain read-only
/// connection, which sees the WAL. Evidence only: an unreadable Store is no
/// variant.
fn recorded_variant(path: &Path, user_version: i64) -> Option<String> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI;
    let connection = match super::format::wal_committed(path).ok()? {
        false => {
            let uri = crate::backend::sqlite::compact::verify::immutable_uri(path);
            Connection::open_with_flags(uri, flags).ok()?
        }
        true => Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?,
    };
    connection.busy_timeout(Duration::ZERO).ok()?;
    connection.execute_batch("BEGIN").ok()?;
    let (source, definition): (String, String) = connection
        .query_row(
            "SELECT schema_source,schema_definition FROM history_meta WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok()?;
    let live = frozen::rows(&connection).ok()?;
    if source != frozen::source_v2(user_version) || definition != frozen::definition(&live) {
        return None;
    }
    let (fresh, migrated) = frozen::rows_v2(user_version).ok()?;
    frozen::recognize_rows(&live, &fresh, &migrated).map(str::to_owned)
}

/// The raw connection: read-write, no mutex, DEFENSIVE off (the step
/// rewrites `sqlite_schema`), no busy wait, then the Normal profile's
/// settings and the exclusive locking mode before the first read.
fn open(path: &Path) -> Result<Connection, UpgradeError> {
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let connection = Connection::open_with_flags(path, flags)
        .map_err(|e| UpgradeError::Failed(format!("opening {}: {e}", path.display())))?;
    connection
        .set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_DEFENSIVE, false)
        .map_err(|e| error(e, "DEFENSIVE off"))?;
    connection
        .busy_timeout(Duration::ZERO)
        .map_err(|e| error(e, "busy_timeout"))?;
    connection
        .execute_batch("PRAGMA locking_mode=EXCLUSIVE")
        .map_err(|e| error(e, "locking_mode"))?;
    Ok(connection)
}

/// The first statements after `locking_mode=EXCLUSIVE`: preparing them
/// reads the schema, which takes the exclusive file lock; a SHARED lock any
/// other open connection holds refuses it (`InUse`). Returns the header's
/// `application_id`.
fn first_read(connection: &Connection) -> Result<i64, UpgradeError> {
    let busy = |e: rusqlite::Error, what: &str| match e.sqlite_error_code() {
        Some(rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked) => {
            UpgradeError::InUse("another connection has the Store file open".to_owned())
        }
        _ => error(e, what),
    };
    connection
        .execute_batch(
            "PRAGMA synchronous=NORMAL; PRAGMA checkpoint_fullfsync=1; PRAGMA wal_autocheckpoint=0",
        )
        .map_err(|e| busy(e, "connection settings"))?;
    connection
        .query_row("PRAGMA application_id", [], |r| r.get(0))
        .map_err(|e| busy(e, "reading the header"))
}

/// [`crate::upgrade::upgrade_sqlite`].
pub(crate) fn upgrade(
    path: &Path,
    request: &UpgradeRequest<'_>,
    phase: &mut dyn FnMut(&'static str),
) -> Result<UpgradeReport, UpgradeError> {
    // The exclusive connection keeps its WAL index in heap memory and never
    // opens a `-shm`; one already here (a read-only session cannot remove
    // its sidecars when it closes) is not left by this run.
    let shm_before = with_suffix(path, "-shm").exists();
    let connection = open(path)?;
    let application_id = first_read(&connection)?;
    let lane = match Writer::try_acquire(path) {
        Ok(Some(lane)) => lane,
        Ok(None) => {
            drop(connection);
            return Err(UpgradeError::Busy(
                "the Store's Writer lane is held".to_owned(),
            ));
        }
        Err(e) => return Err(UpgradeError::Failed(format!("the Writer lane: {e:?}"))),
    };
    // `stopped` is held around every `interrupt()`: once the commit path
    // has set it (under the lock) no interrupt can be in flight or follow,
    // and one issued earlier, between statements, is cleared by SQLite when
    // the next statement starts with none active, so COMMIT cannot be
    // interrupted.
    let stopped = Mutex::new(false);
    let interrupt = connection.get_interrupt_handle();
    let outcome = std::thread::scope(|scope| {
        let watcher = scope.spawn(|| loop {
            {
                let stopped = stopped.lock().unwrap_or_else(|e| e.into_inner());
                if *stopped {
                    return;
                }
                if request.cancel.load(Ordering::SeqCst) {
                    interrupt.interrupt();
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        });
        let stop_watcher = || {
            *stopped.lock().unwrap_or_else(|e| e.into_inner()) = true;
        };
        let result = transaction(&connection, application_id, request, phase, stop_watcher);
        stop_watcher();
        let _ = watcher.join();
        result
    });
    match outcome {
        Ok(Done::Current(user_version)) => {
            let _ = connection.close();
            let _ = lane.release();
            Ok(report(
                UpgradeOutcome::Current,
                user_version,
                user_version,
                Vec::new(),
                0,
                Vec::new(),
            ))
        }
        Ok(Done::Committed {
            before,
            after,
            steps,
            binding,
        }) => {
            let mut warnings = Vec::new();
            match connection.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| {
                r.get::<_, i64>(0)
            }) {
                Ok(0) => {}
                Ok(_) => warnings.push("the WAL checkpoint after COMMIT was busy".to_owned()),
                Err(e) => warnings.push(format!("the WAL checkpoint after COMMIT failed: {e}")),
            }
            if let Err((_, e)) = connection.close() {
                warnings.push(format!("closing after COMMIT failed: {e}"));
            }
            if lane.release().is_err() {
                warnings.push("releasing the Writer lane failed".to_owned());
            }
            for (suffix, before) in [("-wal", false), ("-shm", shm_before)] {
                if !before && with_suffix(path, suffix).exists() {
                    warnings.push(format!("a {suffix} file is left beside the Store"));
                }
            }
            confirm(path, &binding, &mut warnings);
            Ok(report(
                UpgradeOutcome::Upgraded,
                before,
                after,
                steps,
                1,
                warnings,
            ))
        }
        Err(failure) => {
            // Never reuse a connection after a `writable_schema` change: roll
            // back and close it, then release the lane.
            let _ = connection.execute_batch("ROLLBACK");
            let _ = connection.close();
            let _ = lane.release();
            Err(failure)
        }
    }
}

enum Done {
    Current(i64),
    Committed {
        before: i64,
        after: i64,
        steps: Vec<String>,
        binding: Vec<u8>,
    },
}

/// `check`, `convert`, `verify` and the COMMIT, on the open connection.
fn transaction(
    connection: &Connection,
    application_id: i64,
    request: &UpgradeRequest<'_>,
    phase: &mut dyn FnMut(&'static str),
    stop_watcher: impl FnOnce(),
) -> Result<Done, UpgradeError> {
    phase("check");
    let mut context = Context::new(connection, 0, request.failpoint, request.cancel);
    context.exec("BEGIN IMMEDIATE")?;
    let user_version: i64 = connection
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(|e| error(e, "reading user_version"))?;
    let header = classify(application_id, user_version);
    if let Some(refused) = refusal(header, user_version) {
        return Err(refused);
    }
    if header == Header::Schema(SQLITE_LATEST) {
        connection
            .execute_batch("ROLLBACK")
            .map_err(|e| error(e, "ROLLBACK"))?;
        return Ok(Done::Current(SQLITE_LATEST));
    }
    context.user_version = user_version;
    let steps = step::steps();
    let format_before = match header {
        Header::Schema(schema) => schema,
        _ => FLOOR,
    };
    verify::no_live_intent(connection)?;
    let before = verify::values(connection)?;
    let rows_before = verify::rows(connection)?;
    let binding: Vec<u8> = connection
        .query_row("SELECT binding_key FROM history_meta WHERE id=1", [], |r| {
            r.get(0)
        })
        .map_err(|e| error(e, "reading the binding"))?;
    context.cancelled()?;

    phase("convert");
    let names = run_chain(&mut context, &steps, format_before, SQLITE_LATEST)?;
    let expected = frozen::rows_after(&rows_before, user_version, context.user_version)
        .ok_or_else(|| {
            UpgradeError::Failed("verify: no frozen target for the live schema".to_owned())
        })?;

    phase("verify");
    context.cancelled()?;
    verify::verify(connection, &before, &expected, context.user_version)?;
    context.cancelled()?;

    // The point of no return: the watcher stops before COMMIT and cancel is
    // not read again.
    stop_watcher();
    phase("commit");
    if let Err(failure) = context.exec_past_cancel("COMMIT") {
        return Err(match failure {
            // A test stop after the COMMIT: the transaction is committed.
            UpgradeError::Failed(text) if text.starts_with("failpoint") => {
                UpgradeError::Unknown(text)
            }
            // SQLite keeps the transaction open after a COMMIT it refused
            // (autocommit still off): provably not committed, and the
            // failure path rolls it back.
            other if !connection.is_autocommit() => UpgradeError::Failed(format!(
                "COMMIT was refused with the transaction still open, rolled back: {other:?}"
            )),
            other => UpgradeError::Unknown(format!("COMMIT: {other:?}")),
        });
    }
    Ok(Done::Committed {
        before: format_before,
        after: SQLITE_LATEST,
        steps: names,
        binding,
    })
}

/// The cursor key of the confirming opens: they read nothing a cursor
/// signs, but an open refuses the all-zero key.
const CONFIRM_CURSOR_KEY: [u8; 32] = [0x5a; 32];

/// After COMMIT: the current build's read-only and diagnostic opens
/// accept the Store; a refusal is a warning (§6).
fn confirm(path: &Path, binding: &[u8], warnings: &mut Vec<String>) {
    let config = PersistenceConfig::sqlite(path.to_path_buf())
        .with_sqlite_profile(SqlitePersistenceProfile::Normal);
    if let Err(e) = SqliteHandles::open_read_only(config, binding, CONFIRM_CURSOR_KEY) {
        warnings.push(format!("the read-only open after COMMIT failed: {e:?}"));
    }
    if let Err(e) = SqliteHandles::open_diagnostic(path, binding, CONFIRM_CURSOR_KEY) {
        warnings.push(format!("the diagnostic open after COMMIT failed: {e:?}"));
    }
}

fn report(
    outcome: UpgradeOutcome,
    format_before: i64,
    format_after: i64,
    steps: Vec<String>,
    write_transactions: u32,
    warnings: Vec<String>,
) -> UpgradeReport {
    UpgradeReport {
        outcome,
        engine: "sqlite",
        format_before,
        format_after,
        steps,
        warnings,
        work: UpgradeWork {
            connects: 0,
            round_trips: 0,
            write_transactions,
        },
    }
}
