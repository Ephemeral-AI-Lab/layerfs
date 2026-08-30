use crate::{InventoryEntry, InventoryPage, Result, StorageError, StoreId, StoreStorageSnapshot};
use rusqlite::{Connection, OpenFlags};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

pub const SCHEMA_VERSION: i64 = 3;
pub const LAYERSTACK_APPLICATION_ID: i64 = 0x4c46_534c;
pub const BRANCH_APPLICATION_ID: i64 = 0x4c46_5342;
pub const WAL_AUTOCHECKPOINT_PAGES: i64 = 1_000;
pub const SQLITE_PAGE_CACHE_KIB: i64 = 8 * 1_024;

#[cfg(feature = "test-instrumentation")]
thread_local! {
    static SQL_TRACE: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoreRole {
    LayerStack,
    Branch,
}

impl StoreRole {
    const fn application_id(self) -> i64 {
        match self {
            Self::LayerStack => LAYERSTACK_APPLICATION_ID,
            Self::Branch => BRANCH_APPLICATION_ID,
        }
    }

    const fn schema(self) -> &'static str {
        match self {
            Self::LayerStack => LAYERSTACK_SCHEMA,
            Self::Branch => BRANCH_SCHEMA,
        }
    }

    const fn expected_tables(self) -> &'static [(&'static str, usize)] {
        match self {
            Self::LayerStack => &[
                ("branches", 8),
                ("commits", 4),
                ("layer_stacks", 3),
                ("layers", 6),
                ("objects", 2),
                ("store", 2),
            ],
            Self::Branch => &[
                ("branch_scopes", 4),
                ("branches", 8),
                ("commits", 4),
                ("complete_roots", 1),
                ("layer_stack_scopes", 3),
                ("layer_stacks", 2),
                ("layers", 6),
                ("objects", 2),
                ("store", 3),
            ],
        }
    }
}

#[derive(Clone)]
pub struct StoreDb(Arc<StoreInner>);

struct StoreInner {
    connection: Mutex<Connection>,
    gate: TicketGate,
    path: PathBuf,
    _owner: OwnerFile,
    role: StoreRole,
    store_id: StoreId,
    parent_store_id: Option<StoreId>,
}

struct OwnerFile {
    path: PathBuf,
    _file: File,
}

impl Drop for OwnerFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

struct CreatedStoreFile {
    path: PathBuf,
    remove: bool,
}

impl Drop for CreatedStoreFile {
    fn drop(&mut self) {
        if self.remove {
            let _ = std::fs::remove_file(&self.path);
            let _ = std::fs::remove_file(appended(&self.path, "-wal"));
            let _ = std::fs::remove_file(appended(&self.path, "-shm"));
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

pub struct OperationPermit<'a> {
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
            .map_err(|_| StorageError::Integrity("operation gate"))?;
        let ticket = state.next;
        state.next += 1;
        while state.serving != ticket {
            state = self
                .ready
                .wait(state)
                .map_err(|_| StorageError::Integrity("operation gate"))?;
        }
        Ok(OperationPermit { gate: self })
    }
}

impl StoreDb {
    pub fn create(
        path: impl AsRef<Path>,
        role: StoreRole,
        parent_store_id: Option<StoreId>,
    ) -> Result<Self> {
        if (role == StoreRole::Branch) != parent_store_id.is_some() {
            return Err(StorageError::InvalidInput("Store parent"));
        }
        Self::open(path.as_ref(), role, parent_store_id, OpenMode::Create)
    }

    pub fn connect(
        path: impl AsRef<Path>,
        role: StoreRole,
        expected_parent_store_id: Option<StoreId>,
    ) -> Result<Self> {
        if (role == StoreRole::Branch) != expected_parent_store_id.is_some() {
            return Err(StorageError::InvalidInput("Store parent"));
        }
        Self::open(
            path.as_ref(),
            role,
            expected_parent_store_id,
            OpenMode::Connect,
        )
    }

