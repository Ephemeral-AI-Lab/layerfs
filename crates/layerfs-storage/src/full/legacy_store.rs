use crate::integrity;
use crate::RequestId;
use crate::{
    admitted_store_id_counted, configure_profile_counted, map_sqlite_error, preflight_schema,
    CommitDispatch, ConnectionGuard, EngineError, EngineResult, SqliteErrorKind, SqliteProfile,
    BUSY_TIMEOUT,
};
use rusqlite::{params, Connection, OpenFlags};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EngineCounters {
    /// State-changing writer transactions. Admission/read transactions are
    /// reported separately below so publication equations stay exact.
    pub transactions_started: u64,
    pub transactions_committed: u64,
    pub transactions_rolled_back: u64,
    pub statements: u64,
    pub admission_transactions_started: u64,
    pub admission_transactions_committed: u64,
    pub admission_transactions_rolled_back: u64,
    pub admission_statements: u64,
    pub integrity_transactions_started: u64,
    pub integrity_transactions_committed: u64,
    pub integrity_transactions_rolled_back: u64,
    pub integrity_statements: u64,
    pub busy_events: u64,
    pub locked_events: u64,
    pub objects_validated: u64,
    pub objects_created: u64,
    pub objects_reused: u64,
    pub object_bytes_read: u64,
    pub object_bytes_written: u64,
    pub range_bytes_requested: u64,
    pub range_bytes_returned: u64,
    pub logical_object_bytes: u64,
    pub logical_root_bytes: u64,
    pub logical_delta_bytes: u64,
    pub retained_union_scrubs: u64,
    pub root_verifications: u64,
    pub root_verification_objects: u64,
    pub root_verification_bytes: u64,
    pub candidate_full_scans: u64,
    pub candidate_shallow_bindings: u64,
    pub fetch_closure_builds: u64,
    pub fetch_closure_pages: u64,
    pub fetched_rows: u64,
    pub fetched_row_authentication_passes: u64,
    pub fetched_row_role_decode_passes: u64,
    pub new_object_authentication_passes: u64,
    pub incumbent_authentication_passes: u64,
    pub payload_batch_queries: u64,
    pub payload_batch_references: u64,
    pub payload_batch_maximum: u64,
    pub put_lookup_statements: u64,
    pub put_insert_statements: u64,
    pub created_rows: u64,
    pub reused_rows: u64,
    pub publication_transactions_started: u64,
    pub publication_transactions_rolled_back: u64,
    pub publication_commits: u64,
    pub durable_head_transactions: u64,
    pub publication_closure_passes: u64,
    pub namespace_graph_verification_passes: u64,
    pub scratch_tables: u64,
    pub scratch_statements: u64,
    pub scratch_rows: u64,
    pub scratch_high_water_bytes: u64,
    pub retained_roots_validated: u64,
    pub publication_statements: u64,
    pub live_verified_integrity_statements: u64,
    pub primary_read_statements: u64,
    pub reconciliation_statements: u64,
    pub compaction_statements: u64,
    pub connection_mutex_wait_ns: u64,
    pub trust_guard_ns: u64,
    pub nonpayload_query_ns: u64,
    pub payload_query_ns: u64,
    pub identity_authentication_ns: u64,
    pub role_decode_ns: u64,
    pub payload_callback_inclusive_ns: u64,
    pub counter_merge_ns: u64,
    pub store_id_queries: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StorageObservation {
    pub database_bytes: Option<u64>,
    pub rollback_journal_bytes: Option<u64>,
    pub temporary_file_bytes: Option<u64>,
    pub logical_engine_bytes: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CompactionStorageObservation {
    pub old_generation_bytes: u64,
    pub new_generation_bytes: u64,
    pub mark_database_bytes: u64,
    pub candidate_journal_temp_peak_bytes: u64,
    pub verification_scratch_peak_bytes: u64,
    pub selector_temporary_bytes: u64,
    pub total_peak_bytes: u64,
}

pub type Storage = Engine;

pub struct Engine {
    pub(crate) path: PathBuf,
    pub(crate) store_id: [u8; 32],
    pub(crate) connection: Mutex<Option<Connection>>,
    pub(crate) counters: Mutex<EngineCounters>,
    pub(crate) rollback_journal_sample: Mutex<Option<u64>>,
    pub(crate) profile: SqliteProfile,
    pub(crate) mode: integrity::IntegrityMode,
    pub(crate) maintenance_pin: Option<Mutex<Connection>>,
    pub(crate) last_compaction: Option<CompactionStorageObservation>,
    pub(crate) commit_dispatch: std::sync::Arc<dyn CommitDispatch>,
    pub(crate) fetch_boundary_failure: AtomicBool,
    pub(crate) integrity_scope: AtomicBool,
    pub(crate) sql_family_scope: AtomicU8,
    pub(crate) timings: EngineTimings,
}

#[derive(Default)]
pub(crate) struct EngineTimings {
    pub(crate) connection_mutex_wait_ns: AtomicU64,
    pub(crate) trust_guard_ns: AtomicU64,
    pub(crate) nonpayload_query_ns: AtomicU64,
    pub(crate) payload_query_ns: AtomicU64,
    pub(crate) identity_authentication_ns: AtomicU64,
    pub(crate) role_decode_ns: AtomicU64,
    pub(crate) payload_callback_inclusive_ns: AtomicU64,
    pub(crate) counter_merge_ns: AtomicU64,
}

impl EngineTimings {
    pub(crate) fn reset(&self) {
        for timing in [
            &self.connection_mutex_wait_ns,
            &self.trust_guard_ns,
            &self.nonpayload_query_ns,
            &self.payload_query_ns,
            &self.identity_authentication_ns,
            &self.role_decode_ns,
            &self.payload_callback_inclusive_ns,
            &self.counter_merge_ns,
        ] {
            timing.store(0, Ordering::Relaxed);
        }
    }
}

#[derive(Default)]
pub(crate) struct BatchTimings {
    pub(crate) payload_query_ns: u64,
    pub(crate) identity_authentication_ns: u64,
    pub(crate) role_decode_ns: u64,
    pub(crate) payload_callback_inclusive_ns: u64,
    pub(crate) counter_merge_ns: u64,
}

impl BatchTimings {
    pub(crate) fn record(self, timings: &EngineTimings) {
        observe_value(&timings.payload_query_ns, self.payload_query_ns);
        observe_value(
            &timings.identity_authentication_ns,
            self.identity_authentication_ns,
        );
        observe_value(&timings.role_decode_ns, self.role_decode_ns);
        observe_value(
            &timings.payload_callback_inclusive_ns,
            self.payload_callback_inclusive_ns,
        );
        observe_value(&timings.counter_merge_ns, self.counter_merge_ns);
    }
}

pub(crate) const SQL_FAMILY_NONE: u8 = 0;
pub(crate) const SQL_FAMILY_PUBLICATION: u8 = 1;
pub(crate) const SQL_FAMILY_LIVE_INTEGRITY: u8 = 2;
pub(crate) const SQL_FAMILY_PRIMARY_READ: u8 = 3;
pub(crate) const SQL_FAMILY_RECONCILIATION: u8 = 4;
pub(crate) const SQL_FAMILY_COMPACTION: u8 = 5;

pub(crate) fn observe_time(target: &AtomicU64, started: Instant) {
    observe_value(target, elapsed_ns(started));
}

pub(crate) fn observe_value(target: &AtomicU64, elapsed: u64) {
    let _ = target.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        Some(current.saturating_add(elapsed))
    });
}

