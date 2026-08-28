use crate::error::{engine_step, map_sqlite_error, EngineError, EngineResult, SqliteErrorKind};
use crate::full::compaction::verify::verify_product_integrity;
use crate::full::legacy_store::{
    checked_add, CompactionStorageObservation, Engine, EngineCounters, EngineTimings,
    SQL_FAMILY_NONE, SQL_FAMILY_RECONCILIATION,
};
use crate::integrity;
use crate::refs;
use crate::schema::{admitted_store_id_counted, initialize_schema_counted, note_statement};
use crate::scratch;
use crate::sqlite::admission::{admit_schema_counted, preflight_schema, preflight_schema_counted};
#[cfg(any(test, feature = "test-hooks"))]
use crate::sqlite::connection::LostCommitAcknowledgementHook;
use crate::sqlite::connection::SqliteCommit;
use crate::sqlite::profile::{configure_profile_counted, SqliteProfile, BUSY_TIMEOUT};
#[cfg(any(test, feature = "test-hooks"))]
use layerfs_core::ObjectId;
#[cfg(any(test, feature = "test-hooks"))]
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OpenFlags;
use std::cell::Cell;
use std::path::Path;
#[cfg(any(test, feature = "test-hooks"))]
use std::sync::atomic::Ordering;
use std::sync::atomic::{AtomicBool, AtomicU8};
use std::sync::Mutex;

// ponytail: serialize Verified admission in-process; use per-Store locks only if
// parallel open throughput becomes a measured requirement.
static VERIFIED_OPEN_LOCK: Mutex<()> = Mutex::new(());

impl Engine {
    pub fn open(path: impl AsRef<Path>) -> EngineResult<Self> {
        Self::open_with_mode(path, integrity::IntegrityMode::Verified)
    }

    pub fn open_with_mode(
        path: impl AsRef<Path>,
        mode: integrity::IntegrityMode,
    ) -> EngineResult<Self> {
        let path = path.as_ref().to_owned();
        let connection = Connection::open(&path)
            .map_err(map_sqlite_error)
            .map_err(|error| engine_step("primary open", error))?;
        connection
            .busy_timeout(BUSY_TIMEOUT)
            .map_err(map_sqlite_error)?;
        let mut admission_statements = 0;
        let schema_state = admit_schema_counted(&connection, &mut admission_statements)
            .map_err(|error| engine_step("primary preflight", error))?;
        let profile = configure_profile_counted(&connection, &mut admission_statements)
            .map_err(|error| engine_step("profile", error))?;
        initialize_schema_counted(
            &connection,
            &profile,
            schema_state,
            &mut admission_statements,
        )
        .map_err(|error| engine_step("schema initialization", error))?;
        preflight_schema_counted(&connection, &mut admission_statements)
            .map_err(|error| engine_step("migrated schema admission", error))?;
        let store_id = admitted_store_id_counted(&connection, &mut admission_statements)
            .map_err(|error| engine_step("StoreId admission", error))?;
        let scrub = if mode == integrity::IntegrityMode::Verified {
            let _admission = VERIFIED_OPEN_LOCK.lock().map_err(|_| EngineError::Sqlite {
                kind: SqliteErrorKind::Other,
                message: "Verified admission mutex poisoned".to_owned(),
            })?;
            Some(
                initial_verified_scrub(&connection, &path, store_id).map_err(|failure| {
                    let _observation = failure.observation;
                    engine_step("initial verified scrub", failure.error)
                })?,
            )
        } else {
            note_statement(&mut admission_statements)?;
            mark_known_trusted_history(&connection)?;
            None
        };
        let mut counters = EngineCounters {
            admission_statements,
            store_id_queries: 1,
            ..EngineCounters::default()
        };
        if let Some(scrub) = scrub {
            checked_add(
                &mut counters.admission_transactions_started,
                scrub.transactions_started,
            )?;
            checked_add(
                &mut counters.admission_transactions_committed,
                scrub.transactions_committed,
            )?;
            checked_add(
                &mut counters.admission_transactions_rolled_back,
                scrub.transactions_rolled_back,
            )?;
            checked_add(&mut counters.admission_statements, scrub.statements)?;
            if let Some(observation) = scrub.verification {
                add_retained_scrub_counters(&mut counters, observation)?;
            }
        }
        Ok(Self {
            path,
            store_id,
            connection: Mutex::new(Some(connection)),
            counters: Mutex::new(counters),
            rollback_journal_sample: Mutex::new(None),
            profile,
            mode,
            maintenance_pin: None,
            last_compaction: None,
            commit_dispatch: std::sync::Arc::new(SqliteCommit),
            fetch_boundary_failure: AtomicBool::new(false),
            integrity_scope: AtomicBool::new(false),
            sql_family_scope: AtomicU8::new(SQL_FAMILY_NONE),
            timings: EngineTimings::default(),
        })
    }
}