    fn open(
        path: &Path,
        role: StoreRole,
        expected_parent_store_id: Option<StoreId>,
        mode: OpenMode,
    ) -> Result<Self> {
        let path = absolute(path)?;
        if mode == OpenMode::Create && path.exists() {
            return Err(StorageError::StoreAlreadyExists);
        }
        if mode == OpenMode::Connect && !path.is_file() {
            return Err(StorageError::StoreMissing);
        }
        if mode == OpenMode::Create {
            std::fs::create_dir_all(
                path.parent()
                    .ok_or(StorageError::InvalidInput("Store location"))?,
            )?;
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|error| {
                    if error.kind() == std::io::ErrorKind::AlreadyExists {
                        StorageError::StoreAlreadyExists
                    } else {
                        StorageError::Io(error)
                    }
                })?;
        }
        let mut created = (mode == OpenMode::Create).then(|| CreatedStoreFile {
            path: path.clone(),
            remove: true,
        });
        let owner_path = appended(&path, ".owner");
        let owner = OwnerFile {
            _file: acquire_owner(
                &owner_path,
                path.parent()
                    .ok_or(StorageError::InvalidInput("Store location"))?,
            )?,
            path: owner_path,
        };
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let connection = Connection::open_with_flags(&path, flags)?;
        #[cfg(feature = "test-instrumentation")]
        connection.trace_v2(
            rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
            Some(trace_sql),
        );
        configure(&connection)?;
        let (store_id, parent_store_id) = if mode == OpenMode::Create {
            install_schema(&connection, role, expected_parent_store_id)?
        } else {
            verify_schema(&connection, role, expected_parent_store_id)?
        };
        let store = Self(Arc::new(StoreInner {
            connection: Mutex::new(connection),
            gate: TicketGate::default(),
            path,
            _owner: owner,
            role,
            store_id,
            parent_store_id,
        }));
        if let Some(created) = &mut created {
            created.remove = false;
        }
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.0.path
    }

    pub fn role(&self) -> StoreRole {
        self.0.role
    }

    pub fn store_id(&self) -> StoreId {
        self.0.store_id
    }

    pub fn parent_store_id(&self) -> Option<StoreId> {
        self.0.parent_store_id
    }

    pub fn enter_operation(&self) -> Result<OperationPermit<'_>> {
        self.0.gate.enter()
    }

    pub fn inventory_page(
        &self,
        after: Option<layerfs_content::ObjectId>,
        limit: u16,
    ) -> Result<InventoryPage> {
        if limit == 0 || limit > 512 {
            return Err(StorageError::InvalidInput("inventory page"));
        }
        let connection = self.connection()?;
        let mut statement = connection.prepare_cached(
            "SELECT object_id,length(bytes) FROM objects
             WHERE object_id>?1 ORDER BY object_id LIMIT ?2",
        )?;
        let mut entries = statement
            .query_map(
                rusqlite::params![
                    after.map(|id| id.to_bytes().to_vec()).unwrap_or_default(),
                    i64::from(limit) + 1
                ],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?)),
            )?
            .map(|row| {
                let (id, encoded_length) = row?;
                Ok(InventoryEntry {
                    object_id: layerfs_content::ObjectId::from_bytes(&id)?,
                    encoded_length: encoded_length
                        .try_into()
                        .map_err(|_| StorageError::Integrity("object length"))?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let has_more = entries.len() > usize::from(limit);
        entries.truncate(usize::from(limit));
        let continuation = has_more
            .then(|| entries.last().map(|entry| entry.object_id))
            .flatten();
        Ok(InventoryPage {
            entries,
            continuation,
        })
    }

    pub fn storage_snapshot(&self) -> Result<StoreStorageSnapshot> {
        fn len(path: &Path) -> Result<u64> {
            match std::fs::metadata(path) {
                Ok(metadata) => Ok(metadata.len()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
                Err(error) => Err(StorageError::Io(error)),
            }
        }
        Ok(StoreStorageSnapshot {
            database_bytes: len(self.path())?,
            wal_bytes: len(&appended(self.path(), "-wal"))?,
            shm_bytes: len(&appended(self.path(), "-shm"))?,
        })
    }

    pub fn stable_barrier(&self) -> Result<crate::DurabilityReceipt> {
        fn elapsed_ns(started: std::time::Instant) -> u64 {
            started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
        }

        let stable_started = std::time::Instant::now();
        let checkpoint_started = std::time::Instant::now();
        let checkpoint =
            self.connection()?
                .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                })?;
        let checkpoint_ns = elapsed_ns(checkpoint_started);
        if checkpoint != (0, 0, 0) {
            return Err(StorageError::Integrity("incomplete WAL checkpoint"));
        }

        let database_started = std::time::Instant::now();
        File::open(self.path())?.sync_all()?;
        let database_fsync_ns = elapsed_ns(database_started);

        let directory_started = std::time::Instant::now();
        File::open(
            self.path()
                .parent()
                .ok_or(StorageError::InvalidInput("Store location"))?,
        )?
        .sync_all()?;
        let directory_fsync_ns = elapsed_ns(directory_started);

        let stable_ns = elapsed_ns(stable_started);
        let attributed = checkpoint_ns
            .saturating_add(database_fsync_ns)
            .saturating_add(directory_fsync_ns);
        let receipt = crate::DurabilityReceipt {
            store_id: self.store_id(),
            role: self.role(),
            stable_ns,
            checkpoint_ns,
            database_fsync_ns,
            directory_fsync_ns,
            unattributed_ns: stable_ns.saturating_sub(attributed),
        };
        receipt.validate()?;
        Ok(receipt)
    }

    pub(crate) fn connection(&self) -> Result<MutexGuard<'_, Connection>> {
        self.0
            .connection
            .lock()
            .map_err(|_| StorageError::Integrity("database lock"))
    }
}

