use crate::statements;
use crate::{BranchId, Result, StoreError};
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

pub const APPLICATION_ID: i64 = 0x4c46_534c;
pub const SCHEMA_VERSION: i64 = 5;
const PREVIOUS_SCHEMA_VERSION: i64 = 4;
pub const SQLITE_PAGE_SIZE_BYTES: i64 = 64 * 1024;
pub const SQLITE_PAGE_CACHE_KIB: i64 = 32 * 1024;

#[cfg(feature = "test-instrumentation")]
thread_local! {
    static SQL_TRACE: std::cell::RefCell<Option<Vec<String>>> = const { std::cell::RefCell::new(None) };
}

#[cfg(debug_assertions)]
thread_local! {
    static TRANSACTION_FAILURE_AT: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
}

#[cfg(feature = "test-instrumentation")]
fn trace_sql(event: rusqlite::trace::TraceEvent<'_>) {
    if let rusqlite::trace::TraceEvent::Stmt(_, sql) = event {
        SQL_TRACE.with(|trace| {
            if let Some(history) = trace.borrow_mut().as_mut() {
                history.push(sql.to_owned());
            }
        });
    }
}

#[cfg(feature = "test-instrumentation")]
pub fn reset_sql_trace() {
    // Explicit test opt-in. Feature-enabled runtimes retain no SQL history by default.
    SQL_TRACE.with(|trace| *trace.borrow_mut() = Some(Vec::new()));
}

#[cfg(feature = "test-instrumentation")]
pub fn sql_trace() -> Vec<String> {
    SQL_TRACE.with(|trace| trace.borrow().clone().unwrap_or_default())
}

#[cfg(debug_assertions)]
pub fn set_transaction_failure_at(statement: Option<u64>) {
    TRANSACTION_FAILURE_AT.with(|failure| failure.set(statement));
}

#[cfg(debug_assertions)]
pub(crate) fn fail_transaction_statement(statement: u64) -> Result<()> {
    if TRANSACTION_FAILURE_AT.with(|failure| failure.get()) == Some(statement) {
        return Err(StoreError::Integrity("injected transaction failure"));
    }
    Ok(())
}

#[cfg(not(debug_assertions))]
pub(crate) fn fail_transaction_statement(_statement: u64) -> Result<()> {
    Ok(())
}

#[derive(Clone)]
pub(crate) struct StoreDb(Arc<StoreInner>);

struct StoreInner {
    connection: Mutex<Connection>,
    gate: TicketGate,
    leases: Mutex<BTreeSet<BranchId>>,
    path: PathBuf,
}

struct CreatedStoreFile {
    path: PathBuf,
    remove: bool,
}

impl Drop for CreatedStoreFile {
    fn drop(&mut self) {
        if self.remove {
            let _ = std::fs::remove_file(&self.path);
            let _ = std::fs::remove_file(appended(&self.path, "-journal"));
        }
    }
}

#[derive(Default)]
struct TicketState {
    next: u64,
    serving: u64,
}

#[derive(Default)]
struct TicketGate {
    state: Mutex<TicketState>,
    ready: Condvar,
}

pub(crate) struct OperationPermit<'a> {
    gate: &'a TicketGate,
}

impl Drop for OperationPermit<'_> {
    fn drop(&mut self) {
        if let Ok(mut state) = self.gate.state.lock() {
            state.serving += 1;
            self.gate.ready.notify_all();
        }
    }
}

impl TicketGate {
    fn enter(&self) -> Result<OperationPermit<'_>> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| StoreError::Integrity("operation gate"))?;
        let ticket = state.next;
        state.next += 1;
        while state.serving != ticket {
            state = self
                .ready
                .wait(state)
                .map_err(|_| StoreError::Integrity("operation gate"))?;
        }
        Ok(OperationPermit { gate: self })
    }
}

pub(crate) struct BranchLease {
    store: StoreDb,
    branch_id: BranchId,
}

impl Drop for BranchLease {
    fn drop(&mut self) {
        if let Ok(mut leases) = self.store.0.leases.lock() {
            leases.remove(&self.branch_id);
        }
    }
}

impl StoreDb {
    pub fn create(path: impl AsRef<Path>) -> Result<Self> {
        Self::open(path.as_ref(), OpenMode::Create)
    }

