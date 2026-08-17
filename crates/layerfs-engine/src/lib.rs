//! Minimal Phase 4A durable LayerFS engine.

#![forbid(unsafe_code)]

use layerfs_core::{
    validate_identity, validate_object_from, CoreError, ObjectId, ObjectKind, ObjectSummary,
};
use rusqlite::{params, Connection, OptionalExtension};
use std::fmt;
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

pub const COMPONENT: &str = "layerfs-engine";
const FORMAT_MARKER: &str = "layerfs-phase4a-sqlite-blob";
const SCHEMA_VERSION: i64 = 1;
const BUSY_TIMEOUT: Duration = Duration::from_millis(100);
const ROOT_RECORD_BASE_BYTES: u64 = 64;
const OBJECTS_TABLE: &str = "layerfs_objects";
const OBJECTS_BLOB_COLUMN: &str = "canonical_bytes";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SqliteErrorKind {
    Busy,
    Locked,
    PermissionDenied,
    NoSpace,
    Corrupt,
    ReadOnly,
    Constraint,
    Io,
    Other,
}

#[derive(Debug, Eq, PartialEq)]
pub enum EngineError {
    Core(CoreError),
    Sqlite {
        kind: SqliteErrorKind,
        message: String,
    },
    MissingObject(ObjectId),
    MissingRoot(ObjectId),
    MissingDelta(ObjectId),
    IdentityMismatch {
        expected: ObjectId,
        actual: ObjectId,
    },
    MalformedObject {
        id: ObjectId,
        cause: CoreError,
    },
    ImmutableConflict(&'static str, ObjectId),
    InvalidRange {
        start: u64,
        end: u64,
        length: u64,
    },
    ShortRead {
        expected: u64,
        actual: u64,
    },
    ParentMismatch {
        expected: Option<ObjectId>,
        actual: Option<ObjectId>,
    },
    SchemaMismatch,
    ProfileMismatch,
    InvalidRecord(&'static str),
    InvalidTransaction,
    CounterOverflow,
    InjectedFailure(&'static str),
}

impl fmt::Display for EngineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Core(error) => error.fmt(formatter),
            Self::Sqlite { kind, message } => write!(formatter, "SQLite {kind:?}: {message}"),
            Self::MissingObject(id) => write!(formatter, "object {id} is missing"),
            Self::MissingRoot(id) => write!(formatter, "root {id} is missing"),
            Self::MissingDelta(id) => write!(formatter, "delta {id} is missing"),
            Self::IdentityMismatch { expected, actual } => {
                write!(
                    formatter,
                    "identity mismatch: expected {expected}, got {actual}"
                )
            }
            Self::MalformedObject { id, cause } => {
                write!(formatter, "object {id} is malformed: {cause}")
            }
            Self::ImmutableConflict(kind, id) => {
                write!(formatter, "immutable {kind} {id} conflicts")
            }
            Self::InvalidRange { start, end, length } => {
                write!(
                    formatter,
                    "invalid range {start}..{end} for length {length}"
                )
            }
            Self::ShortRead { expected, actual } => {
                write!(formatter, "short read: expected {expected}, got {actual}")
            }
            Self::ParentMismatch { expected, actual } => {
                write!(
                    formatter,
                    "parent mismatch: expected {expected:?}, got {actual:?}"
                )
            }
            Self::SchemaMismatch => formatter.write_str("SQLite schema marker mismatch"),
            Self::ProfileMismatch => formatter.write_str("SQLite profile mismatch"),
            Self::InvalidRecord(name) => write!(formatter, "invalid durable {name} record"),
            Self::InvalidTransaction => formatter.write_str("capture transaction is not active"),
            Self::CounterOverflow => formatter.write_str("counter arithmetic overflow"),
            Self::InjectedFailure(point) => write!(formatter, "injected failure at {point}"),
        }
    }
}

impl std::error::Error for EngineError {}