fn configure(connection: &Connection) -> Result<()> {
    let version: String = connection.query_row("SELECT sqlite_version()", [], |row| row.get(0))?;
    if sqlite_version(&version)? < (3, 37, 0) {
        return Err(StorageError::Integrity("SQLite STRICT support"));
    }
    connection.pragma_update(None, "foreign_keys", true)?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "FULL")?;
    connection.pragma_update(None, "temp_store", "FILE")?;
    connection.pragma_update(None, "cache_size", -SQLITE_PAGE_CACHE_KIB)?;
    connection.pragma_update(None, "wal_autocheckpoint", WAL_AUTOCHECKPOINT_PAGES)?;
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(())
}

fn install_schema(
    connection: &Connection,
    role: StoreRole,
    parent_store_id: Option<StoreId>,
) -> Result<(StoreId, Option<StoreId>)> {
    let transaction = connection.unchecked_transaction()?;
    transaction.pragma_update(None, "application_id", role.application_id())?;
    transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    transaction.execute_batch(role.schema())?;
    let store_id = StoreId::random()?;
    match role {
        StoreRole::LayerStack => {
            transaction.execute(
                "INSERT INTO store(singleton,store_id) VALUES(1,?1)",
                [crate::StorageId::as_slice(&store_id)],
            )?;
        }
        StoreRole::Branch => {
            let parent = parent_store_id.ok_or(StorageError::InvalidInput("Store parent"))?;
            transaction.execute(
                "INSERT INTO store(singleton,store_id,parent_store_id) VALUES(1,?1,?2)",
                rusqlite::params![
                    crate::StorageId::as_slice(&store_id),
                    crate::StorageId::as_slice(&parent)
                ],
            )?;
        }
    }
    transaction.commit()?;
    verify_schema(connection, role, parent_store_id)
}