    pub fn connect(path: impl AsRef<Path>) -> Result<Self> {
        Self::open(path.as_ref(), OpenMode::Connect)
    }

    fn open(path: &Path, mode: OpenMode) -> Result<Self> {
        let path = absolute(path)?;
        if mode == OpenMode::Create && path.exists() {
            return Err(StoreError::StoreAlreadyExists);
        }
        if mode == OpenMode::Connect && !path.is_file() {
            return Err(StoreError::StoreMissing);
        }
        if mode == OpenMode::Create {
            std::fs::create_dir_all(
                path.parent()
                    .ok_or(StoreError::InvalidInput("Store location"))?,
            )?;
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|error| {
                    if error.kind() == std::io::ErrorKind::AlreadyExists {
                        StoreError::StoreAlreadyExists
                    } else {
                        StoreError::Io(error)
                    }
                })?;
        }
        let mut created = (mode == OpenMode::Create).then(|| CreatedStoreFile {
            path: path.clone(),
            remove: true,
        });
        let existing_version = (mode == OpenMode::Connect)
            .then(|| preflight_connect(&path))
            .transpose()?;
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let mut connection = Connection::open_with_flags(&path, flags)?;
        if mode == OpenMode::Create {
            connection.pragma_update(None, "page_size", SQLITE_PAGE_SIZE_BYTES)?;
        }
        configure_connection(&connection)?;
        if mode == OpenMode::Create {
            connection.execute_batch(statements::schema::V5)?;
        }
        acquire_exclusive_lock(&mut connection)?;
        if existing_version == Some(PREVIOUS_SCHEMA_VERSION) {
            migrate_v4_to_v5(&mut connection)?;
        }
        verify_schema(&connection, SCHEMA_VERSION)?;
        prepare_manifest(&connection)?;
        #[cfg(feature = "test-instrumentation")]
        {
            connection.trace_v2(
                rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
                Some(trace_sql),
            );
        }
        let store = Self(Arc::new(StoreInner {
            connection: Mutex::new(connection),
            gate: TicketGate::default(),
            leases: Mutex::new(BTreeSet::new()),
            path,
        }));
        if let Some(created) = &mut created {
            created.remove = false;
        }
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.0.path
    }

    pub fn enter_operation(&self) -> Result<OperationPermit<'_>> {
        self.0.gate.enter()
    }

    pub fn acquire_workspace_lease(&self, branch_id: BranchId) -> Result<Option<BranchLease>> {
        let mut leases = self
            .0
            .leases
            .lock()
            .map_err(|_| StoreError::Integrity("workspace lease"))?;
        if !leases.insert(branch_id) {
            return Ok(None);
        }
        Ok(Some(BranchLease {
            store: self.clone(),
            branch_id,
        }))
    }

    pub fn writer(&self) -> Result<MutexGuard<'_, Connection>> {
        self.0
            .connection
            .lock()
            .map_err(|_| StoreError::Integrity("Store connection"))
    }

    pub fn reader(&self) -> Result<MutexGuard<'_, Connection>> {
        self.writer()
    }

    pub fn data_version(&self) -> Result<u64> {
        let value: i64 = self
            .reader()?
            .pragma_query_value(None, "data_version", |row| row.get(0))?;
        value
            .try_into()
            .map_err(|_| StoreError::Integrity("SQLite data version"))
    }
}

fn preflight_connect(path: &Path) -> Result<i64> {
    let mut header = [0; 20];
    let mut file = std::fs::File::open(path)?;
    if file.metadata()?.len() < header.len() as u64 {
        return Err(StoreError::WrongStoreSchema);
    }
    file.read_exact(&mut header)?;
    if header[18] == 2 || header[19] == 2 {
        return Err(StoreError::WrongStoreSchema);
    }
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let journal: String = connection.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
    if journal.eq_ignore_ascii_case("wal") {
        return Err(StoreError::WrongStoreSchema);
    }
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if !matches!(version, PREVIOUS_SCHEMA_VERSION | SCHEMA_VERSION) {
        return Err(StoreError::WrongStoreSchema);
    }
    verify_schema(&connection, version)?;
    Ok(version)
}

