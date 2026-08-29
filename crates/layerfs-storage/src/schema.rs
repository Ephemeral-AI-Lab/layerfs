use crate::{Fact, FactKind, Result, StorageError};
use rusqlite::types::Value;
use rusqlite::{Connection, OpenFlags};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

#[cfg(feature = "test-instrumentation")]
static SQL_TRACE: std::sync::OnceLock<Mutex<Vec<String>>> = std::sync::OnceLock::new();

#[cfg(feature = "test-instrumentation")]
fn trace_sql(event: rusqlite::trace::TraceEvent<'_>) {
    if let rusqlite::trace::TraceEvent::Stmt(_, sql) = event {
        SQL_TRACE
            .get_or_init(|| Mutex::new(Vec::new()))
            .lock()
            .expect("SQL trace")
            .push(sql.to_owned());
    }
}

#[cfg(feature = "test-instrumentation")]
pub fn reset_sql_trace() {
    SQL_TRACE
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .expect("SQL trace")
        .clear();
}

#[cfg(feature = "test-instrumentation")]
pub fn sql_trace() -> Vec<String> {
    SQL_TRACE
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .expect("SQL trace")
        .clone()
}

pub const SCHEMA_VERSION: i64 = 1;
pub const WAL_AUTOCHECKPOINT_PAGES: i64 = 1_000;
pub const SQLITE_PAGE_CACHE_KIB: i64 = 8 * 1_024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SchemaKind {
    Branch,
    Full,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoreRole {
    Layer,
    Stack,
    Branch,
}

impl StoreRole {
    const fn schema(self) -> SchemaKind {
        match self {
            Self::Layer | Self::Stack => SchemaKind::Full,
            Self::Branch => SchemaKind::Branch,
        }
    }

    const fn bytes(self) -> &'static [u8] {
        match self {
            Self::Layer => b"layer\n",
            Self::Stack => b"stack\n",
            Self::Branch => b"branch\n",
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
    identity: [u8; 32],
}

struct OwnerFile {
    path: PathBuf,
    _file: File,
}

struct CreatedStoreFile {
    path: PathBuf,
    remove: bool,
}

impl Drop for CreatedStoreFile {
    fn drop(&mut self) {
        if self.remove {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

impl Drop for OwnerFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
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
    pub fn create(path: impl AsRef<Path>, role: StoreRole) -> Result<Self> {
        Self::open(path.as_ref(), role, OpenMode::Create)
    }

    pub fn connect(path: impl AsRef<Path>, role: StoreRole) -> Result<Self> {
        Self::open(path.as_ref(), role, OpenMode::Connect)
    }

    #[doc(hidden)]
    pub fn fact_page(&self, kind: FactKind, after: Option<&[u8]>, limit: u16) -> Result<Vec<Fact>> {
        if limit == 0 || limit > 512 {
            return Err(StorageError::InvalidInput("fact query page"));
        }
        let (table, key, columns) = crate::admission::fact_shape(kind, self.kind())?;
        let sql = format!(
            "SELECT {} FROM {table} WHERE {key}>?1 ORDER BY {key} LIMIT ?2",
            columns.join(",")
        );
        let connection = self.connection()?;
        let mut statement = connection.prepare(&sql)?;
        let rows = statement
            .query_map(rusqlite::params![after.unwrap_or(&[]), limit], |row| {
                (0..columns.len())
                    .map(|index| row.get::<_, Value>(index))
                    .collect::<std::result::Result<Vec<_>, _>>()
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.iter()
            .map(|values| crate::admission::fact_from_values(kind, values))
            .collect()
    }

    fn open(path: &Path, role: StoreRole, mode: OpenMode) -> Result<Self> {
        let kind = role.schema();
        let path = absolute(path)?;
        if mode == OpenMode::Create {
            if path.exists() {
                return Err(StorageError::StoreAlreadyExists);
            }
        } else if !path.is_file() {
            return Err(StorageError::StoreMissing);
        }
        if mode == OpenMode::Create {
            let parent = path
                .parent()
                .ok_or(StorageError::InvalidInput("Store location"))?;
            std::fs::create_dir_all(parent)?;
        }
        let owner_path = appended(&path, ".owner");
        let owner_file = acquire_owner(&owner_path, path.parent().expect("absolute Store path"))?;
        let owner = OwnerFile {
            path: owner_path,
            _file: owner_file,
        };
        let role_path = appended(&path, ".role");
        let identity_path = appended(&path, ".store-id");
        let mut created = if mode == OpenMode::Create {
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
            Some(CreatedStoreFile {
                path: path.clone(),
                remove: true,
            })
        } else {
            None
        };
        let mut created_role = if mode == OpenMode::Create {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&role_path)
                .map_err(|error| {
                    if error.kind() == std::io::ErrorKind::AlreadyExists {
                        StorageError::StoreAlreadyExists
                    } else {
                        StorageError::Io(error)
                    }
                })?;
            file.write_all(role.bytes())?;
            file.sync_all()?;
            Some(CreatedStoreFile {
                path: role_path,
                remove: true,
            })
        } else {
            let actual = std::fs::read(&role_path).map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    StorageError::WrongStoreSchema
                } else {
                    StorageError::Io(error)
                }
            })?;
            if actual != role.bytes() {
                return Err(StorageError::WrongStoreRole);
            }
            None
        };
        let (identity, mut created_identity) = if mode == OpenMode::Create {
            let identity = new_store_identity(&path, role);
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&identity_path)?;
            file.write_all(&identity)?;
            file.sync_all()?;
            (
                identity,
                Some(CreatedStoreFile {
                    path: identity_path,
                    remove: true,
                }),
            )
        } else {
            let bytes = std::fs::read(&identity_path).map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    StorageError::WrongStoreSchema
                } else {
                    StorageError::Io(error)
                }
            })?;
            (
                bytes
                    .try_into()
                    .map_err(|_| StorageError::WrongStoreSchema)?,
                None,
            )
        };
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let connection = Connection::open_with_flags(&path, flags)?;
        #[cfg(feature = "test-instrumentation")]
        connection.trace_v2(
            rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
            Some(trace_sql),
        );
        let version: String =
            connection.query_row("SELECT sqlite_version()", [], |row| row.get(0))?;
        if sqlite_version(&version)? < (3, 35, 0) {
            return Err(StorageError::Integrity("SQLite version"));
        }
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        connection.pragma_update(None, "temp_store", "FILE")?;
        connection.pragma_update(None, "cache_size", -SQLITE_PAGE_CACHE_KIB)?;
        connection.pragma_update(None, "wal_autocheckpoint", WAL_AUTOCHECKPOINT_PAGES)?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        if mode == OpenMode::Create {
            install_schema(&connection, kind)?;
        } else {
            verify_schema(&connection, kind)?;
        }
        prepare_fixed_shapes(&connection, kind)?;
        let store = Self(Arc::new(StoreInner {
            connection: Mutex::new(connection),
            gate: TicketGate::default(),
            path,
            _owner: owner,
            role,
            identity,
        }));
        if let Some(created) = &mut created {
            created.remove = false;
        }
        if let Some(created) = &mut created_role {
            created.remove = false;
        }
        if let Some(created) = &mut created_identity {
            created.remove = false;
        }
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.0.path
    }

    pub fn kind(&self) -> SchemaKind {
        self.0.role.schema()
    }

    pub fn role(&self) -> StoreRole {
        self.0.role
    }

    pub fn identity(&self) -> [u8; 32] {
        self.0.identity
    }

    pub fn bind_parent(&self, parent: [u8; 32], create: bool) -> Result<()> {
        let path = appended(self.path(), ".parent");
        if create {
            let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
            file.write_all(&parent)?;
            file.sync_all()?;
            return Ok(());
        }
        let actual = std::fs::read(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                StorageError::WrongStoreSchema
            } else {
                StorageError::Io(error)
            }
        })?;
        if actual == parent {
            Ok(())
        } else {
            Err(StorageError::WrongParent)
        }
    }

    pub fn inventory_page(
        &self,
        after: Option<layerfs_content::ObjectId>,
        limit: u16,
    ) -> Result<crate::InventoryPage> {
        if limit == 0 || limit > crate::ID_BATCH_COUNT as u16 {
            return Err(StorageError::InvalidInput("inventory page"));
        }
        let connection = self.connection()?;
        let mut statement = connection.prepare_cached(
            "SELECT object_id,length(bytes) FROM objects
             WHERE (?1 IS NULL OR object_id>?1) ORDER BY object_id LIMIT ?2",
        )?;
        let entries = statement
            .query_map(
                rusqlite::params![after.map(|id| id.to_bytes().to_vec()), limit],
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, i64>(1)?)),
            )?
            .map(|row| {
                let (id, encoded_length) = row?;
                Ok(crate::InventoryEntry {
                    object_id: layerfs_content::ObjectId::from_bytes(&id)?,
                    encoded_length: encoded_length
                        .try_into()
                        .map_err(|_| StorageError::Integrity("object length"))?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let continuation = (entries.len() == usize::from(limit))
            .then(|| entries.last().map(|entry| entry.object_id))
            .flatten();
        Ok(crate::InventoryPage {
            entries,
            continuation,
        })
    }

    pub fn storage_snapshot(&self) -> Result<crate::StoreStorageSnapshot> {
        fn len(path: &Path) -> Result<u64> {
            match std::fs::metadata(path) {
                Ok(metadata) => Ok(metadata.len()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
                Err(error) => Err(StorageError::Io(error)),
            }
        }
        Ok(crate::StoreStorageSnapshot {
            database_bytes: len(self.path())?,
            wal_bytes: len(&appended(self.path(), "-wal"))?,
            shm_bytes: len(&appended(self.path(), "-shm"))?,
        })
    }

    pub fn enter_operation(&self) -> Result<OperationPermit<'_>> {
        self.0.gate.enter()
    }

    pub(crate) fn connection(&self) -> Result<MutexGuard<'_, Connection>> {
        self.0
            .connection
            .lock()
            .map_err(|_| StorageError::Integrity("database lock"))
    }
}

fn acquire_owner(path: &Path, parent: &Path) -> Result<File> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    for _ in 0..2 {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
        {
            Ok(mut file) => {
                writeln!(file, "{}", std::process::id())?;
                file.sync_all()?;
                return Ok(file);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let metadata = std::fs::symlink_metadata(path)?;
                if !metadata.file_type().is_file()
                    || metadata.uid() != std::fs::metadata(parent)?.uid()
                {
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

fn new_store_identity(path: &Path, role: StoreRole) -> [u8; 32] {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let mut input = Vec::new();
    input.extend_from_slice(role.bytes());
    input.extend_from_slice(path.as_os_str().as_encoded_bytes());
    input.extend_from_slice(&std::process::id().to_be_bytes());
    input.extend_from_slice(
        &std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_be_bytes(),
    );
    input.extend_from_slice(&SERIAL.fetch_add(1, Ordering::Relaxed).to_be_bytes());
    *blake3::hash(&input).as_bytes()
}

pub(crate) fn require_full(db: &StoreDb) -> Result<()> {
    if db.kind() == SchemaKind::Full {
        Ok(())
    } else {
        Err(StorageError::WrongSourceRoute)
    }
}

fn prepare_fixed_shapes(connection: &Connection, kind: SchemaKind) -> Result<()> {
    connection.prepare_cached(&membership_sql("objects", "object_id"))?;
    connection.prepare_cached(&crate::admission::fixed_insert_sql(
        "objects",
        &["object_id", "bytes"],
    ))?;
    for fact in [
        FactKind::Commit,
        FactKind::Branch,
        FactKind::LayerHistory,
        FactKind::Layer,
        FactKind::StackHistory,
        FactKind::Stack,
        FactKind::AddResult,
    ] {
        if let Ok((table, key, columns)) = crate::admission::fact_shape(fact, kind) {
            connection.prepare_cached(&membership_sql(table, key))?;
            connection.prepare_cached(&crate::admission::fixed_insert_sql(table, columns))?;
        }
    }
    Ok(())
}

pub(crate) fn membership_sql(table: &str, key: &str) -> String {
    let placeholders = (1..=crate::ID_BATCH_COUNT)
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(",");
    format!("SELECT {key} FROM {table} WHERE {key} IN ({placeholders})")
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

pub(crate) fn appended(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    value.into()
}

const OBJECTS: &str = "CREATE TABLE objects (
    object_id BLOB PRIMARY KEY NOT NULL,
    bytes BLOB NOT NULL
)";
const COMMITS: &str = "CREATE TABLE commits (
    commit_id BLOB PRIMARY KEY NOT NULL,
    root_id BLOB NOT NULL,
    parent_id BLOB,
    merge_parent_id BLOB,
    CHECK (merge_parent_id IS NULL OR parent_id IS NOT NULL),
    CHECK (merge_parent_id IS NULL OR merge_parent_id != parent_id)
)";
const BRANCHES: &str = "CREATE TABLE branches (
    branch_id BLOB PRIMARY KEY NOT NULL,
    head_commit_id BLOB NOT NULL,
    base_id BLOB NOT NULL
)";
const LAYER_HISTORIES: &str = "CREATE TABLE layer_histories (
    history_id BLOB PRIMARY KEY NOT NULL,
    head_layer_id BLOB NOT NULL
)";
const LAYERS: &str = "CREATE TABLE layers (
    layer_id BLOB PRIMARY KEY NOT NULL,
    history_id BLOB NOT NULL,
    parent_id BLOB,
    root_id BLOB NOT NULL
)";
const STACK_HISTORIES: &str = "CREATE TABLE stack_histories (
    history_id BLOB PRIMARY KEY NOT NULL,
    base_layer_id BLOB NOT NULL,
    head_stack_id BLOB NOT NULL
)";
const STACKS: &str = "CREATE TABLE stacks (
    stack_id BLOB PRIMARY KEY NOT NULL,
    history_id BLOB NOT NULL,
    parent_id BLOB,
    root_id BLOB NOT NULL
)";
const ADD_RESULTS: &str = "CREATE TABLE add_results (
    source_id BLOB PRIMARY KEY NOT NULL,
    result_id BLOB NOT NULL
)";