fn verify_schema(
    connection: &Connection,
    role: StoreRole,
    expected_parent_store_id: Option<StoreId>,
) -> Result<(StoreId, Option<StoreId>)> {
    let application_id: i64 =
        connection.query_row("PRAGMA application_id", [], |row| row.get(0))?;
    if application_id != role.application_id() {
        return Err(StorageError::WrongStoreRole);
    }
    let user_version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if user_version != SCHEMA_VERSION {
        return Err(StorageError::WrongStoreSchema);
    }
    if schema_objects(connection)? != expected_schema_objects(role)? {
        return Err(StorageError::WrongStoreSchema);
    }
    let tables = connection
        .prepare(
            "SELECT name FROM sqlite_schema
             WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let expected_names = role
        .expected_tables()
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>();
    if tables.iter().map(String::as_str).collect::<Vec<_>>() != expected_names {
        return Err(StorageError::WrongStoreSchema);
    }
    for (table, expected_columns) in role.expected_tables() {
        let columns: i64 = connection.query_row(
            "SELECT count(*) FROM pragma_table_info(?1)",
            [table],
            |row| row.get(0),
        )?;
        if columns != *expected_columns as i64 {
            return Err(StorageError::WrongStoreSchema);
        }
    }
    let foreign_key_errors: i64 = connection
        .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .unwrap_or(1);
    if foreign_key_errors != 0 {
        return Err(StorageError::Integrity("foreign key check"));
    }
    let (store_id, parent_store_id) = if role == StoreRole::LayerStack {
        let bytes: Vec<u8> =
            connection.query_row("SELECT store_id FROM store WHERE singleton=1", [], |row| {
                row.get(0)
            })?;
        (crate::StorageId::from_slice(&bytes)?, None)
    } else {
        let (store, parent): (Vec<u8>, Vec<u8>) = connection.query_row(
            "SELECT store_id,parent_store_id FROM store WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        (
            crate::StorageId::from_slice(&store)?,
            Some(crate::StorageId::from_slice(&parent)?),
        )
    };
    if expected_parent_store_id != parent_store_id {
        return Err(StorageError::WrongParent);
    }
    Ok((store_id, parent_store_id))
}

type SchemaObject = (String, String, String, Option<String>);

fn schema_objects(connection: &Connection) -> Result<Vec<SchemaObject>> {
    Ok(connection
        .prepare(
            "SELECT type,name,tbl_name,sql FROM sqlite_schema
             WHERE name NOT LIKE 'sqlite_%' ORDER BY type,name",
        )?
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?
        .collect::<std::result::Result<_, _>>()?)
}

fn expected_schema_objects(role: StoreRole) -> Result<Vec<SchemaObject>> {
    let expected = Connection::open_in_memory()?;
    expected.execute_batch(role.schema())?;
    schema_objects(&expected)
}

fn acquire_owner(path: &Path, parent: &Path) -> Result<File> {
    #[cfg(unix)]
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    for _ in 0..2 {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        match options.open(path) {
            Ok(mut file) => {
                writeln!(file, "{}", std::process::id())?;
                file.sync_all()?;
                return Ok(file);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let metadata = std::fs::symlink_metadata(path)?;
                if !metadata.file_type().is_file() {
                    return Err(StorageError::StoreBusy);
                }
                #[cfg(unix)]
                if metadata.uid() != std::fs::metadata(parent)?.uid() {
                    return Err(StorageError::StoreBusy);
                }
                let pid = std::fs::read_to_string(path)
                    .ok()
                    .and_then(|value| value.trim().parse::<u32>().ok());
                let fresh = metadata
                    .modified()
                    .ok()
                    .and_then(|modified| modified.elapsed().ok())
                    .is_some_and(|age| age < std::time::Duration::from_secs(2));
                if pid.is_some_and(process_is_live) || (pid.is_none() && fresh) {
                    return Err(StorageError::StoreBusy);
                }
                std::fs::remove_file(path)?;
            }
            Err(error) => return Err(StorageError::Io(error)),
        }
    }
    Err(StorageError::StoreBusy)
}

fn process_is_live(pid: u32) -> bool {
    std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "stat="])
        .output()
        .is_ok_and(|output| {
            output.status.success()
                && String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .chars()
                    .next()
                    .is_some_and(|state| state != 'Z')
        })
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum OpenMode {
    Create,
    Connect,
}

fn sqlite_version(value: &str) -> Result<(u32, u32, u32)> {
    let mut parts = value.split('.');
    let parse = |part: Option<&str>| {
        part.and_then(|value| value.parse().ok())
            .ok_or(StorageError::Integrity("SQLite version"))
    };
    Ok((
        parse(parts.next())?,
        parse(parts.next())?,
        parse(parts.next())?,
    ))
}

fn absolute(path: &Path) -> Result<PathBuf> {
    Ok(if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    })
}

fn appended(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(suffix);
    value.into()
}