fn configure_connection(connection: &Connection) -> Result<()> {
    if rusqlite::version_number() < 3_037_000 {
        return Err(StoreError::Integrity("SQLite STRICT support"));
    }
    connection.pragma_update(None, "foreign_keys", true)?;
    connection.pragma_update(None, "journal_mode", "MEMORY")?;
    let journal: String = connection.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
    if !journal.eq_ignore_ascii_case("memory") {
        return Err(StoreError::WrongStoreSchema);
    }
    connection.pragma_update(None, "synchronous", "OFF")?;
    connection.pragma_update(None, "temp_store", "MEMORY")?;
    connection.pragma_update(None, "cache_size", -SQLITE_PAGE_CACHE_KIB)?;
    connection.pragma_update(None, "cache_spill", "OFF")?;
    connection.pragma_update(None, "mmap_size", 0_i64)?;
    connection.pragma_update(None, "threads", 0_i64)?;
    connection.pragma_update(None, "locking_mode", "EXCLUSIVE")?;
    let locking: String = connection.pragma_query_value(None, "locking_mode", |row| row.get(0))?;
    if !locking.eq_ignore_ascii_case("exclusive") {
        return Err(StoreError::WrongStoreSchema);
    }
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(())
}

fn acquire_exclusive_lock(connection: &mut Connection) -> Result<()> {
    connection
        .transaction_with_behavior(TransactionBehavior::Immediate)?
        .commit()?;
    Ok(())
}

fn verify_schema(connection: &Connection, version: i64) -> Result<()> {
    let application_id: i64 =
        connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
    let user_version: i64 =
        connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let page_size: i64 = connection.pragma_query_value(None, "page_size", |row| row.get(0))?;
    if application_id != APPLICATION_ID
        || user_version != version
        || page_size != SQLITE_PAGE_SIZE_BYTES
    {
        return Err(StoreError::WrongStoreSchema);
    }
    if schema_objects(connection)? != expected_schema_objects(version)? {
        return Err(StoreError::WrongStoreSchema);
    }
    let mut foreign_keys = connection.prepare(statements::schema::FOREIGN_KEY_CHECK)?;
    if foreign_keys.exists([])? {
        return Err(StoreError::Integrity("foreign key check"));
    }
    Ok(())
}

fn prepare_manifest(connection: &Connection) -> Result<()> {
    for (name, sql) in statements::ALL {
        if matches!(
            *name,
            "schema/v4.sql" | "schema/v5.sql" | "schema/migrate_v4_to_v5.sql"
        ) {
            continue;
        }
        connection.prepare(sql)?;
    }
    Ok(())
}

type SchemaObject = (String, String, String, Option<String>);

fn schema_objects(connection: &Connection) -> Result<Vec<SchemaObject>> {
    Ok(connection
        .prepare(statements::schema::SCHEMA_OBJECTS)?
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<rusqlite::Result<_>>()?)
}

fn expected_schema_objects(version: i64) -> Result<Vec<SchemaObject>> {
    let expected = Connection::open_in_memory()?;
    expected.execute_batch(match version {
        PREVIOUS_SCHEMA_VERSION => statements::schema::V4,
        SCHEMA_VERSION => statements::schema::V5,
        _ => return Err(StoreError::WrongStoreSchema),
    })?;
    schema_objects(&expected)
}

fn migrate_v4_to_v5(connection: &mut Connection) -> Result<()> {
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute_batch(statements::schema::MIGRATE_V4_TO_V5)?;
    transaction.commit()?;
    verify_schema(connection, SCHEMA_VERSION)
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum OpenMode {
    Create,
    Connect,
}

fn absolute(path: &Path) -> Result<PathBuf> {
    Ok(if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    })
}