pub(crate) fn elapsed_ns(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

impl Engine {
    pub fn counters(&self) -> EngineResult<EngineCounters> {
        let mut counters = self
            .counters
            .lock()
            .map(|counters| *counters)
            .map_err(|_| EngineError::Sqlite {
                kind: SqliteErrorKind::Other,
                message: "counter mutex poisoned".to_owned(),
            })?;
        counters.connection_mutex_wait_ns = self
            .timings
            .connection_mutex_wait_ns
            .load(Ordering::Relaxed);
        counters.trust_guard_ns = self.timings.trust_guard_ns.load(Ordering::Relaxed);
        counters.nonpayload_query_ns = self.timings.nonpayload_query_ns.load(Ordering::Relaxed);
        counters.payload_query_ns = self.timings.payload_query_ns.load(Ordering::Relaxed);
        counters.identity_authentication_ns = self
            .timings
            .identity_authentication_ns
            .load(Ordering::Relaxed);
        counters.role_decode_ns = self.timings.role_decode_ns.load(Ordering::Relaxed);
        counters.payload_callback_inclusive_ns = self
            .timings
            .payload_callback_inclusive_ns
            .load(Ordering::Relaxed);
        counters.counter_merge_ns = self.timings.counter_merge_ns.load(Ordering::Relaxed);
        Ok(counters)
    }

    pub fn reset_counters(&self) -> EngineResult<()> {
        *self.counters.lock().map_err(|_| EngineError::Sqlite {
            kind: SqliteErrorKind::Other,
            message: "counter mutex poisoned".to_owned(),
        })? = EngineCounters::default();
        *self
            .rollback_journal_sample
            .lock()
            .map_err(|_| EngineError::Sqlite {
                kind: SqliteErrorKind::Other,
                message: "journal observation mutex poisoned".to_owned(),
            })? = None;
        self.timings.reset();
        Ok(())
    }

    pub fn observations(&self) -> StorageObservation {
        let database_bytes = fs::metadata(&self.path).ok().map(|metadata| metadata.len());
        let mut journal_path = self.path.as_os_str().to_os_string();
        journal_path.push("-journal");
        let rollback_journal_bytes = fs::metadata(PathBuf::from(journal_path))
            .ok()
            .map(|metadata| metadata.len())
            .or_else(|| {
                self.rollback_journal_sample
                    .lock()
                    .ok()
                    .and_then(|sample| *sample)
            });
        let logical_engine_bytes = self.logical_engine_bytes();
        StorageObservation {
            database_bytes,
            rollback_journal_bytes,
            temporary_file_bytes: None,
            logical_engine_bytes,
        }
    }

    fn logical_engine_bytes(&self) -> Option<u64> {
        let connection = self.connection.lock().ok()?;
        let connection = connection.as_ref()?;
        self.mark_family_sql(SQL_FAMILY_PRIMARY_READ, 1).ok()?;
        let objects = connection
            .query_row(
                "SELECT COALESCE(SUM(canonical_length), 0) FROM layerfs_objects",
                [],
                |row| row.get::<_, i64>(0),
            )
            .ok()
            .and_then(|value| u64::try_from(value).ok())?;
        self.mark_family_sql(SQL_FAMILY_PRIMARY_READ, 1).ok()?;
        let roots = connection
            .query_row(
                "SELECT COALESCE(SUM(64 + CASE WHEN parent_root IS NULL THEN 0 ELSE 32 END), 0)
                 FROM layerfs_roots",
                [],
                |row| row.get::<_, i64>(0),
            )
            .ok()
            .and_then(|value| u64::try_from(value).ok())?;
        self.mark_family_sql(SQL_FAMILY_PRIMARY_READ, 1).ok()?;
        let deltas = connection
            .query_row(
                "SELECT COALESCE(SUM(64 + length(payload) + CASE WHEN parent_root IS NULL THEN 0 ELSE 32 END), 0)
                 FROM layerfs_deltas",
                [],
                |row| row.get::<_, i64>(0),
            )
            .ok()
            .and_then(|value| u64::try_from(value).ok())?;
        objects.checked_add(roots)?.checked_add(deltas)
    }
}

pub(crate) fn mark_sql_family(
    counters: &mut EngineCounters,
    family: u8,
    statements: u64,
) -> EngineResult<()> {
    match family {
        SQL_FAMILY_NONE => Ok(()),
        SQL_FAMILY_PUBLICATION => checked_add(&mut counters.publication_statements, statements),
        SQL_FAMILY_LIVE_INTEGRITY => {
            checked_add(&mut counters.live_verified_integrity_statements, statements)
        }
        SQL_FAMILY_PRIMARY_READ => checked_add(&mut counters.primary_read_statements, statements),
        SQL_FAMILY_RECONCILIATION => {
            checked_add(&mut counters.reconciliation_statements, statements)
        }
        SQL_FAMILY_COMPACTION => checked_add(&mut counters.compaction_statements, statements),
        _ => Err(EngineError::InvalidRecord("SQL statement family")),
    }
}

pub(crate) fn checked_add(value: &mut u64, amount: u64) -> EngineResult<()> {
    *value = value
        .checked_add(amount)
        .ok_or(EngineError::CounterOverflow)?;
    Ok(())
}

pub(crate) fn begin_product_transaction(engine: &Engine) -> EngineResult<ConnectionGuard<'_>> {
    let mut connection = engine.lock_write_connection()?;
    if !connection.transaction {
        connection
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(map_sqlite_error)?;
        connection.transaction = true;
        engine.bump(|counters| checked_add(&mut counters.transactions_started, 1))?;
    }
    Ok(connection)
}