const LAYERSTACK_SCHEMA: &str = r#"
CREATE TABLE store (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    store_id BLOB NOT NULL UNIQUE CHECK (length(store_id) = 32)
) STRICT;
CREATE TABLE objects (
    object_id BLOB PRIMARY KEY CHECK (length(object_id) = 32),
    bytes BLOB NOT NULL
) STRICT;
CREATE TABLE commits (
    commit_id BLOB PRIMARY KEY CHECK (length(commit_id) = 33),
    root_id BLOB NOT NULL CHECK (length(root_id) = 32) REFERENCES objects(object_id),
    parent_commit_id BLOB CHECK (parent_commit_id IS NULL OR length(parent_commit_id) = 33),
    base_layer_id BLOB NOT NULL CHECK (length(base_layer_id) = 33) REFERENCES layers(layer_id),
    FOREIGN KEY (parent_commit_id) REFERENCES commits(commit_id)
) STRICT;
CREATE TABLE branches (
    branch_id BLOB PRIMARY KEY CHECK (length(branch_id) = 17),
    layer_stack_id BLOB NOT NULL CHECK (length(layer_stack_id) = 17) REFERENCES layer_stacks(layer_stack_id) DEFERRABLE INITIALLY DEFERRED,
    name TEXT NOT NULL
        CHECK (length(name) BETWEEN 1 AND 63)
        CHECK (name = lower(name))
        CHECK (name NOT GLOB '*[^a-z0-9._-]*')
        CHECK (substr(name, 1, 1) GLOB '[a-z0-9]')
        CHECK (substr(name, -1, 1) GLOB '[a-z0-9]'),
    base_layer_id BLOB NOT NULL CHECK (length(base_layer_id) = 33),
    head_commit_id BLOB NOT NULL CHECK (length(head_commit_id) = 33) REFERENCES commits(commit_id),
    forked_from_layer_id BLOB CHECK (forked_from_layer_id IS NULL OR length(forked_from_layer_id) = 33),
    forked_from_branch_id BLOB CHECK (forked_from_branch_id IS NULL OR length(forked_from_branch_id) = 17),
    forked_from_commit_id BLOB CHECK (forked_from_commit_id IS NULL OR length(forked_from_commit_id) = 33) REFERENCES commits(commit_id),
    CHECK ((forked_from_layer_id IS NOT NULL AND forked_from_branch_id IS NULL AND forked_from_commit_id IS NULL)
        OR (forked_from_layer_id IS NULL AND forked_from_branch_id IS NOT NULL AND forked_from_commit_id IS NOT NULL)),
    FOREIGN KEY (layer_stack_id, base_layer_id) REFERENCES layers(layer_stack_id, layer_id),
    FOREIGN KEY (layer_stack_id, forked_from_layer_id) REFERENCES layers(layer_stack_id, layer_id),
    FOREIGN KEY (layer_stack_id, forked_from_branch_id) REFERENCES branches(layer_stack_id, branch_id)
) STRICT;
CREATE TABLE layer_stacks (
    layer_stack_id BLOB PRIMARY KEY CHECK (length(layer_stack_id) = 17),
    name TEXT NOT NULL
        CHECK (length(name) BETWEEN 1 AND 63)
        CHECK (name = lower(name))
        CHECK (name NOT GLOB '*[^a-z0-9._-]*')
        CHECK (substr(name, 1, 1) GLOB '[a-z0-9]')
        CHECK (substr(name, -1, 1) GLOB '[a-z0-9]'),
    head_layer_id BLOB NOT NULL CHECK (length(head_layer_id) = 33),
    FOREIGN KEY (layer_stack_id, head_layer_id) REFERENCES layers(layer_stack_id, layer_id) DEFERRABLE INITIALLY DEFERRED
) STRICT;
CREATE TABLE layers (
    layer_id BLOB PRIMARY KEY CHECK (length(layer_id) = 33),
    layer_stack_id BLOB NOT NULL CHECK (length(layer_stack_id) = 17) REFERENCES layer_stacks(layer_stack_id) DEFERRABLE INITIALLY DEFERRED,
    parent_layer_id BLOB CHECK (parent_layer_id IS NULL OR length(parent_layer_id) = 33),
    root_id BLOB NOT NULL CHECK (length(root_id) = 32) REFERENCES objects(object_id),
    source_branch_id BLOB CHECK (source_branch_id IS NULL OR length(source_branch_id) = 17),
    source_commit_id BLOB CHECK (source_commit_id IS NULL OR length(source_commit_id) = 33) REFERENCES commits(commit_id) DEFERRABLE INITIALLY DEFERRED,
    CHECK ((parent_layer_id IS NULL AND source_branch_id IS NULL AND source_commit_id IS NULL)
        OR (parent_layer_id IS NOT NULL AND source_branch_id IS NOT NULL AND source_commit_id IS NOT NULL)),
    FOREIGN KEY (layer_stack_id, parent_layer_id) REFERENCES layers(layer_stack_id, layer_id),
    FOREIGN KEY (layer_stack_id, source_branch_id) REFERENCES branches(layer_stack_id, branch_id)
) STRICT;
CREATE UNIQUE INDEX layer_stack_names ON layer_stacks(name);
CREATE UNIQUE INDEX layer_identity ON layers(layer_stack_id, layer_id);
CREATE UNIQUE INDEX layers_genesis ON layers(layer_stack_id) WHERE parent_layer_id IS NULL;
CREATE UNIQUE INDEX layers_child ON layers(layer_stack_id, parent_layer_id) WHERE parent_layer_id IS NOT NULL;
CREATE UNIQUE INDEX layers_source ON layers(source_branch_id, source_commit_id) WHERE source_branch_id IS NOT NULL;
CREATE INDEX layers_parent ON layers(parent_layer_id);
CREATE INDEX commits_parent ON commits(parent_commit_id);
CREATE UNIQUE INDEX branch_identity ON branches(layer_stack_id, branch_id);
CREATE UNIQUE INDEX branch_names ON branches(layer_stack_id, name);
CREATE INDEX branches_head ON branches(head_commit_id);
CREATE INDEX branches_fork ON branches(forked_from_branch_id, forked_from_commit_id);
"#;