pub(crate) fn appended(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(suffix);
    value.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "test-instrumentation")]
    #[test]
    fn sql_history_is_opt_in_and_reset_preserves_explicit_contract() {
        assert!(SQL_TRACE.with(|trace| trace.borrow().is_none()));
        let connection = Connection::open_in_memory().unwrap();
        connection.trace_v2(
            rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
            Some(trace_sql),
        );
        for value in 0..1000_i64 {
            assert_eq!(
                connection
                    .query_row("SELECT ?1", [value], |row| row.get::<_, i64>(0))
                    .unwrap(),
                value
            );
        }
        assert!(sql_trace().is_empty());
        assert!(SQL_TRACE.with(|trace| trace.borrow().is_none()));
        reset_sql_trace();
        connection
            .execute_batch("CREATE TABLE example(value INTEGER)")
            .unwrap();
        assert_eq!(sql_trace().len(), 1);
        assert!(sql_trace()[0].contains("CREATE TABLE example"));
        connection
            .execute("INSERT INTO example VALUES (?1)", [7])
            .unwrap();
        assert_eq!(sql_trace().len(), 2); // Reading history does not consume it.
        reset_sql_trace();
        assert!(sql_trace().is_empty());
        assert_eq!(
            connection
                .query_row("SELECT value FROM example", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            7
        );
        assert_eq!(sql_trace().len(), 1);
        SQL_TRACE.with(|trace| {
            trace.borrow_mut().take();
        });
        connection
            .execute("INSERT INTO example VALUES (?1)", [8])
            .unwrap();
        assert!(SQL_TRACE.with(|trace| trace.borrow().is_none()));
    }

    #[test]
    fn every_owned_connection_uses_the_frozen_runtime_pragmas() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-v4-pragmas-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("store.sqlite");
        let store = StoreDb::create(&path).unwrap();
        assert_pragmas(&store);
        assert_eq!(store_files(&root), vec!["store.sqlite"]);
        assert!(matches!(
            StoreDb::connect(&path),
            Err(StoreError::StoreBusy)
        ));
        assert_eq!(store_files(&root), vec!["store.sqlite"]);
        drop(store);
        let reopened = StoreDb::connect(&path).unwrap();
        assert_pragmas(&reopened);
        assert_eq!(store_files(&root), vec!["store.sqlite"]);
        drop(reopened);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn wal_store_is_rejected_without_mutation() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-v4-wal-preflight-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("store.sqlite");
        let connection = Connection::open(&path).unwrap();
        connection
            .pragma_update(None, "page_size", SQLITE_PAGE_SIZE_BYTES)
            .unwrap();
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .unwrap();
        connection.execute_batch(statements::schema::V4).unwrap();
        drop(connection);

        let before_bytes = std::fs::read(&path).unwrap();
        let before_files = store_files(&root);
        assert!(matches!(
            StoreDb::connect(&path),
            Err(StoreError::WrongStoreSchema)
        ));
        assert_eq!(std::fs::read(&path).unwrap(), before_bytes);
        assert_eq!(store_files(&root), before_files);

        let connection = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .unwrap();
        let journal: String = connection
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        assert_eq!(journal, "wal");
        drop(connection);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn assert_pragmas(store: &StoreDb) {
        let connection = store.writer().unwrap();
        let foreign_keys: i64 = connection
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap();
        let synchronous: i64 = connection
            .pragma_query_value(None, "synchronous", |row| row.get(0))
            .unwrap();
        let temp_store: i64 = connection
            .pragma_query_value(None, "temp_store", |row| row.get(0))
            .unwrap();
        let cache_size: i64 = connection
            .pragma_query_value(None, "cache_size", |row| row.get(0))
            .unwrap();
        let cache_spill: i64 = connection
            .pragma_query_value(None, "cache_spill", |row| row.get(0))
            .unwrap();
        let mmap_size: i64 = connection
            .pragma_query_value(None, "mmap_size", |row| row.get(0))
            .unwrap();
        let threads: i64 = connection
            .pragma_query_value(None, "threads", |row| row.get(0))
            .unwrap();
        let journal: String = connection
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .unwrap();
        let locking: String = connection
            .pragma_query_value(None, "locking_mode", |row| row.get(0))
            .unwrap();
        assert_eq!(foreign_keys, 1);
        assert_eq!(synchronous, 0);
        assert_eq!(temp_store, 2);
        assert_eq!(cache_size, -SQLITE_PAGE_CACHE_KIB);
        assert_eq!(cache_spill, 0);
        assert_eq!(mmap_size, 0);
        assert_eq!(threads, 0);
        assert_eq!(journal, "memory");
        assert_eq!(locking, "exclusive");
    }

    fn store_files(root: &Path) -> Vec<String> {
        let mut files = std::fs::read_dir(root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        files.sort();
        files
    }
}

#[cfg(feature = "test-instrumentation")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VerificationStoreFault {
    LaterAdmissionBatch,
    FinalPublication,
}
#[cfg(feature = "test-instrumentation")]
#[derive(Clone, Debug)]
pub struct VerificationStoreFaultReceipt {
    pub branch: BranchId,
    pub fault: VerificationStoreFault,
    pub hit_count: u64,
    pub committed_early_transactions: u64,
    pub candidate_spill_count: u64,
    pub active: bool,
}
#[cfg(feature = "test-instrumentation")]
thread_local! {static VERIFICATION_STORE_FAULT:std::cell::RefCell<Option<VerificationStoreFaultReceipt>>=const{std::cell::RefCell::new(None)};}
#[cfg(feature = "test-instrumentation")]
pub fn arm_verification_store_fault(branch: BranchId, fault: VerificationStoreFault) -> Result<()> {
    VERIFICATION_STORE_FAULT.with(|state| {
        let mut state = state.borrow_mut();
        if state.is_some() {
            return Err(StoreError::StoreBusy);
        }
        *state = Some(VerificationStoreFaultReceipt {
            branch,
            fault,
            hit_count: 0,
            committed_early_transactions: 0,
            candidate_spill_count: 0,
            active: false,
        });
        Ok(())
    })
}
#[cfg(feature = "test-instrumentation")]
pub fn take_verification_store_fault_receipt() -> Option<VerificationStoreFaultReceipt> {
    VERIFICATION_STORE_FAULT.with(|s| s.borrow_mut().take())
}
#[cfg(feature = "test-instrumentation")]
pub(crate) fn verification_candidate(branch: BranchId, spills: u64) {
    VERIFICATION_STORE_FAULT.with(|s| {
        if let Some(r) = s.borrow_mut().as_mut() {
            if r.branch == branch {
                r.active = true;
                r.candidate_spill_count = spills;
            }
        }
    });
}
#[cfg(feature = "test-instrumentation")]
pub(crate) fn verification_early_committed() {
    VERIFICATION_STORE_FAULT.with(|s| {
        if let Some(r) = s.borrow_mut().as_mut() {
            if r.active && r.hit_count == 0 {
                r.committed_early_transactions += 1;
            }
        }
    });
}
#[cfg(feature = "test-instrumentation")]
pub(crate) fn verification_store_checkpoint(fault: VerificationStoreFault) -> Result<()> {
    VERIFICATION_STORE_FAULT.with(|s| {
        if let Some(r) = s.borrow_mut().as_mut() {
            if r.active
                && r.hit_count == 0
                && r.fault == fault
                && (fault != VerificationStoreFault::LaterAdmissionBatch
                    || r.committed_early_transactions == 1)
            {
                r.hit_count = 1;
                return Err(StoreError::Integrity(
                    "injected qualified Workspace transaction failure",
                ));
            }
        }
        Ok(())
    })
}

#[cfg(all(test, feature = "test-instrumentation"))]
#[test]
fn verification_store_fault_boundary_is_one_shot() {
    let branch = BranchId::new();
    arm_verification_store_fault(branch, VerificationStoreFault::LaterAdmissionBatch).unwrap();
    verification_candidate(BranchId::new(), 1);
    assert!(verification_store_checkpoint(VerificationStoreFault::LaterAdmissionBatch).is_ok());
    verification_candidate(branch, 1);
    assert!(verification_store_checkpoint(VerificationStoreFault::LaterAdmissionBatch).is_ok());
    verification_early_committed();
    assert!(verification_store_checkpoint(VerificationStoreFault::LaterAdmissionBatch).is_err());
    assert!(verification_store_checkpoint(VerificationStoreFault::LaterAdmissionBatch).is_ok());
    let receipt = take_verification_store_fault_receipt().unwrap();
    assert_eq!(
        (
            receipt.hit_count,
            receipt.committed_early_transactions,
            receipt.candidate_spill_count
        ),
        (1, 1, 1)
    );
    assert!(take_verification_store_fault_receipt().is_none());
}