const INDEXES: &[&str] = &[
    "CREATE INDEX commits_parent ON commits(parent_id)",
    "CREATE INDEX commits_merge_parent ON commits(merge_parent_id)",
];

const FULL_INDEXES: &[&str] = &[
    "CREATE UNIQUE INDEX layers_genesis ON layers(history_id) WHERE parent_id IS NULL",
    "CREATE UNIQUE INDEX layers_child ON layers(history_id, parent_id) WHERE parent_id IS NOT NULL",
    "CREATE UNIQUE INDEX stacks_seed ON stacks(history_id) WHERE parent_id IS NULL",
    "CREATE UNIQUE INDEX stacks_child ON stacks(history_id, parent_id) WHERE parent_id IS NOT NULL",
    "CREATE INDEX add_results_result ON add_results(result_id)",
];

pub fn install_schema(connection: &Connection, kind: SchemaKind) -> Result<()> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    let table_count: i64 = connection.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        [],
        |row| row.get(0),
    )?;
    if version == 0 && table_count == 0 {
        let transaction = connection.unchecked_transaction()?;
        for ddl in [OBJECTS, COMMITS, BRANCHES] {
            transaction.execute_batch(ddl)?;
        }
        for ddl in INDEXES {
            transaction.execute_batch(ddl)?;
        }
        if kind == SchemaKind::Full {
            for ddl in [
                LAYER_HISTORIES,
                LAYERS,
                STACK_HISTORIES,
                STACKS,
                ADD_RESULTS,
            ] {
                transaction.execute_batch(ddl)?;
            }
            for ddl in FULL_INDEXES {
                transaction.execute_batch(ddl)?;
            }
        }
        transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        transaction.commit()?;
    }
    verify_schema(connection, kind)
}

