use crate::statements;
use crate::{BranchId, Result, StoreError};
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use std::collections::{BTreeSet, VecDeque};
use std::fs::OpenOptions;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex, MutexGuard};

pub(crate) const WORKSPACE_PUBLICATIONS_SCHEMA: &str =
    include_str!("../sql/schema/workspace_publications.sql");

pub const APPLICATION_ID: i64 = 0x4c46_534c;
pub const SCHEMA_VERSION: i64 = 10;
pub const LEGACY_SCHEMA_VERSION: i64 = 6;
// Creation policy is independent of supported existing schema-6 layouts.
pub const NEW_STORE_PAGE_SIZE_BYTES: i64 = 4096;
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
    write_failed: std::sync::atomic::AtomicBool,
    format_version: i64,
    physical: crate::telemetry::PhysicalStorageCounters,
    connection: Mutex<Connection>,
    gate: Arc<TicketGate>,
    leases: Mutex<BTreeSet<BranchId>>,
    idle_small_candidates: Mutex<Option<crate::objects::small_candidates::Candidates>>,
    metadata_index: Mutex<Option<crate::objects::metadata::ValueIndex>>,
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
    occupied: bool,
    waiters: VecDeque<mpsc::SyncSender<()>>,
}

#[derive(Default)]
struct TicketGate {
    state: Mutex<TicketState>,
}

pub(crate) struct OperationPermit {
    gate: Arc<TicketGate>,
}

impl Drop for OperationPermit {
    fn drop(&mut self) {
        let mut state = match self.gate.state.lock() {
            Ok(state) => state,
            Err(poisoned) => {
                // Disconnect every queued receiver on failure; no waiter can be
                // left asleep behind a gate that future entrants will reject.
                poisoned.into_inner().waiters.clear();
                return;
            }
        };
        while let Some(successor) = state.waiters.pop_front() {
            // A one-slot channel is empty until this single grant. A cancelled
            // receiver is skipped, without waking or rescanning other waiters.
            if successor.send(()).is_ok() {
                return;
            }
        }
        state.occupied = false;
    }
}