impl From<CoreError> for EngineError {
    fn from(error: CoreError) -> Self {
        Self::Core(error)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObjectRecord {
    pub id: ObjectId,
    pub kind: ObjectKind,
    pub canonical_len: u64,
    pub canonical_bytes: Vec<u8>,
}

impl ObjectRecord {
    pub fn new(id: ObjectId, canonical_bytes: Vec<u8>) -> EngineResult<Self> {
        let object = validate_identity(&canonical_bytes, id)
            .map_err(|cause| EngineError::MalformedObject { id, cause })?;
        let canonical_len =
            u64::try_from(canonical_bytes.len()).map_err(|_| EngineError::CounterOverflow)?;
        Ok(Self {
            id,
            kind: object.kind(),
            canonical_len,
            canonical_bytes,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RootRecord {
    pub id: ObjectId,
    pub directory_object: ObjectId,
    pub parent: Option<ObjectId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeltaRecord {
    pub id: ObjectId,
    pub parent: Option<ObjectId>,
    pub child: ObjectId,
    pub payload: Vec<u8>,
}

impl DeltaRecord {
    pub fn new(parent: Option<ObjectId>, child: ObjectId, payload: Vec<u8>) -> Self {
        Self {
            id: delta_identity(parent, child, &payload),
            parent,
            child,
            payload,
        }
    }

    fn validate(&self) -> EngineResult<()> {
        let actual = delta_identity(self.parent, self.child, &self.payload);
        if actual != self.id {
            return Err(EngineError::IdentityMismatch {
                expected: self.id,
                actual,
            });
        }
        Ok(())
    }
}

fn delta_identity(parent: Option<ObjectId>, child: ObjectId, payload: &[u8]) -> ObjectId {
    let mut identity = Vec::with_capacity(1 + 32 + 32 + payload.len());
    identity.extend_from_slice(b"layerfs-phase4a-delta-v1");
    match parent {
        Some(parent) => {
            identity.push(1);
            identity.extend_from_slice(parent.as_bytes());
        }
        None => identity.push(0),
    }
    identity.extend_from_slice(child.as_bytes());
    identity.extend_from_slice(payload);
    ObjectId::for_bytes(&identity)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SqliteProfile {
    pub journal_mode: String,
    pub synchronous: i64,
    pub temp_store: i64,
    pub mmap_size: i64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EngineCounters {
    pub transactions_started: u64,
    pub transactions_committed: u64,
    pub transactions_rolled_back: u64,
    pub statements: u64,
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
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StorageObservation {
    pub database_bytes: Option<u64>,
    pub rollback_journal_bytes: Option<u64>,
    pub temporary_file_bytes: Option<u64>,
    pub logical_engine_bytes: Option<u64>,
}

pub type EngineResult<T> = Result<T, EngineError>;

pub struct Engine {
    path: PathBuf,
    connection: Mutex<Connection>,
    counters: Mutex<EngineCounters>,
    rollback_journal_sample: Mutex<Option<u64>>,
    profile: SqliteProfile,
}

impl Engine {
    pub fn open(path: impl AsRef<Path>) -> EngineResult<Self> {
        let path = path.as_ref().to_owned();
        let connection = Connection::open(&path).map_err(map_sqlite_error)?;
        connection
            .busy_timeout(BUSY_TIMEOUT)
            .map_err(map_sqlite_error)?;
        let profile = configure_profile(&connection)?;
        initialize_schema(&connection, &profile)?;
        Ok(Self {
            path,
            connection: Mutex::new(connection),
            counters: Mutex::new(EngineCounters::default()),
            rollback_journal_sample: Mutex::new(None),
            profile,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn profile(&self) -> &SqliteProfile {
        &self.profile
    }

    pub fn counters(&self) -> EngineResult<EngineCounters> {
        self.counters
            .lock()
            .map(|counters| *counters)
            .map_err(|_| EngineError::Sqlite {
                kind: SqliteErrorKind::Other,
                message: "counter mutex poisoned".to_owned(),
            })
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
        let objects = connection
            .query_row(
                "SELECT COALESCE(SUM(canonical_length), 0) FROM layerfs_objects",
                [],
                |row| row.get::<_, i64>(0),
            )
            .ok()
            .and_then(|value| u64::try_from(value).ok())?;
        let roots = connection
            .query_row(
                "SELECT COALESCE(SUM(64 + CASE WHEN parent_root IS NULL THEN 0 ELSE 32 END), 0)
                 FROM layerfs_roots",
                [],
                |row| row.get::<_, i64>(0),
            )
            .ok()
            .and_then(|value| u64::try_from(value).ok())?;
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

    pub fn load_visible_root(&self) -> EngineResult<Option<RootId>> {
        let connection = self.lock_connection()?;
        let root = visible_root_on_connection(self, &connection)?;
        if let Some(root) = root {
            let record = load_root_on_connection(self, &connection, root)?;
            authenticate_directory_object(self, &connection, record.directory_object)?;
        }
        Ok(root)
    }

    pub fn load_root(&self, id: RootId) -> EngineResult<RootRecord> {
        let connection = self.lock_connection()?;
        let record = load_root_on_connection(self, &connection, id)?;
        authenticate_directory_object(self, &connection, record.directory_object)?;
        Ok(record)
    }

    pub fn load_delta(&self, id: ObjectId) -> EngineResult<DeltaRecord> {
        let connection = self.lock_connection()?;
        load_delta_on_connection(self, &connection, id)
    }

    pub fn load_object(&self, id: ObjectId) -> EngineResult<ObjectRecord> {
        let length = self.object_length(id)?;
        let bytes = self.read_object_range(id, 0..length)?;
        ObjectRecord::new(id, bytes)
    }

    pub fn object_length(&self, id: ObjectId) -> EngineResult<u64> {
        let connection = self.lock_connection()?;
        Ok(object_meta(self, &connection, id)?
            .ok_or(EngineError::MissingObject(id))?
            .canonical_len)
    }

    pub fn read_object_range(&self, id: ObjectId, range: Range<u64>) -> EngineResult<Vec<u8>> {
        let connection = self.lock_connection()?;
        read_object_range_on_connection(self, &connection, id, range)
    }

    pub fn put_object_if_absent(
        &self,
        id: ObjectId,
        canonical_bytes: &[u8],
    ) -> EngineResult<PutOutcome> {
        let connection = self.lock_connection()?;
        put_object_on_connection(self, &connection, id, canonical_bytes)
    }

    pub fn begin_capture(&self, parent: Option<RootId>) -> EngineResult<Capture<'_>> {
        let connection = self.lock_connection()?;
        self.mark_statement()?;
        if let Err(error) = connection.execute_batch("BEGIN IMMEDIATE") {
            self.note_sqlite_error(&error)?;
            return Err(map_sqlite_error(error));
        }
        self.bump(|counters| checked_add(&mut counters.transactions_started, 1))?;

        let current = match visible_root_on_connection(self, &connection) {
            Ok(current) => current,
            Err(error) => {
                let _ = connection.execute_batch("ROLLBACK");
                self.bump_best_effort(|counters| {
                    checked_add(&mut counters.transactions_rolled_back, 1)
                });
                return Err(error);
            }
        };
        if current != parent {
            let _ = connection.execute_batch("ROLLBACK");
            self.bump_best_effort(|counters| {
                checked_add(&mut counters.transactions_rolled_back, 1)
            });
            return Err(EngineError::ParentMismatch {
                expected: parent,
                actual: current,
            });
        }
        if let Some(root) = current {
            let record = match load_root_on_connection(self, &connection, root) {
                Ok(record) => record,
                Err(error) => {
                    let _ = connection.execute_batch("ROLLBACK");
                    self.bump_best_effort(|counters| {
                        checked_add(&mut counters.transactions_rolled_back, 1)
                    });
                    return Err(error);
                }
            };
            if let Err(error) =
                authenticate_directory_object(self, &connection, record.directory_object)
            {
                let _ = connection.execute_batch("ROLLBACK");
                self.bump_best_effort(|counters| {
                    checked_add(&mut counters.transactions_rolled_back, 1)
                });
                return Err(error);
            }
        }
        Ok(Capture {
            engine: self,
            connection,
            parent,
            delta: None,
            active: true,
            #[cfg(test)]
            fault: None,
        })
    }

    fn lock_connection(&self) -> EngineResult<MutexGuard<'_, Connection>> {
        self.connection.lock().map_err(|_| EngineError::Sqlite {
            kind: SqliteErrorKind::Other,
            message: "connection mutex poisoned".to_owned(),
        })
    }

    fn mark_statement(&self) -> EngineResult<()> {
        self.bump(|counters| checked_add(&mut counters.statements, 1))
    }

    fn sample_rollback_journal(&self) {
        let mut sample = match self.rollback_journal_sample.lock() {
            Ok(sample) => sample,
            Err(_) => return,
        };
        let mut journal_path = self.path.as_os_str().to_os_string();
        journal_path.push("-journal");
        if let Ok(metadata) = fs::metadata(PathBuf::from(journal_path)) {
            *sample = Some(sample.map_or(metadata.len(), |current| current.max(metadata.len())));
        }
    }

    fn bump<F>(&self, update: F) -> EngineResult<()>
    where
        F: FnOnce(&mut EngineCounters) -> EngineResult<()>,
    {
        let mut counters = self.counters.lock().map_err(|_| EngineError::Sqlite {
            kind: SqliteErrorKind::Other,
            message: "counter mutex poisoned".to_owned(),
        })?;
        update(&mut counters)
    }

    fn bump_best_effort<F>(&self, update: F)
    where
        F: FnOnce(&mut EngineCounters) -> EngineResult<()>,
    {
        if let Ok(mut counters) = self.counters.lock() {
            let _ = update(&mut counters);
        }
    }

    fn note_sqlite_error(&self, error: &rusqlite::Error) -> EngineResult<()> {
        match sqlite_error_kind(error) {
            SqliteErrorKind::Busy => {
                self.bump(|counters| checked_add(&mut counters.busy_events, 1))
            }
            SqliteErrorKind::Locked => {
                self.bump(|counters| checked_add(&mut counters.locked_events, 1))
            }
            _ => Ok(()),
        }
    }
}

pub type RootId = ObjectId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PutOutcome {
    Created,
    Reused,
}

pub struct Capture<'a> {
    engine: &'a Engine,
    connection: MutexGuard<'a, Connection>,
    parent: Option<RootId>,
    delta: Option<DeltaRecord>,
    active: bool,
    #[cfg(test)]
    fault: Option<FaultPoint>,
}

impl<'a> Capture<'a> {
    pub fn put_object_if_absent(
        &mut self,
        id: ObjectId,
        canonical_bytes: &[u8],
    ) -> EngineResult<PutOutcome> {
        self.ensure_active()?;
        put_object_on_connection(self.engine, &self.connection, id, canonical_bytes)
    }

    pub fn write_delta(&mut self, delta: &DeltaRecord) -> EngineResult<()> {
        self.ensure_active()?;
        delta.validate()?;
        if delta.parent != self.parent {
            return Err(EngineError::ParentMismatch {
                expected: self.parent,
                actual: delta.parent,
            });
        }
        self.engine.mark_statement()?;
        let mut select = self
            .connection
            .prepare_cached(
                "SELECT parent_root, child_root, payload FROM layerfs_deltas WHERE delta_id = ?1",
            )
            .map_err(map_sqlite_error)?;
        let existing = select
            .query_row(params![delta.id.as_bytes().as_slice()], |row| {
                Ok((
                    row.get::<_, Option<Vec<u8>>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            })
            .optional()
            .map_err(map_sqlite_error)?;
        if let Some((parent, child, payload)) = existing {
            let existing = decode_delta_parts(delta.id, parent, child, payload)?;
            if existing != *delta {
                return Err(EngineError::ImmutableConflict("delta", delta.id));
            }
            self.delta = Some(delta.clone());
            return Ok(());
        }

        self.engine.mark_statement()?;
        let mut insert = self
            .connection
            .prepare_cached(
                "INSERT INTO layerfs_deltas (delta_id, parent_root, child_root, payload)
                 VALUES (?1, ?2, ?3, ?4)",
            )
            .map_err(map_sqlite_error)?;
        insert
            .execute(params![
                delta.id.as_bytes().as_slice(),
                delta.parent.map(|id| id.to_bytes().to_vec()),
                delta.child.as_bytes().as_slice(),
                &delta.payload,
            ])
            .map_err(map_sqlite_error)?;
        self.engine.bump(|counters| {
            checked_add(&mut counters.logical_delta_bytes, delta_record_len(delta)?)
        })?;
        self.delta = Some(delta.clone());
        Ok(())
    }

    pub fn commit_root(mut self, root: RootRecord) -> EngineResult<()> {
        self.ensure_active()?;
        if root.parent != self.parent {
            return Err(EngineError::ParentMismatch {
                expected: self.parent,
                actual: root.parent,
            });
        }
        let delta = self
            .delta
            .as_ref()
            .ok_or(EngineError::InvalidRecord("delta"))?;
        if delta.child != root.id {
            return Err(EngineError::InvalidRecord("root/delta linkage"));
        }
        authenticate_directory_object(self.engine, &self.connection, root.directory_object)?;
        write_root_on_connection(self.engine, &self.connection, &root)?;
        #[cfg(test)]
        if self.fault == Some(FaultPoint::BeforeVisibleRoot) {
            return Err(EngineError::InjectedFailure("visible root"));
        }
        self.engine.mark_statement()?;
        let mut update = self
            .connection
            .prepare_cached("UPDATE layerfs_store_meta SET visible_root = ?1 WHERE store_id = 1")
            .map_err(map_sqlite_error)?;
        let changed = update
            .execute(params![root.id.as_bytes().as_slice()])
            .map_err(map_sqlite_error)?;
        if changed != 1 {
            return Err(EngineError::SchemaMismatch);
        }
        self.engine.sample_rollback_journal();
        self.engine.mark_statement()?;
        self.connection
            .execute_batch("COMMIT")
            .map_err(map_sqlite_error)?;
        self.active = false;
        self.engine.bump(|counters| {
            checked_add(&mut counters.transactions_committed, 1)?;
            checked_add(&mut counters.logical_root_bytes, root_record_len(&root)?)
        })?;
        Ok(())
    }

    pub fn rollback(mut self) -> EngineResult<()> {
        self.ensure_active()?;
        self.engine.mark_statement()?;
        self.connection
            .execute_batch("ROLLBACK")
            .map_err(map_sqlite_error)?;
        self.active = false;
        self.engine
            .bump(|counters| checked_add(&mut counters.transactions_rolled_back, 1))
    }

    fn ensure_active(&self) -> EngineResult<()> {
        if self.active {
            Ok(())
        } else {
            Err(EngineError::InvalidTransaction)
        }
    }

    #[cfg(test)]
    fn fail_before_visible_root(&mut self) {
        self.fault = Some(FaultPoint::BeforeVisibleRoot);
    }
}

impl Drop for Capture<'_> {
    fn drop(&mut self) {
        if self.active && self.connection.execute_batch("ROLLBACK").is_ok() {
            self.active = false;
            self.engine.bump_best_effort(|counters| {
                checked_add(&mut counters.transactions_rolled_back, 1)
            });
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FaultPoint {
    BeforeVisibleRoot,
}

fn configure_profile(connection: &Connection) -> EngineResult<SqliteProfile> {
    let journal_mode = connection
        .query_row("PRAGMA journal_mode=DELETE", [], |row| {
            row.get::<_, String>(0)
        })
        .map_err(map_sqlite_error)?;
    connection
        .execute_batch("PRAGMA synchronous=FULL; PRAGMA temp_store=FILE; PRAGMA mmap_size=0;")
        .map_err(map_sqlite_error)?;
    let synchronous = connection
        .query_row("PRAGMA synchronous", [], |row| row.get::<_, i64>(0))
        .map_err(map_sqlite_error)?;
    let temp_store = connection
        .query_row("PRAGMA temp_store", [], |row| row.get::<_, i64>(0))
        .map_err(map_sqlite_error)?;
    let mmap_size = connection
        .query_row("PRAGMA mmap_size", [], |row| row.get::<_, i64>(0))
        .map_err(map_sqlite_error)?;
    let profile = SqliteProfile {
        journal_mode,
        synchronous,
        temp_store,
        mmap_size,
    };
    if !profile.journal_mode.eq_ignore_ascii_case("DELETE")
        || profile.synchronous != 2
        || profile.temp_store != 1
        || profile.mmap_size != 0
    {
        return Err(EngineError::ProfileMismatch);
    }
    Ok(profile)
}

fn initialize_schema(connection: &Connection, profile: &SqliteProfile) -> EngineResult<()> {
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS layerfs_store_meta (
                store_id INTEGER PRIMARY KEY CHECK (store_id = 1),
                format_marker TEXT NOT NULL,
                schema_version INTEGER NOT NULL,
                journal_mode TEXT NOT NULL,
                synchronous INTEGER NOT NULL,
                temp_store INTEGER NOT NULL,
                mmap_size INTEGER NOT NULL,
                visible_root BLOB
            );
            CREATE TABLE IF NOT EXISTS layerfs_objects (
                rowid INTEGER PRIMARY KEY,
                object_id BLOB NOT NULL UNIQUE,
                kind INTEGER NOT NULL,
                canonical_length INTEGER NOT NULL,
                canonical_bytes BLOB NOT NULL
            );
            CREATE TABLE IF NOT EXISTS layerfs_roots (
                root_id BLOB PRIMARY KEY,
                directory_object BLOB NOT NULL,
                parent_root BLOB
            );
            CREATE TABLE IF NOT EXISTS layerfs_deltas (
                delta_id BLOB PRIMARY KEY,
                parent_root BLOB,
                child_root BLOB NOT NULL,
                payload BLOB NOT NULL
            );",
        )
        .map_err(map_sqlite_error)?;
    let existing = connection
        .query_row(
            "SELECT format_marker, schema_version, journal_mode, synchronous, temp_store, mmap_size
             FROM layerfs_store_meta WHERE store_id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?;
    match existing {
        Some((marker, version, journal_mode, synchronous, temp_store, mmap_size)) => {
            if marker != FORMAT_MARKER
                || version != SCHEMA_VERSION
                || !journal_mode.eq_ignore_ascii_case(&profile.journal_mode)
                || synchronous != profile.synchronous
                || temp_store != profile.temp_store
                || mmap_size != profile.mmap_size
            {
                return Err(EngineError::SchemaMismatch);
            }
        }
        None => {
            connection
                .execute(
                    "INSERT INTO layerfs_store_meta
                     (store_id, format_marker, schema_version, journal_mode, synchronous, temp_store, mmap_size)
                     VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        FORMAT_MARKER,
                        SCHEMA_VERSION,
                        &profile.journal_mode,
                        profile.synchronous,
                        profile.temp_store,
                        profile.mmap_size,
                    ],
                )
                .map_err(map_sqlite_error)?;
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
struct ObjectMeta {
    row_id: i64,
    kind: ObjectKind,
    canonical_len: u64,
    blob_len: u64,
}

fn object_meta(
    engine: &Engine,
    connection: &Connection,
    id: ObjectId,
) -> EngineResult<Option<ObjectMeta>> {
    engine.mark_statement()?;
    let mut statement = connection
        .prepare_cached(
            "SELECT rowid, kind, canonical_length, length(canonical_bytes)
             FROM layerfs_objects WHERE object_id = ?1",
        )
        .map_err(map_sqlite_error)?;
    let row = statement
        .query_row(params![id.as_bytes().as_slice()], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .optional()
        .map_err(map_sqlite_error)?;
    row.map(|(row_id, kind, canonical_len, blob_len)| {
        let kind = ObjectKind::try_from(
            u8::try_from(kind).map_err(|_| EngineError::InvalidRecord("object kind"))?,
        )?;
        let canonical_len = u64::try_from(canonical_len)
            .map_err(|_| EngineError::InvalidRecord("object length"))?;
        let blob_len =
            u64::try_from(blob_len).map_err(|_| EngineError::InvalidRecord("blob length"))?;
        Ok(ObjectMeta {
            row_id,
            kind,
            canonical_len,
            blob_len,
        })
    })
    .transpose()
}

fn put_object_on_connection(
    engine: &Engine,
    connection: &Connection,
    id: ObjectId,
    canonical_bytes: &[u8],
) -> EngineResult<PutOutcome> {
    let object = validate_identity(canonical_bytes, id)
        .map_err(|cause| EngineError::MalformedObject { id, cause })?;
    let canonical_len =
        u64::try_from(canonical_bytes.len()).map_err(|_| EngineError::CounterOverflow)?;
    engine.bump(|counters| checked_add(&mut counters.objects_validated, 1))?;
    if let Some(meta) = object_meta(engine, connection, id)? {
        if meta.canonical_len != canonical_len
            || meta.kind != object.kind()
            || meta.blob_len != canonical_len
        {
            return Err(EngineError::ImmutableConflict("object", id));
        }
        engine.mark_statement()?;
        let mut select = connection
            .prepare_cached("SELECT canonical_bytes FROM layerfs_objects WHERE object_id = ?1")
            .map_err(map_sqlite_error)?;
        let stored = select
            .query_row(params![id.as_bytes().as_slice()], |row| {
                row.get::<_, Vec<u8>>(0)
            })
            .map_err(map_sqlite_error)?;
        if stored != canonical_bytes {
            return Err(EngineError::ImmutableConflict("object", id));
        }
        validate_identity(&stored, id)
            .map_err(|cause| EngineError::MalformedObject { id, cause })?;
        engine.bump(|counters| {
            checked_add(&mut counters.objects_reused, 1)?;
            checked_add(&mut counters.object_bytes_read, canonical_len)
        })?;
        return Ok(PutOutcome::Reused);
    }

    engine.mark_statement()?;
    let mut insert = connection
        .prepare_cached(
            "INSERT INTO layerfs_objects (object_id, kind, canonical_length, canonical_bytes)
             VALUES (?1, ?2, ?3, ?4)",
        )
        .map_err(map_sqlite_error)?;
    insert
        .execute(params![
            id.as_bytes().as_slice(),
            i64::from(object.kind() as u8),
            i64::try_from(canonical_len).map_err(|_| EngineError::CounterOverflow)?,
            canonical_bytes,
        ])
        .map_err(map_sqlite_error)?;
    engine.bump(|counters| {
        checked_add(&mut counters.objects_created, 1)?;
        checked_add(&mut counters.object_bytes_written, canonical_len)?;
        checked_add(&mut counters.logical_object_bytes, canonical_len)
    })?;
    Ok(PutOutcome::Created)
}

fn read_object_range_on_connection(
    engine: &Engine,
    connection: &Connection,
    id: ObjectId,
    range: Range<u64>,
) -> EngineResult<Vec<u8>> {
    let meta = object_meta(engine, connection, id)?.ok_or(EngineError::MissingObject(id))?;
    if meta.canonical_len != meta.blob_len {
        return Err(EngineError::ShortRead {
            expected: meta.canonical_len,
            actual: meta.blob_len,
        });
    }
    if range.start > range.end || range.end > meta.canonical_len {
        return Err(EngineError::InvalidRange {
            start: range.start,
            end: range.end,
            length: meta.canonical_len,
        });
    }
    authenticate_blob(engine, connection, id, meta)?;
    let requested = range
        .end
        .checked_sub(range.start)
        .ok_or(EngineError::CounterOverflow)?;
    let requested_usize = usize::try_from(requested).map_err(|_| EngineError::CounterOverflow)?;
    let start_usize = usize::try_from(range.start).map_err(|_| EngineError::CounterOverflow)?;
    let mut output = vec![0_u8; requested_usize];
    if requested_usize != 0 {
        let blob = connection
            .blob_open(
                "main",
                OBJECTS_TABLE,
                OBJECTS_BLOB_COLUMN,
                meta.row_id,
                true,
            )
            .map_err(map_sqlite_error)?;
        match blob.read_at_exact(&mut output, start_usize) {
            Ok(()) => {}
            Err(rusqlite::Error::BlobSizeError) => {
                return Err(EngineError::ShortRead {
                    expected: requested,
                    actual: 0,
                })
            }
            Err(error) => return Err(map_sqlite_error(error)),
        }
    }
    engine.bump(|counters| {
        checked_add(&mut counters.range_bytes_requested, requested)?;
        checked_add(&mut counters.range_bytes_returned, requested)
    })?;
    Ok(output)
}

fn authenticate_blob(
    engine: &Engine,
    connection: &Connection,
    id: ObjectId,
    meta: ObjectMeta,
) -> EngineResult<ObjectSummary> {
    let blob = connection
        .blob_open(
            "main",
            OBJECTS_TABLE,
            OBJECTS_BLOB_COLUMN,
            meta.row_id,
            true,
        )
        .map_err(map_sqlite_error)?;
    let actual = ObjectId::from_reader(blob).map_err(|error| EngineError::Sqlite {
        kind: SqliteErrorKind::Io,
        message: error.to_string(),
    })?;
    if actual != id {
        return Err(EngineError::IdentityMismatch {
            expected: id,
            actual,
        });
    }
    let blob = connection
        .blob_open(
            "main",
            OBJECTS_TABLE,
            OBJECTS_BLOB_COLUMN,
            meta.row_id,
            true,
        )
        .map_err(map_sqlite_error)?;
    let summary =
        validate_object_from(blob).map_err(|cause| EngineError::MalformedObject { id, cause })?;
    if summary.kind != meta.kind || summary.canonical_len != meta.canonical_len {
        return Err(EngineError::MalformedObject {
            id,
            cause: CoreError::LengthMismatch {
                expected: meta.canonical_len,
                actual: summary.canonical_len,
            },
        });
    }
    engine.bump(|counters| {
        checked_add(&mut counters.objects_validated, 1)?;
        checked_add(&mut counters.object_bytes_read, meta.canonical_len)
    })?;
    Ok(summary)
}

fn authenticate_directory_object(
    engine: &Engine,
    connection: &Connection,
    id: ObjectId,
) -> EngineResult<()> {
    let meta = object_meta(engine, connection, id)?.ok_or(EngineError::MissingObject(id))?;
    if meta.kind != ObjectKind::Directory {
        return Err(EngineError::InvalidRecord("root directory object"));
    }
    authenticate_blob(engine, connection, id, meta).map(|_| ())
}

fn visible_root_on_connection(
    engine: &Engine,
    connection: &Connection,
) -> EngineResult<Option<RootId>> {
    engine.mark_statement()?;
    let mut statement = connection
        .prepare_cached("SELECT visible_root FROM layerfs_store_meta WHERE store_id = 1")
        .map_err(map_sqlite_error)?;
    let bytes = statement
        .query_row([], |row| row.get::<_, Option<Vec<u8>>>(0))
        .map_err(map_sqlite_error)?;
    bytes
        .map(|bytes| ObjectId::from_bytes(&bytes).map_err(EngineError::Core))
        .transpose()
}

fn load_root_on_connection(
    engine: &Engine,
    connection: &Connection,
    id: RootId,
) -> EngineResult<RootRecord> {
    engine.mark_statement()?;
    let mut statement = connection
        .prepare_cached(
            "SELECT directory_object, parent_root FROM layerfs_roots WHERE root_id = ?1",
        )
        .map_err(map_sqlite_error)?;
    let row = statement
        .query_row(params![id.as_bytes().as_slice()], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Option<Vec<u8>>>(1)?))
        })
        .optional()
        .map_err(map_sqlite_error)?
        .ok_or(EngineError::MissingRoot(id))?;
    let directory_object = ObjectId::from_bytes(&row.0).map_err(EngineError::Core)?;
    let parent = row
        .1
        .map(|bytes| ObjectId::from_bytes(&bytes).map_err(EngineError::Core))
        .transpose()?;
    Ok(RootRecord {
        id,
        directory_object,
        parent,
    })
}

fn load_delta_on_connection(
    engine: &Engine,
    connection: &Connection,
    id: ObjectId,
) -> EngineResult<DeltaRecord> {
    engine.mark_statement()?;
    let mut statement = connection
        .prepare_cached(
            "SELECT parent_root, child_root, payload FROM layerfs_deltas WHERE delta_id = ?1",
        )
        .map_err(map_sqlite_error)?;
    let row = statement
        .query_row(params![id.as_bytes().as_slice()], |row| {
            Ok((
                row.get::<_, Option<Vec<u8>>>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })
        .optional()
        .map_err(map_sqlite_error)?
        .ok_or(EngineError::MissingDelta(id))?;
    decode_delta_parts(id, row.0, row.1, row.2)
}

fn decode_delta_parts(
    id: ObjectId,
    parent: Option<Vec<u8>>,
    child: Vec<u8>,
    payload: Vec<u8>,
) -> EngineResult<DeltaRecord> {
    let parent = parent
        .map(|bytes| ObjectId::from_bytes(&bytes).map_err(EngineError::Core))
        .transpose()?;
    let child = ObjectId::from_bytes(&child).map_err(EngineError::Core)?;
    let delta = DeltaRecord {
        id,
        parent,
        child,
        payload,
    };
    delta.validate()?;
    Ok(delta)
}

fn write_root_on_connection(
    engine: &Engine,
    connection: &Connection,
    root: &RootRecord,
) -> EngineResult<()> {
    engine.mark_statement()?;
    let mut select = connection
        .prepare_cached(
            "SELECT directory_object, parent_root FROM layerfs_roots WHERE root_id = ?1",
        )
        .map_err(map_sqlite_error)?;
    let existing = select
        .query_row(params![root.id.as_bytes().as_slice()], |row| {
            Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Option<Vec<u8>>>(1)?))
        })
        .optional()
        .map_err(map_sqlite_error)?;
    if let Some((directory_object, parent)) = existing {
        let existing = RootRecord {
            id: root.id,
            directory_object: ObjectId::from_bytes(&directory_object).map_err(EngineError::Core)?,
            parent: parent
                .map(|bytes| ObjectId::from_bytes(&bytes).map_err(EngineError::Core))
                .transpose()?,
        };
        if existing != *root {
            return Err(EngineError::ImmutableConflict("root", root.id));
        }
        return Ok(());
    }
    engine.mark_statement()?;
    let mut insert = connection
        .prepare_cached(
            "INSERT INTO layerfs_roots (root_id, directory_object, parent_root)
             VALUES (?1, ?2, ?3)",
        )
        .map_err(map_sqlite_error)?;
    insert
        .execute(params![
            root.id.as_bytes().as_slice(),
            root.directory_object.as_bytes().as_slice(),
            root.parent.map(|id| id.to_bytes().to_vec()),
        ])
        .map_err(map_sqlite_error)?;
    Ok(())
}

fn root_record_len(root: &RootRecord) -> EngineResult<u64> {
    ROOT_RECORD_BASE_BYTES
        .checked_add(if root.parent.is_some() { 32 } else { 0 })
        .ok_or(EngineError::CounterOverflow)
}

fn delta_record_len(delta: &DeltaRecord) -> EngineResult<u64> {
    let payload = u64::try_from(delta.payload.len()).map_err(|_| EngineError::CounterOverflow)?;
    let parent = if delta.parent.is_some() { 32 } else { 0 };
    payload
        .checked_add(64)
        .and_then(|value| value.checked_add(parent))
        .ok_or(EngineError::CounterOverflow)
}

fn checked_add(value: &mut u64, amount: u64) -> EngineResult<()> {
    *value = value
        .checked_add(amount)
        .ok_or(EngineError::CounterOverflow)?;
    Ok(())
}

fn sqlite_error_kind(error: &rusqlite::Error) -> SqliteErrorKind {
    match error {
        rusqlite::Error::SqliteFailure(error, _) => match error.code {
            rusqlite::ErrorCode::DatabaseBusy => SqliteErrorKind::Busy,
            rusqlite::ErrorCode::DatabaseLocked => SqliteErrorKind::Locked,
            rusqlite::ErrorCode::PermissionDenied => SqliteErrorKind::PermissionDenied,
            rusqlite::ErrorCode::DiskFull => SqliteErrorKind::NoSpace,
            rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase => {
                SqliteErrorKind::Corrupt
            }
            rusqlite::ErrorCode::ReadOnly => SqliteErrorKind::ReadOnly,
            rusqlite::ErrorCode::ConstraintViolation => SqliteErrorKind::Constraint,
            rusqlite::ErrorCode::SystemIoFailure => SqliteErrorKind::Io,
            _ => SqliteErrorKind::Other,
        },
        rusqlite::Error::BlobSizeError => SqliteErrorKind::Io,
        _ => SqliteErrorKind::Other,
    }
}

fn map_sqlite_error(error: rusqlite::Error) -> EngineError {
    let kind = sqlite_error_kind(&error);
    EngineError::Sqlite {
        kind,
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use layerfs_core::{encode_object, Object};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(0);

    fn test_path() -> PathBuf {
        let id = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("layerfs-engine-{id}.sqlite"))
    }

    fn bytes_object(value: &[u8]) -> (ObjectId, Vec<u8>) {
        let bytes = encode_object(&Object::bytes(value.to_vec()).expect("test object"))
            .expect("test encoding");
        (ObjectId::for_bytes(&bytes), bytes)
    }

    fn empty_directory() -> (ObjectId, Vec<u8>) {
        let bytes = encode_object(&Object::directory(Vec::new()).expect("test directory"))
            .expect("test encoding");
        (ObjectId::for_bytes(&bytes), bytes)
    }

    fn root(number: u8, directory_object: ObjectId, parent: Option<ObjectId>) -> RootRecord {
        RootRecord {
            id: ObjectId::for_bytes(&[number]),
            directory_object,
            parent,
        }
    }

    #[test]
    fn profile_range_reopen_and_counters() {
        let path = test_path();
        let (id, bytes) = bytes_object(b"durable range payload");
        {
            let engine = Engine::open(&path).expect("open");
            assert_eq!(engine.profile().journal_mode.to_ascii_uppercase(), "DELETE");
            assert_eq!(engine.profile().synchronous, 2);
            assert_eq!(engine.profile().temp_store, 1);
            assert_eq!(engine.profile().mmap_size, 0);
            assert_eq!(
                engine.put_object_if_absent(id, &bytes),
                Ok(PutOutcome::Created)
            );
            assert_eq!(
                engine.read_object_range(id, 2..7).expect("range"),
                bytes[2..7]
            );
            assert!(matches!(
                engine.read_object_range(id, 7..2),
                Err(EngineError::InvalidRange { .. })
            ));
            assert!(matches!(
                engine.read_object_range(id, 0..bytes.len() as u64 + 1),
                Err(EngineError::InvalidRange { .. })
            ));
            assert_eq!(
                engine
                    .read_object_range(id, bytes.len() as u64..bytes.len() as u64)
                    .expect("empty"),
                Vec::<u8>::new()
            );
            let counters = engine.counters().expect("counters");
            assert!(counters.objects_validated >= 2);
            assert_eq!(counters.range_bytes_returned, 5);
        }
        {
            let engine = Engine::open(&path).expect("reopen");
            assert_eq!(
                engine.load_object(id).expect("object").canonical_bytes,
                bytes
            );
            assert!(engine.observations().logical_engine_bytes.is_some());
        }
        let _ = fs::remove_file(path);
    }

    #[test]
    fn immutable_reuse_and_tamper_are_distinct() {
        let path = test_path();
        let (id, bytes) = bytes_object(b"immutable");
        let engine = Engine::open(&path).expect("open");
        assert_eq!(
            engine.put_object_if_absent(id, &bytes),
            Ok(PutOutcome::Created)
        );
        assert_eq!(
            engine.put_object_if_absent(id, &bytes),
            Ok(PutOutcome::Reused)
        );
        let changed = bytes_object(b"different").1;
        assert!(matches!(
            engine.put_object_if_absent(id, &changed),
            Err(EngineError::MalformedObject { .. })
        ));
        drop(engine);
        let connection = Connection::open(&path).expect("tamper connection");
        connection
            .execute(
                "UPDATE layerfs_objects SET canonical_bytes = ?1 WHERE object_id = ?2",
                params![vec![1_u8, 2, 3], id.as_bytes().as_slice()],
            )
            .expect("tamper");
        drop(connection);
        let engine = Engine::open(&path).expect("reopen");
        assert!(matches!(
            engine.read_object_range(id, 0..1),
            Err(EngineError::ShortRead { .. })
                | Err(EngineError::IdentityMismatch { .. })
                | Err(EngineError::MalformedObject { .. })
        ));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn capture_is_atomic_and_durable() {
        let path = test_path();
        let (directory_id, directory_bytes) = empty_directory();
        let base_root = root(1, directory_id, None);
        let child_root = root(2, directory_id, Some(base_root.id));
        let base_delta = DeltaRecord::new(None, base_root.id, b"base".to_vec());
        let child_delta = DeltaRecord::new(Some(base_root.id), child_root.id, b"child".to_vec());
        {
            let engine = Engine::open(&path).expect("open");
            let mut capture = engine.begin_capture(None).expect("base capture");
            capture
                .put_object_if_absent(directory_id, &directory_bytes)
                .expect("directory");
            capture.write_delta(&base_delta).expect("base delta");
            capture.commit_root(base_root.clone()).expect("base root");
            assert_eq!(
                engine.load_visible_root().expect("visible"),
                Some(base_root.id)
            );

            let (child_id, child_bytes) = bytes_object(b"child object");
            let mut capture = engine
                .begin_capture(Some(base_root.id))
                .expect("child capture");
            capture
                .put_object_if_absent(child_id, &child_bytes)
                .expect("child object");
            capture.write_delta(&child_delta).expect("child delta");
            capture.fail_before_visible_root();
            assert!(matches!(
                capture.commit_root(child_root.clone()),
                Err(EngineError::InjectedFailure(_))
            ));
            assert_eq!(
                engine.load_visible_root().expect("old visible"),
                Some(base_root.id)
            );
            assert!(matches!(
                engine.load_root(child_root.id),
                Err(EngineError::MissingRoot(_))
            ));

            let mut capture = engine
                .begin_capture(Some(base_root.id))
                .expect("retry capture");
            capture
                .put_object_if_absent(child_id, &child_bytes)
                .expect("child object retry");
            capture
                .write_delta(&child_delta)
                .expect("child delta retry");
            capture.commit_root(child_root.clone()).expect("child root");
        }
        {
            let engine = Engine::open(&path).expect("reopen");
            assert_eq!(
                engine.load_visible_root().expect("visible"),
                Some(child_root.id)
            );
            assert_eq!(engine.load_root(child_root.id).expect("root"), child_root);
            assert_eq!(
                engine.load_delta(child_delta.id).expect("delta"),
                child_delta
            );
        }
        let _ = fs::remove_file(path);
    }

    #[test]
    fn sqlite_error_mapping_preserves_busy_and_no_space() {
        let busy = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ErrorCode::DatabaseBusy,
                extended_code: 5,
            },
            None,
        );
        assert!(matches!(
            map_sqlite_error(busy),
            EngineError::Sqlite {
                kind: SqliteErrorKind::Busy,
                ..
            }
        ));
        let full = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error {
                code: rusqlite::ErrorCode::DiskFull,
                extended_code: 13,
            },
            None,
        );
        assert!(matches!(
            map_sqlite_error(full),
            EngineError::Sqlite {
                kind: SqliteErrorKind::NoSpace,
                ..
            }
        ));
    }
}
