use crate::statements;
use crate::{BranchId, Result, StoreError};
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

pub const APPLICATION_ID: i64 = 0x4c46_534c;
pub const SCHEMA_VERSION: i64 = 4;
pub const SQLITE_PAGE_SIZE_BYTES: i64 = 64 * 1024;
pub const SQLITE_PAGE_CACHE_KIB: i64 = 32 * 1024;

#[cfg(feature = "test-instrumentation")]
thread_local! {
    static SQL_TRACE: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(debug_assertions)]
thread_local! {
    static TRANSACTION_FAILURE_AT: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
}

#[cfg(feature = "test-instrumentation")]
fn trace_sql(event: rusqlite::trace::TraceEvent<'_>) {
    if let rusqlite::trace::TraceEvent::Stmt(_, sql) = event {
        SQL_TRACE.with(|trace| trace.borrow_mut().push(sql.to_owned()));
    }
}

#[cfg(feature = "test-instrumentation")]
pub fn reset_sql_trace() {
    SQL_TRACE.with(|trace| trace.borrow_mut().clear());
}

#[cfg(feature = "test-instrumentation")]
pub fn sql_trace() -> Vec<String> {
    SQL_TRACE.with(|trace| trace.borrow().clone())
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
        if mode == OpenMode::Connect {
            preflight_connect(&path)?;
        }
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let mut connection = Connection::open_with_flags(&path, flags)?;
        if mode == OpenMode::Create {
            connection.pragma_update(None, "page_size", SQLITE_PAGE_SIZE_BYTES)?;
        }
        configure_connection(&connection)?;
        if mode == OpenMode::Create {
            connection.execute_batch(statements::schema::V4)?;
        }
        verify_schema(&connection)?;
        prepare_manifest(&connection)?;
        acquire_exclusive_lock(&mut connection)?;
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

fn preflight_connect(path: &Path) -> Result<()> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let journal: String = connection.pragma_query_value(None, "journal_mode", |row| row.get(0))?;
    if journal.eq_ignore_ascii_case("wal") {
        return Err(StoreError::WrongStoreSchema);
    }
    verify_schema(&connection)
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

fn verify_schema(connection: &Connection) -> Result<()> {
    let application_id: i64 =
        connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
    let user_version: i64 =
        connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let page_size: i64 = connection.pragma_query_value(None, "page_size", |row| row.get(0))?;
    if application_id != APPLICATION_ID
        || user_version != SCHEMA_VERSION
        || page_size != SQLITE_PAGE_SIZE_BYTES
    {
        return Err(StoreError::WrongStoreSchema);
    }
    if schema_objects(connection)? != expected_schema_objects()? {
        return Err(StoreError::WrongStoreSchema);
    }
    let mut foreign_keys = connection.prepare(statements::schema::FOREIGN_KEY_CHECK)?;
    if foreign_keys.exists([])? {
        return Err(StoreError::Integrity("foreign key check"));
    }
    Ok(())
}

fn prepare_manifest(connection: &Connection) -> Result<()> {
    for (_, sql) in statements::ALL.iter().skip(1) {
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

fn expected_schema_objects() -> Result<Vec<SchemaObject>> {
    let expected = Connection::open_in_memory()?;
    expected.execute_batch(statements::schema::V4)?;
    schema_objects(&expected)
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