impl Engine {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn profile(&self) -> &SqliteProfile {
        &self.profile
    }

    pub fn last_compaction_observation(&self) -> Option<CompactionStorageObservation> {
        self.last_compaction
    }

    pub fn store_id(&self) -> EngineResult<[u8; 32]> {
        Ok(self.store_id)
    }

    pub const fn store_id_cached(&self) -> [u8; 32] {
        self.store_id
    }

    pub fn create_scratch_table(&self, label: &str) -> EngineResult<scratch::DiskTable> {
        scratch::DiskTable::create_near_with_store_id(&self.path, label, self.store_id)
    }

    pub fn active_connection_count(&self) -> EngineResult<u64> {
        Ok(u64::from(
            self.connection
                .lock()
                .map_err(|_| EngineError::InvalidRecord("connection lock"))?
                .is_some(),
        ))
    }

    pub fn close_primary_connection(&self) -> EngineResult<()> {
        self.connection
            .lock()
            .map_err(|_| EngineError::InvalidRecord("connection lock"))?
            .take();
        Ok(())
    }

    #[cfg(any(test, feature = "test-hooks"))]
    #[doc(hidden)]
    pub fn inject_lost_commit_acknowledgement(&mut self) {
        self.commit_dispatch = std::sync::Arc::new(LostCommitAcknowledgementHook);
    }

    #[cfg(any(test, feature = "test-hooks"))]
    #[doc(hidden)]
    pub fn inject_fetch_boundary_failure(&self) {
        self.fetch_boundary_failure.store(true, Ordering::Release);
    }

    #[cfg(any(test, feature = "test-hooks"))]
    #[doc(hidden)]
    pub fn corrupt_object_for_test(&self, id: ObjectId, canonical: &[u8]) -> EngineResult<()> {
        let mut connection = self.lock_write_connection()?;
        let changed = connection
            .execute(
                "UPDATE layerfs_objects SET canonical_bytes = ?1 WHERE object_id = ?2",
                params![canonical, id.as_bytes().as_slice()],
            )
            .map_err(map_sqlite_error)?;
        if changed != 1 {
            return Err(EngineError::MissingObject(id));
        }
        if connection.transaction {
            connection
                .execute_batch("COMMIT")
                .map_err(map_sqlite_error)?;
            connection.transaction = false;
        }
        Ok(())
    }
}

pub(crate) fn inspect_store_id_readonly(path: &Path) -> EngineResult<[u8; 32]> {
    open_store_readonly(path).map(|(_, store_id)| store_id)
}

fn open_store_readonly(path: &Path) -> EngineResult<(Connection, [u8; 32])> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(map_sqlite_error)?;
    connection
        .busy_timeout(BUSY_TIMEOUT)
        .map_err(map_sqlite_error)?;
    preflight_schema(&connection)?;
    let bytes = connection
        .query_row(
            "SELECT store_id FROM layerfs_authority WHERE authority_id = 1",
            [],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .map_err(map_sqlite_error)?;
    let store_id = bytes
        .try_into()
        .map_err(|_| EngineError::InvalidRecord("StoreId"))?;
    Ok((connection, store_id))
}