pub(crate) fn commit_product_state(
    engine: &Engine,
    connection: &mut ConnectionGuard<'_>,
    reconciliation_sql: &str,
    key: &[u8],
) -> EngineResult<bool> {
    match engine.commit_dispatch.commit(connection) {
        Ok(()) => {
            connection.transaction = false;
            engine.bump(|counters| checked_add(&mut counters.transactions_committed, 1))?;
            Ok(false)
        }
        Err(error) => {
            let _ = engine.note_sqlite_error(&error);
            let _ = engine.commit_dispatch.rollback(connection);
            connection.transaction = false;
            connection.guard.take();
            let fresh = Connection::open_with_flags(
                &engine.path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(map_sqlite_error)?;
            preflight_schema(&fresh)?;
            if fresh
                .query_row(reconciliation_sql, params![key], |row| {
                    row.get::<_, bool>(0)
                })
                .map_err(map_sqlite_error)?
            {
                restore_product_primary(engine, connection)?;
                engine.bump(|counters| checked_add(&mut counters.transactions_committed, 1))?;
                Ok(true)
            } else {
                Err(EngineError::AmbiguousDurability)
            }
        }
    }
}

pub(crate) fn commit_product_state_pair(
    engine: &Engine,
    connection: &mut ConnectionGuard<'_>,
    reconciliation_sql: &str,
    first: &[u8],
    second: &[u8],
) -> EngineResult<bool> {
    match engine.commit_dispatch.commit(connection) {
        Ok(()) => {
            connection.transaction = false;
            engine.bump(|counters| checked_add(&mut counters.transactions_committed, 1))?;
            Ok(false)
        }
        Err(error) => {
            let _ = engine.note_sqlite_error(&error);
            let _ = engine.commit_dispatch.rollback(connection);
            connection.transaction = false;
            connection.guard.take();
            let fresh = Connection::open_with_flags(
                &engine.path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(map_sqlite_error)?;
            preflight_schema(&fresh)?;
            if fresh
                .query_row(reconciliation_sql, params![first, second], |row| {
                    row.get::<_, bool>(0)
                })
                .map_err(map_sqlite_error)?
            {
                restore_product_primary(engine, connection)?;
                engine.bump(|counters| checked_add(&mut counters.transactions_committed, 1))?;
                Ok(true)
            } else {
                Err(EngineError::AmbiguousDurability)
            }
        }
    }
}

pub(crate) fn commit_product_request(
    engine: &Engine,
    connection: &mut ConnectionGuard<'_>,
    table: &str,
    request_id: RequestId,
) -> EngineResult<bool> {
    match engine.commit_dispatch.commit(connection) {
        Ok(()) => {
            connection.transaction = false;
            engine.bump(|counters| checked_add(&mut counters.transactions_committed, 1))?;
            Ok(false)
        }
        Err(error) => {
            let _ = engine.note_sqlite_error(&error);
            let _ = engine.commit_dispatch.rollback(connection);
            connection.transaction = false;
            connection.guard.take();
            let fresh = Connection::open_with_flags(
                &engine.path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .map_err(map_sqlite_error)?;
            preflight_schema(&fresh)?;
            let sql = format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE request_id = ?1)");
            if fresh
                .query_row(&sql, params![request_id.as_bytes()], |row| {
                    row.get::<_, bool>(0)
                })
                .map_err(map_sqlite_error)?
            {
                restore_product_primary(engine, connection)?;
                engine.bump(|counters| checked_add(&mut counters.transactions_committed, 1))?;
                Ok(true)
            } else {
                Err(EngineError::AmbiguousDurability)
            }
        }
    }
}

pub(crate) fn restore_product_primary(
    engine: &Engine,
    connection: &mut ConnectionGuard<'_>,
) -> EngineResult<()> {
    let reopened = Connection::open_with_flags(&engine.path, OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(map_sqlite_error)?;
    reopened
        .busy_timeout(BUSY_TIMEOUT)
        .map_err(map_sqlite_error)?;
    preflight_schema(&reopened)?;
    let mut statements = 0;
    let profile = configure_profile_counted(&reopened, &mut statements)?;
    if profile != engine.profile {
        return Err(EngineError::ProfileMismatch);
    }
    if admitted_store_id_counted(&reopened, &mut statements)? != engine.store_id {
        return Err(EngineError::InvalidRecord("reconciliation StorageId"));
    }
    *connection.guard = Some(reopened);
    Ok(())
}

pub(crate) fn rollback_product_transaction<T>(
    engine: &Engine,
    connection: &mut ConnectionGuard<'_>,
    _result: &EngineResult<T>,
) {
    if connection.transaction {
        if engine.commit_dispatch.rollback(connection).is_ok() {
            let _ = engine.bump(|counters| checked_add(&mut counters.transactions_rolled_back, 1));
        }
        connection.transaction = false;
    }
}