impl TicketGate {
    fn enter(self: &Arc<Self>) -> Result<OperationPermit> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| StoreError::Integrity("operation gate"))?;
        if !state.occupied {
            state.occupied = true;
            return Ok(OperationPermit { gate: self.clone() });
        }
        let (grant, ready) = mpsc::sync_channel(1);
        state.waiters.push_back(grant);
        drop(state);
        ready
            .recv()
            .map_err(|_| StoreError::Integrity("operation gate"))?;
        Ok(OperationPermit { gate: self.clone() })
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
    pub(crate) fn native_format(&self) -> bool {
        self.0.format_version >= 7
    }

    pub(crate) fn small_content_format(&self) -> bool {
        self.0.format_version >= 8
    }

    pub(crate) fn small_chain_format(&self) -> bool {
        self.0.format_version >= 9
    }

    pub(crate) fn compact_framing(&self) -> bool {
        self.0.format_version >= 10
    }
    pub(crate) fn compact_namespace(&self) -> bool {
        self.0.format_version >= 10
    }
    pub(crate) fn metadata_index(
        &self,
    ) -> Result<MutexGuard<'_, Option<crate::objects::metadata::ValueIndex>>> {
        self.0
            .metadata_index
            .lock()
            .map_err(|_| StoreError::Integrity("metadata index ownership"))
    }
    pub(crate) fn clear_metadata_index(&self) -> Result<()> {
        *self.metadata_index()? = None;
        Ok(())
    }

    /// Burn a range before canonical construction/admission. A failed candidate
    /// cannot roll this reservation back or reuse an identity exposed by it.
    pub(crate) fn reserve_inode_serials(
        &self,
        scope: layerfs_content::ObjectId,
        count: u64,
    ) -> Result<std::ops::Range<u64>> {
        use rusqlite::OptionalExtension;
        if !self.compact_namespace() || count == 0 || count > i64::MAX as u64 {
            return Err(StoreError::InvalidInput("compact inode reservation"));
        }
        let _permit = self.enter_operation()?;
        let mut connection = self.writer()?;
        if !connection.is_autocommit() {
            return Err(StoreError::Integrity(
                "inode reservation must precede admission",
            ));
        }
        let result = (|| {
            // Allocation is durable before its serials escape. Keep the normal
            // publication policy unchanged after this isolated transaction.
            connection.pragma_update(None, "journal_mode", "DELETE")?;
            connection.pragma_update(None, "synchronous", "FULL")?;
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let end = transaction
                .query_row(
                    statements::schema::RESERVE_INODE_SERIALS,
                    rusqlite::params![scope.as_bytes().as_slice(), count as i64],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
                .ok_or(StoreError::InvalidInput("inode serial space exhausted"))?;
            let end = u64::try_from(end)
                .map_err(|_| StoreError::Integrity("inode allocator highwater"))?;
            let start = end
                .checked_sub(count)
                .and_then(|value| value.checked_add(1))
                .ok_or(StoreError::Integrity("inode allocator range"))?;
            transaction.commit()?;
            Ok(start..end + 1)
        })();
        let restored = connection
            .pragma_update(None, "journal_mode", "MEMORY")
            .and_then(|_| connection.pragma_update(None, "synchronous", "OFF"));
        if restored.is_err() {
            self.quarantine_writes();
            return Err(StoreError::Integrity(
                "inode allocator connection restoration",
            ));
        }
        let cleanup = (|| {
            if !connection.is_autocommit() {
                return Err(StoreError::Integrity(
                    "inode allocator transaction remains active",
                ));
            }
            // EXCLUSIVE locking leaves a zeroed rollback journal after commit.
            // Switching to MEMORY closes SQLite's disk journal handle without
            // releasing the exclusive database lock. Remove only that inactive
            // journal; a nonzero header is retained and quarantines further writes.
            let journal = appended(self.path(), "-journal");
            let file = match std::fs::File::open(&journal) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(error) => return Err(error.into()),
            };
            let mut header = Vec::with_capacity(8);
            file.take(8).read_to_end(&mut header)?;
            if header.iter().any(|byte| *byte != 0) {
                return Err(StoreError::Integrity("inode allocator journal remains hot"));
            }
            std::fs::remove_file(journal)?;
            Ok(())
        })();
        if cleanup.is_err() {
            self.quarantine_writes();
        }
        cleanup?;
        result
    }

    pub(crate) fn same_instance(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

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
        let format_version = if mode == OpenMode::Connect {
            preflight_connect(&path)?
        } else {
            SCHEMA_VERSION
        };
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let mut connection = Connection::open_with_flags(&path, flags)?;
        if mode == OpenMode::Create {
            connection.pragma_update(None, "page_size", NEW_STORE_PAGE_SIZE_BYTES)?;
        }
        configure_connection(&connection)?;
        if mode == OpenMode::Create {
            connection.execute_batch(statements::schema::V10)?;
        }
        acquire_exclusive_lock(&mut connection)?;
        verify_schema(&connection, format_version)?;
        prepare_manifest(&connection)?;
        #[cfg(feature = "test-instrumentation")]
        {
            connection.trace_v2(
                rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
                Some(trace_sql),
            );
        }
        let store = Self(Arc::new(StoreInner {
            write_failed: std::sync::atomic::AtomicBool::new(false),
            format_version,
            physical: Default::default(),
            connection: Mutex::new(connection),
            gate: Arc::new(TicketGate::default()),
            leases: Mutex::new(BTreeSet::new()),
            idle_small_candidates: Mutex::new(None),
            metadata_index: Mutex::new(None),
            path,
        }));
        if let Some(created) = &mut created {
            created.remove = false;
        }
        Ok(store)
    }

    pub(crate) fn note_physical(&self, receipt: crate::PhysicalStorageReceipt) {
        self.0.physical.note(receipt);
    }

    // Called under the existing admission writer permit. Poisoned optional hints
    // are discarded; they are never an authentication or membership authority.
    pub(crate) fn take_small_candidates(
        &self,
    ) -> Option<crate::objects::small_candidates::Candidates> {
        match self.0.idle_small_candidates.lock() {
            Ok(mut idle) => idle.take(),
            Err(poisoned) => {
                drop(poisoned.into_inner().take());
                None
            }
        }
    }

    pub(crate) fn return_small_candidates(
        &self,
        candidates: crate::objects::small_candidates::Candidates,
    ) {
        if let Ok(mut idle) = self.0.idle_small_candidates.lock() {
            debug_assert!(idle.is_none());
            if idle.is_none() {
                *idle = Some(candidates);
            }
        }
    }

    pub(crate) fn physical_storage_receipt(&self) -> crate::PhysicalStorageReceipt {
        self.0.physical.snapshot()
    }

    pub fn path(&self) -> &Path {
        &self.0.path
    }

    #[cfg(test)]
    pub(crate) fn operation_waiters(&self) -> usize {
        self.0.gate.state.lock().unwrap().waiters.len()
    }

    pub fn enter_operation(&self) -> Result<OperationPermit> {
        self.ensure_writable()?;
        let permit = self.0.gate.enter()?;
        self.ensure_writable()?;
        Ok(permit)
    }

    pub(crate) fn quarantine_writes(&self) {
        self.0
            .write_failed
            .store(true, std::sync::atomic::Ordering::Release);
    }

    fn ensure_writable(&self) -> Result<()> {
        if self
            .0
            .write_failed
            .load(std::sync::atomic::Ordering::Acquire)
        {
            Err(StoreError::Integrity(
                "Store writes quarantined after failed admission cleanup",
            ))
        } else {
            Ok(())
        }
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
        self.ensure_writable()?;
        self.reader()
    }

    pub fn reader(&self) -> Result<MutexGuard<'_, Connection>> {
        self.0
            .connection
            .lock()
            .map_err(|_| StoreError::Integrity("Store connection"))
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
    if !matches!(version, LEGACY_SCHEMA_VERSION | 7 | 8 | 9 | SCHEMA_VERSION) {
        return Err(StoreError::WrongStoreSchema);
    }
    // Reject unsupported layouts before writer configuration. Both callers
    // validate all foreign keys again under their exclusive lock, before any
    // publication or migration; scanning them here duplicates whole-Store work.
    verify_schema_layout(&connection, version)?;
    // Research binaries wrote native packs under6 without a writer fence. They
    // are isolated evidence, not supported legacy Stores. Inspect headers only;
    // do not decompress, migrate or rewrite them during connect.
    if version == LEGACY_SCHEMA_VERSION
        && connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM object_packs WHERE substr(data,9,4) != x'01000000')",
            [],
            |row| row.get::<_, bool>(0),
        )?
    {
        return Err(StoreError::WrongStoreSchema);
    }
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
    verify_schema_layout(connection, version)?;
    let mut foreign_keys = connection.prepare(statements::schema::FOREIGN_KEY_CHECK)?;
    if foreign_keys.exists([])? {
        return Err(StoreError::Integrity("foreign key check"));
    }
    Ok(())
}