pub(crate) fn read_ref_reconcile_readonly(
    engine: &Engine,
    name: &str,
    expected_store_id: [u8; 32],
) -> EngineResult<Option<refs::RefState>> {
    let mut statements = 0;
    let result = (|| {
        let connection =
            Connection::open_with_flags(&engine.path, OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(map_sqlite_error)?;
        connection
            .busy_timeout(BUSY_TIMEOUT)
            .map_err(map_sqlite_error)?;
        preflight_schema_counted(&connection, &mut statements)?;
        note_statement(&mut statements)?;
        let bytes = connection
            .query_row(
                "SELECT store_id FROM layerfs_authority WHERE authority_id = 1",
                [],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .map_err(map_sqlite_error)?;
        let store_id: [u8; 32] = bytes
            .try_into()
            .map_err(|_| EngineError::InvalidRecord("StoreId"))?;
        if store_id != expected_store_id {
            return Err(EngineError::InvalidRecord("reconciliation StoreId"));
        }
        note_statement(&mut statements)?;
        refs::read_ref_on_connection(&connection, name)
    })();
    engine.mark_family_sql(SQL_FAMILY_RECONCILIATION, statements)?;
    result
}

pub(crate) fn reopen_store_primary(
    engine: &Engine,
    expected_store_id: [u8; 32],
    ref_name: &str,
    expected_ref: &Option<refs::RefState>,
) -> EngineResult<Connection> {
    let mut statements = 0;
    let result = (|| {
        let connection =
            Connection::open_with_flags(&engine.path, OpenFlags::SQLITE_OPEN_READ_WRITE)
                .map_err(map_sqlite_error)?;
        connection
            .busy_timeout(BUSY_TIMEOUT)
            .map_err(map_sqlite_error)?;
        preflight_schema_counted(&connection, &mut statements)?;
        note_statement(&mut statements)?;
        let store_id = connection
            .query_row(
                "SELECT store_id FROM layerfs_authority WHERE authority_id = 1",
                [],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .map_err(map_sqlite_error)?;
        if store_id.as_slice() != expected_store_id {
            return Err(EngineError::InvalidRecord("reopened StoreId"));
        }
        note_statement(&mut statements)?;
        if &refs::read_ref_on_connection(&connection, ref_name)? != expected_ref {
            return Err(EngineError::AmbiguousDurability);
        }
        let profile = configure_profile_counted(&connection, &mut statements)?;
        Ok((connection, profile))
    })();
    engine.mark_family_sql(SQL_FAMILY_RECONCILIATION, statements)?;
    result.map(|(connection, _)| connection)
}

pub(crate) fn clear_known_trusted_history(connection: &Connection) -> EngineResult<()> {
    connection
        .execute(
            "UPDATE layerfs_authority SET trusted_history = 0 WHERE authority_id = 1",
            [],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

pub(crate) fn mark_known_trusted_history(connection: &Connection) -> EngineResult<()> {
    connection
        .execute(
            "UPDATE layerfs_authority SET trusted_history = 1 WHERE authority_id = 1",
            [],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

#[derive(Debug, Default)]
pub(crate) struct InitialScrubObservation {
    pub(crate) verification: Option<integrity::VerificationObservation>,
    pub(crate) failed_verification: integrity::VerificationObservation,
    pub(crate) transactions_started: u64,
    pub(crate) transactions_committed: u64,
    pub(crate) transactions_rolled_back: u64,
    pub(crate) statements: u64,
}

pub(crate) struct InitialScrubFailure {
    pub(crate) error: EngineError,
    pub(crate) observation: InitialScrubObservation,
}

pub(crate) fn initial_verified_scrub(
    connection: &Connection,
    path: &Path,
    store_id: [u8; 32],
) -> Result<InitialScrubObservation, Box<InitialScrubFailure>> {
    let mut observation = InitialScrubObservation {
        statements: 1,
        ..InitialScrubObservation::default()
    };
    if let Err(error) = connection.execute_batch("BEGIN IMMEDIATE") {
        return Err(Box::new(InitialScrubFailure {
            error: map_sqlite_error(error),
            observation,
        }));
    }
    observation.transactions_started = 1;
    match verify_product_integrity(connection) {
        Ok(statements) => observation.statements += statements,
        Err(error) => return Err(initial_scrub_rollback(connection, error, observation)),
    }
    observation.statements += 1;
    let dirty = match trusted_history(connection) {
        Ok(dirty) => dirty,
        Err(error) => return Err(initial_scrub_rollback(connection, error, observation)),
    };
    if dirty {
        let statements = Cell::new(0);
        let failed = Cell::new(integrity::VerificationObservation::default());
        let verified = integrity::verify_retained_union_observed_counted(
            connection,
            path,
            store_id,
            &statements,
            &failed,
        );
        observation.statements += statements.get();
        let verified = match verified {
            Ok(verified) => verified,
            Err(error) => {
                observation.failed_verification = failed.get();
                return Err(initial_scrub_rollback(connection, error, observation));
            }
        };
        observation.statements += 1;
        if let Err(error) = clear_known_trusted_history(connection) {
            return Err(initial_scrub_rollback(connection, error, observation));
        }
        observation.verification = Some(verified.verification);
    }
    observation.statements += 1;
    if let Err(error) = connection.execute_batch("COMMIT") {
        return Err(Box::new(InitialScrubFailure {
            error: map_sqlite_error(error),
            observation,
        }));
    }
    observation.transactions_committed = 1;
    Ok(observation)
}

fn initial_scrub_rollback(
    connection: &Connection,
    error: EngineError,
    mut observation: InitialScrubObservation,
) -> Box<InitialScrubFailure> {
    observation.statements += 1;
    if connection.execute_batch("ROLLBACK").is_ok() {
        observation.transactions_rolled_back = 1;
    }
    Box::new(InitialScrubFailure { error, observation })
}

pub(crate) fn add_retained_scrub_counters(
    counters: &mut EngineCounters,
    observation: integrity::VerificationObservation,
) -> EngineResult<()> {
    checked_add(&mut counters.retained_union_scrubs, 1)?;
    add_verification_progress_counters(counters, observation)?;
    checked_add(
        &mut counters.namespace_graph_verification_passes,
        observation.namespace_graphs,
    )?;
    checked_add(
        &mut counters.retained_roots_validated,
        observation.retained_roots_validated,
    )?;
    Ok(())
}

pub(crate) fn add_verification_progress_counters(
    counters: &mut EngineCounters,
    observation: integrity::VerificationObservation,
) -> EngineResult<()> {
    checked_add(
        &mut counters.objects_validated,
        observation.authentication_passes,
    )?;
    checked_add(&mut counters.object_bytes_read, observation.bytes)?;
    checked_add(&mut counters.fetched_rows, observation.fetched_rows)?;
    checked_add(
        &mut counters.fetched_row_authentication_passes,
        observation.authentication_passes,
    )?;
    checked_add(
        &mut counters.fetched_row_role_decode_passes,
        observation.role_decode_passes,
    )?;
    add_verification_scratch_counters(counters, observation)
}

fn add_verification_scratch_counters(
    counters: &mut EngineCounters,
    observation: integrity::VerificationObservation,
) -> EngineResult<()> {
    checked_add(&mut counters.scratch_tables, observation.scratch_tables)?;
    checked_add(
        &mut counters.scratch_statements,
        observation.scratch_statements,
    )?;
    checked_add(&mut counters.scratch_rows, observation.scratch_rows)?;
    counters.scratch_high_water_bytes = counters
        .scratch_high_water_bytes
        .max(observation.scratch_bytes);
    Ok(())
}

pub(crate) fn trusted_history(connection: &Connection) -> EngineResult<bool> {
    connection
        .query_row(
            "SELECT trusted_history FROM layerfs_authority WHERE authority_id = 1",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(map_sqlite_error)
}