const BRANCH_SCHEMA: &str = r#"
CREATE TABLE store (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    store_id BLOB NOT NULL UNIQUE CHECK (length(store_id) = 32),
    parent_store_id BLOB NOT NULL CHECK (length(parent_store_id) = 32)
) STRICT;
CREATE TABLE objects (
    object_id BLOB PRIMARY KEY CHECK (length(object_id) = 32),
    bytes BLOB NOT NULL
) STRICT;
CREATE TABLE commits (
    commit_id BLOB PRIMARY KEY CHECK (length(commit_id) = 33),
    root_id BLOB NOT NULL CHECK (length(root_id) = 32),
    parent_commit_id BLOB CHECK (parent_commit_id IS NULL OR length(parent_commit_id) = 33),
    base_layer_id BLOB NOT NULL CHECK (length(base_layer_id) = 33) REFERENCES layers(layer_id),
    FOREIGN KEY (parent_commit_id) REFERENCES commits(commit_id)
) STRICT;
CREATE TABLE branches (
    branch_id BLOB PRIMARY KEY CHECK (length(branch_id) = 17),
    layer_stack_id BLOB NOT NULL CHECK (length(layer_stack_id) = 17) REFERENCES layer_stacks(layer_stack_id) DEFERRABLE INITIALLY DEFERRED,
    name TEXT NOT NULL
        CHECK (length(name) BETWEEN 1 AND 63)
        CHECK (name = lower(name))
        CHECK (name NOT GLOB '*[^a-z0-9._-]*')
        CHECK (substr(name, 1, 1) GLOB '[a-z0-9]')
        CHECK (substr(name, -1, 1) GLOB '[a-z0-9]'),
    base_layer_id BLOB CHECK (base_layer_id IS NULL OR length(base_layer_id) = 33),
    head_commit_id BLOB CHECK (head_commit_id IS NULL OR length(head_commit_id) = 33) REFERENCES commits(commit_id),
    forked_from_layer_id BLOB CHECK (forked_from_layer_id IS NULL OR length(forked_from_layer_id) = 33),
    forked_from_branch_id BLOB CHECK (forked_from_branch_id IS NULL OR length(forked_from_branch_id) = 17),
    forked_from_commit_id BLOB CHECK (forked_from_commit_id IS NULL OR length(forked_from_commit_id) = 33) REFERENCES commits(commit_id),
    CHECK ((forked_from_layer_id IS NOT NULL AND forked_from_branch_id IS NULL AND forked_from_commit_id IS NULL)
        OR (forked_from_layer_id IS NULL AND forked_from_branch_id IS NOT NULL AND forked_from_commit_id IS NOT NULL)),
    CHECK (base_layer_id IS NOT NULL OR head_commit_id IS NULL),
    FOREIGN KEY (layer_stack_id, base_layer_id) REFERENCES layers(layer_stack_id, layer_id),
    FOREIGN KEY (layer_stack_id, forked_from_layer_id) REFERENCES layers(layer_stack_id, layer_id),
    FOREIGN KEY (layer_stack_id, forked_from_branch_id) REFERENCES branches(layer_stack_id, branch_id)
) STRICT;
CREATE TABLE branch_scopes (
    branch_id BLOB PRIMARY KEY CHECK (length(branch_id) = 17) REFERENCES branches(branch_id),
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('local', 'remote')),
    through_commit_id BLOB CHECK (through_commit_id IS NULL OR length(through_commit_id) = 33),
    serving_mode TEXT CHECK (serving_mode IS NULL OR serving_mode IN ('reference', 'replica')),
    CHECK ((scope_kind = 'local' AND through_commit_id IS NULL AND serving_mode IS NULL)
        OR (scope_kind = 'remote' AND through_commit_id IS NOT NULL AND serving_mode IS NOT NULL)),
    FOREIGN KEY (branch_id, through_commit_id) REFERENCES branches(branch_id, head_commit_id)
        DEFERRABLE INITIALLY DEFERRED
) STRICT;
CREATE TABLE layer_stacks (
    layer_stack_id BLOB PRIMARY KEY CHECK (length(layer_stack_id) = 17),
    name TEXT NOT NULL
        CHECK (length(name) BETWEEN 1 AND 63)
        CHECK (name = lower(name))
        CHECK (name NOT GLOB '*[^a-z0-9._-]*')
        CHECK (substr(name, 1, 1) GLOB '[a-z0-9]')
        CHECK (substr(name, -1, 1) GLOB '[a-z0-9]')
) STRICT;
CREATE TABLE layer_stack_scopes (
    layer_stack_id BLOB PRIMARY KEY CHECK (length(layer_stack_id) = 17) REFERENCES layer_stacks(layer_stack_id),
    through_layer_id BLOB NOT NULL CHECK (length(through_layer_id) = 33),
    serving_mode TEXT NOT NULL CHECK (serving_mode IN ('reference', 'replica')),
    FOREIGN KEY (layer_stack_id, through_layer_id) REFERENCES layers(layer_stack_id, layer_id)
) STRICT;
CREATE TABLE layers (
    layer_id BLOB PRIMARY KEY CHECK (length(layer_id) = 33),
    layer_stack_id BLOB NOT NULL CHECK (length(layer_stack_id) = 17) REFERENCES layer_stacks(layer_stack_id) DEFERRABLE INITIALLY DEFERRED,
    parent_layer_id BLOB CHECK (parent_layer_id IS NULL OR length(parent_layer_id) = 33),
    root_id BLOB NOT NULL CHECK (length(root_id) = 32),
    source_branch_id BLOB CHECK (source_branch_id IS NULL OR length(source_branch_id) = 17),
    source_commit_id BLOB CHECK (source_commit_id IS NULL OR length(source_commit_id) = 33),
    CHECK ((parent_layer_id IS NULL AND source_branch_id IS NULL AND source_commit_id IS NULL)
        OR (parent_layer_id IS NOT NULL AND source_branch_id IS NOT NULL AND source_commit_id IS NOT NULL)),
    FOREIGN KEY (layer_stack_id, parent_layer_id) REFERENCES layers(layer_stack_id, layer_id)
) STRICT;
CREATE TABLE complete_roots (
    root_id BLOB PRIMARY KEY CHECK (length(root_id) = 32) REFERENCES objects(object_id)
) STRICT;
CREATE UNIQUE INDEX layer_stack_names ON layer_stacks(name);
CREATE UNIQUE INDEX layer_identity ON layers(layer_stack_id, layer_id);
CREATE UNIQUE INDEX layers_genesis ON layers(layer_stack_id) WHERE parent_layer_id IS NULL;
CREATE UNIQUE INDEX layers_child ON layers(layer_stack_id, parent_layer_id) WHERE parent_layer_id IS NOT NULL;
CREATE UNIQUE INDEX layers_source ON layers(source_branch_id, source_commit_id) WHERE source_branch_id IS NOT NULL;
CREATE INDEX layers_parent ON layers(parent_layer_id);
CREATE INDEX commits_parent ON commits(parent_commit_id);
CREATE UNIQUE INDEX branch_identity ON branches(layer_stack_id, branch_id);
CREATE UNIQUE INDEX branch_names ON branches(layer_stack_id, name);
CREATE UNIQUE INDEX branch_pointer ON branches(branch_id, head_commit_id);
CREATE INDEX branches_head ON branches(head_commit_id);
CREATE INDEX branches_fork ON branches(forked_from_branch_id, forked_from_commit_id);
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "layerfs-v2-schema-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn exact_roles_census_and_parent_identity_are_enforced() {
        let authority_path = temp("authority");
        let branch_path = temp("branch");
        let authority = StoreDb::create(&authority_path, StoreRole::LayerStack, None).unwrap();
        let parent = authority.store_id();
        let branch = StoreDb::create(&branch_path, StoreRole::Branch, Some(parent)).unwrap();
        assert_eq!(branch.parent_store_id(), Some(parent));
        assert_eq!(census(&authority), (6, 25));
        assert_eq!(census(&branch), (9, 33));
        for db in [&authority, &branch] {
            let connection = db.connection().unwrap();
            assert_eq!(
                connection
                    .query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
                    .unwrap(),
                "wal"
            );
            assert_eq!(pragma(&connection, "synchronous"), 2);
            assert_eq!(pragma(&connection, "foreign_keys"), 1);
            assert_eq!(pragma(&connection, "busy_timeout"), 5_000);
            assert_eq!(pragma(&connection, "temp_store"), 1);
            assert_eq!(pragma(&connection, "cache_size"), -SQLITE_PAGE_CACHE_KIB);
            assert_eq!(
                pragma(&connection, "wal_autocheckpoint"),
                WAL_AUTOCHECKPOINT_PAGES
            );
        }
        for db in [&authority, &branch] {
            let receipt = db.stable_barrier().unwrap();
            receipt.validate().unwrap();
            assert_eq!(receipt.store_id, db.store_id());
            assert_eq!(receipt.role, db.role());
        }
        assert!(matches!(
            StoreDb::connect(&authority_path, StoreRole::Branch, Some(parent)),
            Err(StorageError::StoreBusy)
        ));
        drop(branch);
        drop(authority);
        assert!(matches!(
            StoreDb::connect(
                &branch_path,
                StoreRole::Branch,
                Some(StoreId::random().unwrap())
            ),
            Err(StorageError::WrongParent)
        ));
        std::fs::remove_file(authority_path).unwrap();
        std::fs::remove_file(branch_path).unwrap();
    }

    #[test]
    fn cold_open_rejects_same_census_with_changed_index_or_constraint() {
        let index_path = temp("index");
        let store = StoreDb::create(&index_path, StoreRole::LayerStack, None).unwrap();
        drop(store);
        let connection = Connection::open(&index_path).unwrap();
        connection
            .execute_batch(
                "DROP INDEX branches_head;
                 CREATE INDEX branches_head ON branches(branch_id);",
            )
            .unwrap();
        drop(connection);
        assert!(matches!(
            StoreDb::connect(&index_path, StoreRole::LayerStack, None),
            Err(StorageError::WrongStoreSchema)
        ));

        let constraint_path = temp("constraint");
        let connection = Connection::open(&constraint_path).unwrap();
        configure(&connection).unwrap();
        connection
            .pragma_update(
                None,
                "application_id",
                StoreRole::LayerStack.application_id(),
            )
            .unwrap();
        connection
            .pragma_update(None, "user_version", SCHEMA_VERSION)
            .unwrap();
        connection
            .execute_batch(&LAYERSTACK_SCHEMA.replace(
                "CHECK (length(name) BETWEEN 1 AND 63)",
                "CHECK (length(name) BETWEEN 0 AND 63)",
            ))
            .unwrap();
        connection
            .execute(
                "INSERT INTO store(singleton,store_id) VALUES(1,?1)",
                [crate::StorageId::as_slice(&StoreId::random().unwrap())],
            )
            .unwrap();
        drop(connection);
        assert!(matches!(
            StoreDb::connect(&constraint_path, StoreRole::LayerStack, None),
            Err(StorageError::WrongStoreSchema)
        ));
        std::fs::remove_file(index_path).unwrap();
        std::fs::remove_file(constraint_path).unwrap();
    }

    #[test]
    fn inventory_continuation_is_an_index_seek() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch("CREATE TABLE objects(object_id BLOB PRIMARY KEY,bytes BLOB) STRICT")
            .unwrap();
        let details = connection
            .prepare(
                "EXPLAIN QUERY PLAN
                 SELECT object_id,length(bytes) FROM objects
                 WHERE object_id>?1 ORDER BY object_id LIMIT ?2",
            )
            .unwrap()
            .query_map(rusqlite::params![vec![0_u8; 32], 513_i64], |row| {
                row.get::<_, String>(3)
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert!(details.iter().any(|detail| detail.contains("SEARCH")));
        assert!(details.iter().all(|detail| !detail.starts_with("SCAN ")));
    }

    fn census(db: &StoreDb) -> (i64, i64) {
        let connection = db.connection().unwrap();
        connection
            .query_row(
                "SELECT count(*),sum((SELECT count(*) FROM pragma_table_info(s.name)))
                 FROM sqlite_schema s WHERE type='table' AND name NOT LIKE 'sqlite_%'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap()
    }

    fn pragma(connection: &Connection, name: &str) -> i64 {
        connection
            .query_row(&format!("PRAGMA {name}"), [], |row| row.get(0))
            .unwrap()
    }
}