fn verify_schema_layout(connection: &Connection, version: i64) -> Result<()> {
    let application_id: i64 =
        connection.pragma_query_value(None, "application_id", |row| row.get(0))?;
    let user_version: i64 =
        connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    let page_size: i64 = connection.pragma_query_value(None, "page_size", |row| row.get(0))?;
    if application_id != APPLICATION_ID
        || user_version != version
        || !matches!(page_size, 4096 | 65536)
    {
        return Err(StoreError::WrongStoreSchema);
    }
    // Receipt metadata is an explicit optional extension to every supported
    // canonical format. Validate its exact SQL too; arbitrary extra schema still
    // fails before writer configuration, and opening legacy Stores adds nothing.
    let mut actual = schema_objects(connection)?;
    let mut expected = expected_schema_objects(version)?;
    if actual
        .iter()
        .any(|(_, name, _, _)| name == "workspace_publications")
    {
        let extension = Connection::open_in_memory()?;
        extension.execute_batch(WORKSPACE_PUBLICATIONS_SCHEMA)?;
        expected.extend(schema_objects(&extension)?);
        actual.sort();
        expected.sort();
    }
    if actual != expected {
        return Err(StoreError::WrongStoreSchema);
    }
    Ok(())
}

fn prepare_manifest(connection: &Connection) -> Result<()> {
    let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    for (name, sql) in statements::ALL {
        if *name == "schema/reserve_inode_serials.sql" && version < 10 {
            continue;
        }
        if matches!(
            *name,
            "schema/v4.sql"
                | "schema/v5.sql"
                | "schema/v6.sql"
                | "schema/v7.sql"
                | "schema/v8.sql"
                | "schema/v9.sql"
                | "schema/v10.sql"
                | "schema/workspace_publications.sql"
                | "schema/migrate_to_v9.sql"
                | "schema/migrate_v7_to_v8.sql"
                | "schema/migrate_v4_to_v5.sql"
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
        LEGACY_SCHEMA_VERSION => statements::schema::V6,
        7 => statements::schema::V7,
        8 => statements::schema::V8,
        9 => statements::schema::V9,
        SCHEMA_VERSION => statements::schema::V10,
        _ => return Err(StoreError::WrongStoreSchema),
    })?;
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
            .pragma_update(None, "page_size", NEW_STORE_PAGE_SIZE_BYTES)
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

    #[test]
    fn preflight_defers_foreign_key_scan_but_connect_and_upgrade_reject_without_mutation() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-fk-preflight-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        for (version, sql) in [
            (6, statements::schema::V6),
            (7, statements::schema::V7),
            (8, statements::schema::V8),
            (9, statements::schema::V9),
            (10, statements::schema::V10),
        ] {
            let path = root.join(format!("store-{version}.sqlite"));
            let connection = Connection::open(&path).unwrap();
            connection
                .pragma_update(None, "page_size", NEW_STORE_PAGE_SIZE_BYTES)
                .unwrap();
            connection.execute_batch(sql).unwrap();
            connection
                .pragma_update(None, "foreign_keys", false)
                .unwrap();
            connection.execute(
                "INSERT INTO objects(object_id,canonical_length,pack_id,group_number,record_number) VALUES(zeroblob(32),1,1,0,0)",
                [],
            ).unwrap();
            drop(connection);
            let before = std::fs::read(&path).unwrap();
            let files = store_files(&root);
            assert_eq!(preflight_connect(&path).unwrap(), version);
            assert!(matches!(
                StoreDb::connect(&path),
                Err(StoreError::Integrity("foreign key check"))
            ));
            assert_eq!(std::fs::read(&path).unwrap(), before);
            assert_eq!(store_files(&root), files);
            if (7..=9).contains(&version) {
                assert!(matches!(
                    upgrade_format(&path),
                    Err(StoreError::Integrity("foreign key check"))
                ));
                assert_eq!(std::fs::read(&path).unwrap(), before);
                assert_eq!(store_files(&root), files);
            }
        }
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
#[doc(hidden)]
pub fn verification_candidate(branch: BranchId, spills: u64) {
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

#[cfg(test)]
mod compatibility;

/// Offline promotion never acquires a normal MEMORY/OFF connection.
pub(crate) fn upgrade_format(path: &Path) -> Result<()> {
    let version = preflight_connect(path)?;
    if !matches!(version, 7..=9) {
        return Err(StoreError::InvalidInput(
            "format upgrade requires schema 7, 8 or 9",
        ));
    }
    let mut connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.busy_timeout(std::time::Duration::ZERO)?;
    connection.pragma_update(None, "locking_mode", "EXCLUSIVE")?;
    connection.pragma_update(None, "journal_mode", "DELETE")?;
    connection.pragma_update(None, "synchronous", "FULL")?;
    let journal: String = connection.pragma_query_value(None, "journal_mode", |r| r.get(0))?;
    let synchronous: i64 = connection.pragma_query_value(None, "synchronous", |r| r.get(0))?;
    if journal != "delete" || synchronous != 2 {
        return Err(StoreError::WrongStoreSchema);
    }
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Exclusive)?;
    let current: i64 = transaction.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if !matches!(current, 7..=9) {
        return Err(StoreError::WrongStoreSchema);
    }
    verify_schema(&transaction, current)?;
    if current != 9 {
        transaction.execute_batch(statements::schema::MIGRATE_TO_V9)?;
    }
    match transaction.commit() {
        Ok(()) => Ok(()),
        Err(error) => {
            let promoted = connection.is_autocommit()
                && connection
                    .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
                    .ok()
                    == Some(9);
            if promoted {
                Err(StoreError::Io(std::io::Error::other(format!(
                    "schema 9 promotion occurred; commit reported: {error}"
                ))))
            } else {
                Err(error.into())
            }
        }
    }
}