pub fn verify_schema(connection: &Connection, kind: SchemaKind) -> Result<()> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version != SCHEMA_VERSION {
        return Err(StorageError::WrongStoreSchema);
    }
    let expected: &[(&str, &[&str])] = match kind {
        SchemaKind::Branch => &[
            ("branches", &["branch_id", "head_commit_id", "base_id"]),
            (
                "commits",
                &["commit_id", "root_id", "parent_id", "merge_parent_id"],
            ),
            ("objects", &["object_id", "bytes"]),
        ],
        SchemaKind::Full => &[
            ("add_results", &["source_id", "result_id"]),
            ("branches", &["branch_id", "head_commit_id", "base_id"]),
            (
                "commits",
                &["commit_id", "root_id", "parent_id", "merge_parent_id"],
            ),
            ("layer_histories", &["history_id", "head_layer_id"]),
            (
                "layers",
                &["layer_id", "history_id", "parent_id", "root_id"],
            ),
            ("objects", &["object_id", "bytes"]),
            (
                "stack_histories",
                &["history_id", "base_layer_id", "head_stack_id"],
            ),
            (
                "stacks",
                &["stack_id", "history_id", "parent_id", "root_id"],
            ),
        ],
    };
    let mut statement = connection.prepare(
        "SELECT name FROM sqlite_master
         WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let expected_names = expected
        .iter()
        .map(|(name, _)| (*name).to_owned())
        .collect::<Vec<_>>();
    if names != expected_names {
        let other = match kind {
            SchemaKind::Branch => [
                "add_results",
                "branches",
                "commits",
                "layer_histories",
                "layers",
                "objects",
                "stack_histories",
                "stacks",
            ]
            .as_slice(),
            SchemaKind::Full => ["branches", "commits", "objects"].as_slice(),
        };
        if names
            == other
                .iter()
                .map(|name| (*name).to_owned())
                .collect::<Vec<_>>()
        {
            return Err(StorageError::WrongStoreRole);
        }
        return Err(StorageError::WrongStoreSchema);
    }
    let expected_ddl = match kind {
        SchemaKind::Branch => vec![
            ("branches", BRANCHES),
            ("commits", COMMITS),
            ("objects", OBJECTS),
        ],
        SchemaKind::Full => vec![
            ("add_results", ADD_RESULTS),
            ("branches", BRANCHES),
            ("commits", COMMITS),
            ("layer_histories", LAYER_HISTORIES),
            ("layers", LAYERS),
            ("objects", OBJECTS),
            ("stack_histories", STACK_HISTORIES),
            ("stacks", STACKS),
        ],
    };
    let mut statement = connection.prepare(
        "SELECT name,sql FROM sqlite_master
         WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let actual_ddl = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if actual_ddl
        .iter()
        .map(|(name, sql)| (name.as_str(), normalized(sql)))
        .ne(expected_ddl
            .iter()
            .map(|(name, sql)| (*name, normalized(sql))))
    {
        return Err(StorageError::WrongStoreSchema);
    }
    for (table, columns) in expected {
        let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
        let actual = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if actual
            != columns
                .iter()
                .map(|column| (*column).to_owned())
                .collect::<Vec<_>>()
        {
            return Err(StorageError::WrongStoreSchema);
        }
    }
    let columns = expected
        .iter()
        .map(|(_, columns)| columns.len())
        .sum::<usize>();
    if (kind == SchemaKind::Branch && columns != 9) || (kind == SchemaKind::Full && columns != 24) {
        return Err(StorageError::WrongStoreSchema);
    }
    let expected_indexes = INDEXES
        .iter()
        .chain(
            (kind == SchemaKind::Full)
                .then_some(FULL_INDEXES)
                .into_iter()
                .flatten(),
        )
        .map(|sql| normalized(sql))
        .collect::<Vec<_>>();
    let mut statement = connection.prepare(
        "SELECT sql FROM sqlite_master WHERE type='index' AND sql IS NOT NULL ORDER BY name",
    )?;
    let actual_indexes = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .map(|row| row.map(|sql| normalized(&sql)))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut expected_indexes = expected_indexes;
    expected_indexes.sort();
    let mut actual_indexes = actual_indexes;
    actual_indexes.sort();
    if actual_indexes != expected_indexes {
        return Err(StorageError::WrongStoreSchema);
    }
    Ok(())
}

fn normalized(sql: &str) -> String {
    sql.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_manifests_and_wrong_shape_rejection() {
        for kind in [SchemaKind::Branch, SchemaKind::Full] {
            let connection = Connection::open_in_memory().unwrap();
            install_schema(&connection, kind).unwrap();
            verify_schema(&connection, kind).unwrap();
            connection
                .execute_batch("CREATE TABLE extra(value BLOB)")
                .unwrap();
            assert!(verify_schema(&connection, kind).is_err());
        }

        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(OBJECTS).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE commits (
                    commit_id BLOB PRIMARY KEY NOT NULL,
                    root_id BLOB NOT NULL,
                    parent_id BLOB,
                    merge_parent_id BLOB
                )",
            )
            .unwrap();
        connection.execute_batch(BRANCHES).unwrap();
        for ddl in INDEXES {
            connection.execute_batch(ddl).unwrap();
        }
        connection
            .pragma_update(None, "user_version", SCHEMA_VERSION)
            .unwrap();
        assert!(verify_schema(&connection, SchemaKind::Branch).is_err());
    }

    #[test]
    fn create_and_connect_are_explicit_and_role_typed() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-create-connect-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = root.join("store.sqlite");
        assert!(matches!(
            StoreDb::connect(&path, StoreRole::Layer),
            Err(StorageError::StoreMissing)
        ));
        assert!(!path.exists());

        let store = StoreDb::create(&path, StoreRole::Layer).unwrap();
        assert_eq!(store.role(), StoreRole::Layer);
        drop(store);
        let before = std::fs::read(&path).unwrap();
        assert!(matches!(
            StoreDb::create(&path, StoreRole::Layer),
            Err(StorageError::StoreAlreadyExists)
        ));
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(matches!(
            StoreDb::connect(&path, StoreRole::Stack),
            Err(StorageError::WrongStoreRole)
        ));
        assert_eq!(std::fs::read(&path).unwrap(), before);

        let connected = StoreDb::connect(&path, StoreRole::Layer).unwrap();
        assert_eq!(connected.role(), StoreRole::Layer);
        drop(connected);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn second_database_owner_is_rejected() {
        let root = std::env::temp_dir().join(format!(
            "layerfs-owner-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("store.sqlite");
        let owner = crate::StoreDb::create(&path, StoreRole::Branch).unwrap();
        assert!(matches!(
            crate::StoreDb::connect(&path, StoreRole::Branch),
            Err(StorageError::StoreBusy)
        ));
        drop(owner);
        crate::StoreDb::connect(&path, StoreRole::Branch).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn receiver_gate_is_fifo_single_active_and_does_not_block_database_reads() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let root = std::env::temp_dir().join(format!(
            "layerfs-gate-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let db = StoreDb::create(root.join("store.sqlite"), StoreRole::Branch).unwrap();
        let first = db.enter_operation().unwrap();
        let order = Arc::new(Mutex::new(Vec::new()));
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let mut threads = Vec::new();
        for index in 0..10 {
            let queued = std::sync::mpsc::sync_channel(0);
            let worker = db.clone();
            let order = order.clone();
            let active = active.clone();
            let maximum = maximum.clone();
            threads.push(std::thread::spawn(move || {
                queued.0.send(()).unwrap();
                let _permit = worker.enter_operation().unwrap();
                let count = active.fetch_add(1, Ordering::SeqCst) + 1;
                maximum.fetch_max(count, Ordering::SeqCst);
                order.lock().unwrap().push(index);
                active.fetch_sub(1, Ordering::SeqCst);
            }));
            queued.1.recv().unwrap();
            while db.0.gate.state.lock().unwrap().next != index as u64 + 2 {
                std::thread::yield_now();
            }
        }
        assert_eq!(
            db.connection()
                .unwrap()
                .query_row("SELECT 1", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
        drop(first);
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(*order.lock().unwrap(), (0..10).collect::<Vec<_>>());
        assert_eq!(maximum.load(Ordering::SeqCst), 1);
        assert_eq!(active.load(Ordering::SeqCst), 0);
        drop(db);
        std::fs::remove_dir_all(root).unwrap();
    }
}
