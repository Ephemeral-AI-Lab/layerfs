//! Minimal Phase 4A durable LayerFS engine.

#![forbid(unsafe_code)]

use layerfs_core::content::rope::ObjectRead;
use layerfs_core::{
    authenticate_identity, validate_bytes_identity, validate_identity, validate_object_from,
    CoreError, ObjectId, ObjectKind, ObjectSummary,
};
use rusqlite::types::ValueRef;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Cursor;
use std::ops::Range;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

// ponytail: serialize Verified admission in-process; use per-Store locks only if
// parallel open throughput becomes a measured requirement.
static VERIFIED_OPEN_LOCK: Mutex<()> = Mutex::new(());

pub mod generation;
pub mod integrity;
pub mod publication;
pub mod refs;
pub mod scratch;

pub const COMPONENT: &str = "layerfs-engine";
const FORMAT_MARKER: &str = "layerfs-phase4a-sqlite-blob";
const SCHEMA_VERSION: i64 = 1;
const BUSY_TIMEOUT: Duration = Duration::ZERO;
#[cfg(test)]
const ROOT_RECORD_BASE_BYTES: u64 = 64;

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
    PublicationConflict,
    AmbiguousDurability,
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
            Self::PublicationConflict => {
                formatter.write_str("publication expected ref does not match")
            }
            Self::AmbiguousDurability => formatter.write_str("publication outcome is ambiguous"),
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

    pub(crate) fn validate(&self) -> EngineResult<()> {
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
    pub page_size: i64,
    pub cache_pages: i64,
    pub cache_spill_pages: i64,
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
    pub retained_union_scrubs: u64,
    pub root_verifications: u64,
    pub root_verification_objects: u64,
    pub root_verification_bytes: u64,
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
    pub publication_commits: u64,
    pub publication_closure_passes: u64,
    pub namespace_graph_verification_passes: u64,
    pub scratch_tables: u64,
    pub scratch_statements: u64,
    pub scratch_rows: u64,
    pub scratch_high_water_bytes: u64,
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

pub type EngineResult<T> = Result<T, EngineError>;

pub struct Engine {
    path: PathBuf,
    connection: Mutex<Option<Connection>>,
    counters: Mutex<EngineCounters>,
    rollback_journal_sample: Mutex<Option<u64>>,
    profile: SqliteProfile,
    mode: integrity::IntegrityMode,
    maintenance_pin: Option<Mutex<Connection>>,
    last_compaction: Option<CompactionStorageObservation>,
    commit_dispatch: std::sync::Arc<dyn CommitDispatch>,
}

trait CommitDispatch: Send + Sync {
    fn commit(&self, connection: &Connection) -> rusqlite::Result<()>;
}

struct SqliteCommit;
impl CommitDispatch for SqliteCommit {
    fn commit(&self, connection: &Connection) -> rusqlite::Result<()> {
        connection.execute_batch("COMMIT")
    }
}

pub(crate) struct ConnectionGuard<'a> {
    guard: MutexGuard<'a, Option<Connection>>,
    transaction: bool,
    commit_scrub_on_drop: bool,
}

impl Deref for ConnectionGuard<'_> {
    type Target = Connection;
    fn deref(&self) -> &Self::Target {
        self.guard.as_ref().expect("checked when locked")
    }
}

impl DerefMut for ConnectionGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.guard.as_mut().expect("checked when locked")
    }
}

impl Drop for ConnectionGuard<'_> {
    fn drop(&mut self) {
        if self.transaction {
            if let Some(connection) = self.guard.as_ref() {
                let result = connection.execute_batch(if self.commit_scrub_on_drop {
                    "COMMIT"
                } else {
                    "ROLLBACK"
                });
                if result.is_err() {
                    self.guard.take();
                }
            }
            self.transaction = false;
        }
    }
}

impl ObjectRead for Engine {
    fn get(&self, id: ObjectId) -> Result<Vec<u8>, CoreError> {
        self.load_object(id)
            .map(|object| object.canonical_bytes)
            .map_err(core_store_error)
    }

    fn get_authenticated_batch<F>(&self, ids: &[ObjectId], mut callback: F) -> Result<(), CoreError>
    where
        F: FnMut(ObjectId, &[u8]) -> Result<(), CoreError>,
    {
        self.for_each_authenticated_payload_batch(ids, |id, bytes| {
            callback(id, bytes).map_err(EngineError::Core)
        })
        .map_err(core_store_error)
    }

    fn with_authenticated_canonical<T, F>(&self, id: ObjectId, callback: F) -> Result<T, CoreError>
    where
        F: FnOnce(&[u8]) -> Result<T, CoreError>,
    {
        let connection = self.lock_connection().map_err(core_store_error)?;
        with_authenticated_canonical_on_connection(self, &connection, id, true, true, |_, bytes| {
            callback(bytes).map_err(EngineError::Core)
        })
        .map_err(core_store_error)
    }
}

fn core_store_error(error: EngineError) -> CoreError {
    match error {
        EngineError::Core(error) | EngineError::MalformedObject { cause: error, .. } => error,
        EngineError::MissingObject(_) => CoreError::MissingObject,
        EngineError::IdentityMismatch { .. } | EngineError::ImmutableConflict(_, _) => {
            CoreError::IdentityMismatch
        }
        EngineError::CounterOverflow => CoreError::LengthOverflow,
        _ => CoreError::Io,
    }
}

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
        preflight_schema(&connection).map_err(|error| engine_step("primary preflight", error))?;
        let profile =
            configure_profile(&connection).map_err(|error| engine_step("profile", error))?;
        initialize_schema(&connection, &profile)
            .map_err(|error| engine_step("schema initialization", error))?;
        if mode == integrity::IntegrityMode::Verified {
            let _admission = VERIFIED_OPEN_LOCK.lock().map_err(|_| EngineError::Sqlite {
                kind: SqliteErrorKind::Other,
                message: "Verified admission mutex poisoned".to_owned(),
            })?;
            initial_verified_scrub(&connection, &path)
                .map_err(|error| engine_step("initial verified scrub", error))?;
        }
        Ok(Self {
            path,
            connection: Mutex::new(Some(connection)),
            counters: Mutex::new(EngineCounters::default()),
            rollback_journal_sample: Mutex::new(None),
            profile,
            mode,
            maintenance_pin: None,
            last_compaction: None,
            commit_dispatch: std::sync::Arc::new(SqliteCommit),
        })
    }

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
        let connection = self.lock_connection()?;
        let bytes = connection
            .query_row(
                "SELECT store_id FROM layerfs_authority WHERE authority_id = 1",
                [],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .map_err(map_sqlite_error)?;
        bytes
            .try_into()
            .map_err(|_| EngineError::InvalidRecord("StoreId"))
    }

    pub fn active_connection_count(&self) -> EngineResult<u64> {
        let primary = u64::from(
            self.connection
                .lock()
                .map_err(|_| EngineError::InvalidRecord("connection lock"))?
                .is_some(),
        );
        Ok(primary + u64::from(self.maintenance_pin.is_some()))
    }

    pub fn compact_to(&self, destination: &Path) -> EngineResult<()> {
        self.compact_to_observed(destination).map(drop)
    }

    pub(crate) fn compact_to_observed(
        &self,
        destination: &Path,
    ) -> EngineResult<CompactionStorageObservation> {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .map_err(io_engine_error)?;
        let result = self.compact_to_created(destination);
        if result.is_err() {
            let _ = fs::remove_file(destination);
            let mut journal = destination.as_os_str().to_os_string();
            journal.push("-journal");
            let _ = fs::remove_file(PathBuf::from(journal));
        }
        result
    }

    fn compact_to_created(&self, destination: &Path) -> EngineResult<CompactionStorageObservation> {
        let old_generation_bytes = fs::metadata(&self.path).map_err(io_engine_error)?.len();
        let source = self.lock_connection()?;
        let retained = integrity::retained_union(&source, &self.path)?;
        let mark_database_bytes = retained.work.storage_bytes()?;
        let candidate = Connection::open(destination).map_err(map_sqlite_error)?;
        candidate
            .busy_timeout(BUSY_TIMEOUT)
            .map_err(map_sqlite_error)?;
        let profile = configure_profile(&candidate)?;
        initialize_schema(&candidate, &profile)?;
        let source_path = self
            .path
            .to_str()
            .ok_or(EngineError::InvalidRecord("non-UTF-8 Store path"))?;
        candidate
            .execute("ATTACH DATABASE ?1 AS source", params![source_path])
            .map_err(map_sqlite_error)?;
        candidate.execute_batch("BEGIN").map_err(map_sqlite_error)?;
        let copied = (|| {
            candidate
                .execute_batch(
                    "DELETE FROM layerfs_store_meta;
                 DELETE FROM layerfs_authority;
                 INSERT INTO layerfs_store_meta SELECT * FROM source.layerfs_store_meta;
                 UPDATE layerfs_store_meta SET visible_root = NULL;
                 INSERT INTO layerfs_authority SELECT * FROM source.layerfs_authority;
                 INSERT INTO layerfs_refs SELECT * FROM source.layerfs_refs;
                 INSERT INTO layerfs_retained_roots SELECT * FROM source.layerfs_retained_roots;",
                )
                .map_err(map_sqlite_error)?;
            let mut select = source
                .prepare(
                    "SELECT kind, canonical_length, canonical_bytes FROM layerfs_objects WHERE object_id = ?1",
                )
                .map_err(map_sqlite_error)?;
            let mut insert = candidate
                .prepare(
                    "INSERT INTO layerfs_objects (object_id, kind, canonical_length, canonical_bytes) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(object_id) DO NOTHING",
                )
                .map_err(map_sqlite_error)?;
            retained.work.for_each_key(|key| {
                if key.len() != 34 {
                    return Err(EngineError::InvalidRecord("closure key"));
                }
                let id = &key[..32];
                let row = select
                    .query_row(params![id], |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, Vec<u8>>(2)?,
                        ))
                    })
                    .map_err(map_sqlite_error)?;
                insert
                    .execute(params![id, row.0, row.1, row.2])
                    .map_err(map_sqlite_error)?;
                Ok(())
            })
        })();
        if let Err(error) = copied {
            let _ = candidate.execute_batch("ROLLBACK");
            return Err(error);
        }
        let candidate_journal_temp_peak_bytes = candidate_auxiliary_bytes(destination);
        candidate
            .execute_batch("COMMIT")
            .map_err(map_sqlite_error)?;
        let verification_scratch_peak_bytes =
            integrity::verify_retained_union_observed(&candidate, destination)?;
        candidate
            .execute_batch("DETACH DATABASE source")
            .map_err(map_sqlite_error)?;
        drop(candidate);
        fs::File::open(destination)
            .and_then(|file| file.sync_all())
            .map_err(io_engine_error)?;
        let new_generation_bytes = fs::metadata(destination).map_err(io_engine_error)?.len();
        let selector_temporary_bytes = generation::SELECTOR_BYTES as u64;
        let total_peak_bytes = old_generation_bytes
            .checked_add(new_generation_bytes)
            .and_then(|value| value.checked_add(mark_database_bytes))
            .and_then(|value| value.checked_add(candidate_journal_temp_peak_bytes))
            .and_then(|value| value.checked_add(verification_scratch_peak_bytes))
            .and_then(|value| value.checked_add(selector_temporary_bytes))
            .ok_or(EngineError::CounterOverflow)?;
        Ok(CompactionStorageObservation {
            old_generation_bytes,
            new_generation_bytes,
            mark_database_bytes,
            candidate_journal_temp_peak_bytes,
            verification_scratch_peak_bytes,
            selector_temporary_bytes,
            total_peak_bytes,
        })
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
        let connection = connection.as_ref()?;
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
        let connection = self.lock_connection()?;
        with_authenticated_canonical_on_connection(
            self,
            &connection,
            id,
            false,
            false,
            |kind, bytes| {
                Ok(ObjectRecord {
                    id,
                    kind,
                    canonical_len: bytes.len() as u64,
                    canonical_bytes: bytes.to_vec(),
                })
            },
        )
    }

    pub fn object_length(&self, id: ObjectId) -> EngineResult<u64> {
        let connection = self.lock_connection()?;
        with_authenticated_canonical_on_connection(
            self,
            &connection,
            id,
            false,
            false,
            |_, bytes| Ok(bytes.len() as u64),
        )
    }

    pub fn read_object_range(&self, id: ObjectId, range: Range<u64>) -> EngineResult<Vec<u8>> {
        let connection = self.lock_connection()?;
        with_authenticated_canonical_on_connection(
            self,
            &connection,
            id,
            false,
            false,
            |_, bytes| {
                let length = bytes.len() as u64;
                if range.start > range.end || range.end > length {
                    return Err(EngineError::InvalidRange {
                        start: range.start,
                        end: range.end,
                        length,
                    });
                }
                let start =
                    usize::try_from(range.start).map_err(|_| EngineError::CounterOverflow)?;
                let end = usize::try_from(range.end).map_err(|_| EngineError::CounterOverflow)?;
                let output = bytes[start..end].to_vec();
                let requested = range.end - range.start;
                self.bump(|counters| {
                    checked_add(&mut counters.range_bytes_requested, requested)?;
                    checked_add(&mut counters.range_bytes_returned, requested)
                })?;
                Ok(output)
            },
        )
    }

    pub fn for_each_authenticated_payload_batch<F>(
        &self,
        ids: &[ObjectId],
        mut callback: F,
    ) -> EngineResult<()>
    where
        F: FnMut(ObjectId, &[u8]) -> EngineResult<()>,
    {
        if ids.len() > 64 {
            return Err(EngineError::InvalidRecord("payload batch exceeds 64"));
        }
        if ids.is_empty() {
            return Ok(());
        }
        self.bump(|counters| {
            checked_add(&mut counters.payload_batch_queries, 1)?;
            checked_add(
                &mut counters.payload_batch_references,
                u64::try_from(ids.len()).map_err(|_| EngineError::CounterOverflow)?,
            )?;
            counters.payload_batch_maximum = counters
                .payload_batch_maximum
                .max(u64::try_from(ids.len()).map_err(|_| EngineError::CounterOverflow)?);
            Ok(())
        })?;
        let connection = self.lock_connection()?;
        let sql = payload_batch_sql(ids.len())?;
        self.mark_statement()?;
        let mut statement = connection.prepare_cached(&sql).map_err(map_sqlite_error)?;
        let mut rows = statement
            .query(rusqlite::params_from_iter(
                ids.iter().map(|id| id.as_bytes().as_slice()),
            ))
            .map_err(map_sqlite_error)?;
        let mut ordinal = 0;
        while let Some(row) = rows.next().map_err(map_sqlite_error)? {
            let observed_ordinal = row.get::<_, i64>(0).map_err(map_sqlite_error)?;
            if observed_ordinal != ordinal as i64 {
                return if observed_ordinal > ordinal as i64 {
                    Err(EngineError::MissingObject(ids[ordinal]))
                } else {
                    Err(EngineError::InvalidRecord("payload batch order"))
                };
            }
            let id = ids[ordinal];
            let kind = row
                .get::<_, Option<i64>>(1)
                .map_err(map_sqlite_error)?
                .ok_or(EngineError::MissingObject(id))?;
            let length = row
                .get::<_, Option<i64>>(2)
                .map_err(map_sqlite_error)?
                .ok_or(EngineError::MissingObject(id))?;
            let bytes = match row.get_ref(3).map_err(map_sqlite_error)? {
                ValueRef::Blob(bytes) => bytes,
                _ => return Err(EngineError::InvalidRecord("object bytes")),
            };
            let (payload, actual_length) = validate_payload_borrowed(id, kind, length, bytes)?;
            self.bump(|counters| {
                checked_add(&mut counters.objects_validated, 1)?;
                checked_add(&mut counters.object_bytes_read, actual_length)?;
                checked_add(&mut counters.fetched_rows, 1)?;
                checked_add(&mut counters.fetched_row_authentication_passes, 1)?;
                checked_add(&mut counters.fetched_row_role_decode_passes, 1)
            })?;
            callback(id, payload)?;
            ordinal += 1;
        }
        if ordinal != ids.len() {
            return Err(EngineError::MissingObject(ids[ordinal]));
        }
        Ok(())
    }

    #[cfg(test)]
    fn put_object_if_absent(
        &self,
        id: ObjectId,
        canonical_bytes: &[u8],
    ) -> EngineResult<PutOutcome> {
        let mut connection = self.lock_write_connection()?;
        let outcome = put_object_on_connection(self, &connection, id, canonical_bytes)?;
        if connection.transaction {
            connection
                .execute_batch("COMMIT")
                .map_err(map_sqlite_error)?;
            connection.transaction = false;
        }
        Ok(outcome)
    }

    #[cfg(test)]
    fn begin_capture(&self, parent: Option<RootId>) -> EngineResult<Capture<'_>> {
        let mut connection = self.lock_write_connection()?;
        self.mark_statement()?;
        if !connection.transaction {
            if let Err(error) = connection.execute_batch("BEGIN IMMEDIATE") {
                self.note_sqlite_error(&error)?;
                return Err(map_sqlite_error(error));
            }
            connection.transaction = true;
        }
        self.bump(|counters| checked_add(&mut counters.transactions_started, 1))?;

        let current = match visible_root_on_connection(self, &connection) {
            Ok(current) => current,
            Err(error) => {
                let _ = connection.execute_batch("ROLLBACK");
                connection.transaction = false;
                self.bump_best_effort(|counters| {
                    checked_add(&mut counters.transactions_rolled_back, 1)
                });
                return Err(error);
            }
        };
        if current != parent {
            let _ = connection.execute_batch("ROLLBACK");
            connection.transaction = false;
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
                    connection.transaction = false;
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
                connection.transaction = false;
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

    fn lock_connection(&self) -> EngineResult<ConnectionGuard<'_>> {
        self.lock_connection_mode(false)
    }

    fn lock_write_connection(&self) -> EngineResult<ConnectionGuard<'_>> {
        self.lock_connection_mode(true)
    }

    fn lock_connection_mode(&self, write: bool) -> EngineResult<ConnectionGuard<'_>> {
        let connection = self.connection.lock().map_err(|_| EngineError::Sqlite {
            kind: SqliteErrorKind::Other,
            message: "connection mutex poisoned".to_owned(),
        })?;
        if connection.is_none() {
            return Err(EngineError::AmbiguousDurability);
        }
        let mut connection = ConnectionGuard {
            guard: connection,
            transaction: false,
            commit_scrub_on_drop: false,
        };
        if self.mode == integrity::IntegrityMode::Verified {
            connection
                .execute_batch(if write { "BEGIN IMMEDIATE" } else { "BEGIN" })
                .map_err(map_sqlite_error)?;
            connection.transaction = true;
            if trusted_history(&connection)? {
                if !write {
                    connection
                        .execute_batch("ROLLBACK; BEGIN IMMEDIATE")
                        .map_err(map_sqlite_error)?;
                }
                self.bump(|counters| checked_add(&mut counters.retained_union_scrubs, 1))?;
                if let Err(error) = integrity::verify_retained_union(&connection, &self.path)
                    .and_then(|_| clear_trusted_history(&connection))
                {
                    let _ = connection.execute_batch("ROLLBACK");
                    connection.transaction = false;
                    return Err(error);
                }
                connection.commit_scrub_on_drop = true;
            }
        }
        Ok(connection)
    }

    fn mark_statement(&self) -> EngineResult<()> {
        self.bump(|counters| checked_add(&mut counters.statements, 1))
    }

    #[cfg(test)]
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

fn engine_step(step: &'static str, error: EngineError) -> EngineError {
    match error {
        EngineError::Sqlite { kind, message } => EngineError::Sqlite {
            kind,
            message: format!("{step}: {message}"),
        },
        error => error,
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
    path: &Path,
    name: &str,
    expected_store_id: [u8; 32],
) -> EngineResult<Option<refs::RefState>> {
    let (connection, store_id) = open_store_readonly(path)?;
    if store_id != expected_store_id {
        return Err(EngineError::InvalidRecord("reconciliation StoreId"));
    }
    refs::read_ref_on_connection(&connection, name)
}

fn reopen_store_primary(
    path: &Path,
    expected_store_id: [u8; 32],
    ref_name: &str,
    expected_ref: &Option<refs::RefState>,
) -> EngineResult<Connection> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(map_sqlite_error)?;
    connection
        .busy_timeout(BUSY_TIMEOUT)
        .map_err(map_sqlite_error)?;
    preflight_schema(&connection)?;
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
    if &refs::read_ref_on_connection(&connection, ref_name)? != expected_ref {
        return Err(EngineError::AmbiguousDurability);
    }
    configure_profile(&connection)?;
    Ok(connection)
}

fn io_engine_error(error: std::io::Error) -> EngineError {
    EngineError::Sqlite {
        kind: SqliteErrorKind::Io,
        message: error.to_string(),
    }
}

fn candidate_auxiliary_bytes(path: &Path) -> u64 {
    ["-journal", "-wal", "-shm"]
        .into_iter()
        .filter_map(|suffix| {
            let mut name = path.as_os_str().to_os_string();
            name.push(suffix);
            fs::metadata(PathBuf::from(name))
                .ok()
                .map(|value| value.len())
        })
        .sum()
}

fn clear_trusted_history(connection: &Connection) -> EngineResult<()> {
    if !trusted_history(connection)? {
        return Ok(());
    }
    connection
        .execute(
            "UPDATE layerfs_authority SET trusted_history = 0 WHERE authority_id = 1",
            [],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

fn initial_verified_scrub(connection: &Connection, path: &Path) -> EngineResult<()> {
    connection
        .execute_batch("BEGIN IMMEDIATE")
        .map_err(map_sqlite_error)?;
    let result = trusted_history(connection).and_then(|dirty| {
        if dirty {
            integrity::verify_retained_union(connection, path)
                .and_then(|_| clear_trusted_history(connection))
        } else {
            Ok(())
        }
    });
    match result {
        Ok(()) => connection.execute_batch("COMMIT").map_err(map_sqlite_error),
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

fn trusted_history(connection: &Connection) -> EngineResult<bool> {
    connection
        .query_row(
            "SELECT trusted_history FROM layerfs_authority WHERE authority_id = 1",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(map_sqlite_error)
}

pub type RootId = ObjectId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PutOutcome {
    Created,
    Reused,
}

#[cfg(test)]
struct Capture<'a> {
    engine: &'a Engine,
    connection: ConnectionGuard<'a>,
    parent: Option<RootId>,
    delta: Option<DeltaRecord>,
    active: bool,
    #[cfg(test)]
    fault: Option<FaultPoint>,
}

#[cfg(test)]
impl<'a> Capture<'a> {
    fn put_object_if_absent(
        &mut self,
        id: ObjectId,
        canonical_bytes: &[u8],
    ) -> EngineResult<PutOutcome> {
        self.ensure_active()?;
        put_object_on_connection(self.engine, &self.connection, id, canonical_bytes)
    }

    fn write_delta(&mut self, delta: &DeltaRecord) -> EngineResult<()> {
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

    fn commit_root(mut self, root: RootRecord) -> EngineResult<()> {
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
        drop(update);
        self.engine.sample_rollback_journal();
        self.engine.mark_statement()?;
        self.connection
            .execute_batch("COMMIT")
            .map_err(map_sqlite_error)?;
        self.active = false;
        self.connection.transaction = false;
        self.engine.bump(|counters| {
            checked_add(&mut counters.transactions_committed, 1)?;
            checked_add(&mut counters.logical_root_bytes, root_record_len(&root)?)
        })?;
        Ok(())
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

#[cfg(test)]
impl Drop for Capture<'_> {
    fn drop(&mut self) {
        if self.active && self.connection.execute_batch("ROLLBACK").is_ok() {
            self.active = false;
            self.connection.transaction = false;
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

const TABLE_NAMES: [&str; 7] = [
    "layerfs_authority",
    "layerfs_deltas",
    "layerfs_objects",
    "layerfs_refs",
    "layerfs_retained_roots",
    "layerfs_roots",
    "layerfs_store_meta",
];
const TABLE_SCHEMAS: [(&str, &str); 7] = [
    (
        "layerfs_store_meta",
        "CREATE TABLE IF NOT EXISTS layerfs_store_meta (
            store_id INTEGER PRIMARY KEY CHECK (store_id = 1),
            format_marker TEXT NOT NULL,
            schema_version INTEGER NOT NULL,
            journal_mode TEXT NOT NULL,
            synchronous INTEGER NOT NULL,
            temp_store INTEGER NOT NULL,
            mmap_size INTEGER NOT NULL,
            visible_root BLOB
        )",
    ),
    (
        "layerfs_objects",
        "CREATE TABLE IF NOT EXISTS layerfs_objects (
            rowid INTEGER PRIMARY KEY,
            object_id BLOB NOT NULL UNIQUE,
            kind INTEGER NOT NULL,
            canonical_length INTEGER NOT NULL,
            canonical_bytes BLOB NOT NULL
        )",
    ),
    (
        "layerfs_roots",
        "CREATE TABLE IF NOT EXISTS layerfs_roots (
            root_id BLOB PRIMARY KEY,
            directory_object BLOB NOT NULL,
            parent_root BLOB
        )",
    ),
    (
        "layerfs_deltas",
        "CREATE TABLE IF NOT EXISTS layerfs_deltas (
            delta_id BLOB PRIMARY KEY,
            parent_root BLOB,
            child_root BLOB NOT NULL,
            payload BLOB NOT NULL
        )",
    ),
    (
        "layerfs_authority",
        "CREATE TABLE IF NOT EXISTS layerfs_authority (
            authority_id INTEGER PRIMARY KEY CHECK (authority_id = 1),
            store_id BLOB NOT NULL CHECK (length(store_id) = 32),
            next_inode_serial INTEGER NOT NULL,
            trusted_history INTEGER NOT NULL CHECK (trusted_history IN (0, 1))
        )",
    ),
    (
        "layerfs_refs",
        "CREATE TABLE IF NOT EXISTS layerfs_refs (
            name TEXT PRIMARY KEY,
            generation INTEGER NOT NULL,
            root_id BLOB NOT NULL CHECK (length(root_id) = 32)
        )",
    ),
    (
        "layerfs_retained_roots",
        "CREATE TABLE IF NOT EXISTS layerfs_retained_roots (
            root_id BLOB PRIMARY KEY CHECK (length(root_id) = 32)
        )",
    ),
];

fn preflight_schema(connection: &Connection) -> EngineResult<()> {
    let mut statement = connection
        .prepare(
            "SELECT type, name FROM sqlite_schema
             WHERE name NOT GLOB 'sqlite_*'
             ORDER BY type, name",
        )
        .map_err(map_sqlite_error)?;
    let mut rows = statement.query([]).map_err(map_sqlite_error)?;
    let Some(first) = rows.next().map_err(map_sqlite_error)? else {
        return Ok(());
    };
    if first.get::<_, String>(0).map_err(map_sqlite_error)? != "table"
        || first.get::<_, String>(1).map_err(map_sqlite_error)? != TABLE_NAMES[0]
    {
        return Err(EngineError::SchemaMismatch);
    }
    for expected_name in TABLE_NAMES.into_iter().skip(1) {
        let row = rows
            .next()
            .map_err(map_sqlite_error)?
            .ok_or(EngineError::SchemaMismatch)?;
        if row.get::<_, String>(0).map_err(map_sqlite_error)? != "table"
            || row.get::<_, String>(1).map_err(map_sqlite_error)? != expected_name
        {
            return Err(EngineError::SchemaMismatch);
        }
    }
    if rows.next().map_err(map_sqlite_error)?.is_some() {
        return Err(EngineError::SchemaMismatch);
    }
    for (name, expected) in TABLE_SCHEMAS {
        let actual = connection
            .query_row(
                "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = ?1",
                params![name],
                |row| row.get::<_, String>(0),
            )
            .map_err(|_| EngineError::SchemaMismatch)?;
        if schema_shape(&actual) != schema_shape(expected) {
            return Err(EngineError::SchemaMismatch);
        }
    }
    let metadata = connection
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
        .map_err(|_| EngineError::SchemaMismatch)?;
    if !matches!(metadata, Some((ref marker, SCHEMA_VERSION, ref journal, 2, 1, 0))
        if marker == FORMAT_MARKER && journal.eq_ignore_ascii_case("DELETE"))
    {
        return Err(EngineError::SchemaMismatch);
    }
    let authority = connection
        .query_row(
            "SELECT length(store_id), next_inode_serial, trusted_history
             FROM layerfs_authority WHERE authority_id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|_| EngineError::SchemaMismatch)?;
    if !matches!(authority, Some((32, serial, trusted)) if serial >= 0 && matches!(trusted, 0 | 1))
    {
        return Err(EngineError::SchemaMismatch);
    }
    Ok(())
}

fn schema_shape(sql: &str) -> String {
    sql.chars()
        .filter(|character| !character.is_ascii_whitespace())
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .replace("ifnotexists", "")
}

fn configure_profile(connection: &Connection) -> EngineResult<SqliteProfile> {
    let journal_mode = connection
        .query_row("PRAGMA journal_mode=DELETE", [], |row| {
            row.get::<_, String>(0)
        })
        .map_err(map_sqlite_error)?;
    connection
        .execute_batch(
            "PRAGMA synchronous=FULL; PRAGMA temp_store=FILE; PRAGMA mmap_size=0; PRAGMA cache_size=1280; PRAGMA cache_spill=1280;",
        )
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
    let page_size = connection
        .query_row("PRAGMA page_size", [], |row| row.get::<_, i64>(0))
        .map_err(map_sqlite_error)?;
    let cache_pages = connection
        .query_row("PRAGMA cache_size", [], |row| row.get::<_, i64>(0))
        .map_err(map_sqlite_error)?;
    let cache_spill_pages = connection
        .query_row("PRAGMA cache_spill", [], |row| row.get::<_, i64>(0))
        .map_err(map_sqlite_error)?;
    let profile = SqliteProfile {
        journal_mode,
        synchronous,
        temp_store,
        mmap_size,
        page_size,
        cache_pages,
        cache_spill_pages,
    };
    if !profile.journal_mode.eq_ignore_ascii_case("DELETE")
        || profile.synchronous != 2
        || profile.temp_store != 1
        || profile.mmap_size != 0
        || profile.page_size != 4096
        || profile.cache_pages != 1280
        || profile.cache_spill_pages != 1280
    {
        return Err(EngineError::ProfileMismatch);
    }
    Ok(profile)
}

fn initialize_schema(connection: &Connection, profile: &SqliteProfile) -> EngineResult<()> {
    for (_, schema) in TABLE_SCHEMAS {
        connection.execute_batch(schema).map_err(map_sqlite_error)?;
    }
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
    let authority_exists = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM layerfs_authority WHERE authority_id = 1)",
            [],
            |row| row.get::<_, bool>(0),
        )
        .map_err(map_sqlite_error)?;
    if !authority_exists {
        let store_id = connection
            .query_row("SELECT randomblob(32)", [], |row| row.get::<_, Vec<u8>>(0))
            .map_err(map_sqlite_error)?;
        connection.execute("INSERT INTO layerfs_authority (authority_id, store_id, next_inode_serial, trusted_history) VALUES (1, ?1, 0, 0)", params![store_id.as_slice()]).map_err(map_sqlite_error)?;
    }
    Ok(())
}

fn with_authenticated_canonical_on_connection<T>(
    engine: &Engine,
    connection: &Connection,
    id: ObjectId,
    fetched_row: bool,
    role_decode: bool,
    callback: impl FnOnce(ObjectKind, &[u8]) -> EngineResult<T>,
) -> EngineResult<T> {
    if fetched_row != role_decode {
        return Err(EngineError::InvalidRecord("fetched role accounting"));
    }
    engine.mark_statement()?;
    let mut statement = connection
        .prepare_cached("SELECT kind, canonical_length, canonical_bytes FROM layerfs_objects WHERE object_id = ?1")
        .map_err(map_sqlite_error)?;
    let mut rows = statement
        .query(params![id.as_bytes().as_slice()])
        .map_err(map_sqlite_error)?;
    let row = rows
        .next()
        .map_err(map_sqlite_error)?
        .ok_or(EngineError::MissingObject(id))?;
    let kind = row.get::<_, i64>(0).map_err(map_sqlite_error)?;
    let length = row.get::<_, i64>(1).map_err(map_sqlite_error)?;
    let bytes = match row.get_ref(2).map_err(map_sqlite_error)? {
        ValueRef::Blob(bytes) => bytes,
        _ => return Err(EngineError::InvalidRecord("object bytes")),
    };
    let kind = authenticate_borrowed(engine, id, kind, length, bytes)?;
    if !role_decode {
        let summary = validate_object_from(Cursor::new(bytes))?;
        if summary.kind != kind || summary.canonical_len != bytes.len() as u64 {
            return Err(EngineError::InvalidRecord("object summary"));
        }
    }
    let value = callback(kind, bytes)?;
    if fetched_row {
        engine.bump(|counters| {
            checked_add(&mut counters.fetched_rows, 1)?;
            checked_add(&mut counters.fetched_row_authentication_passes, 1)?;
            checked_add(&mut counters.fetched_row_role_decode_passes, 1)
        })?;
    }
    Ok(value)
}

fn authenticate_borrowed(
    engine: &Engine,
    id: ObjectId,
    kind: i64,
    length: i64,
    bytes: &[u8],
) -> EngineResult<ObjectKind> {
    let (summary, actual_length) = authenticate_borrowed_unaccounted(id, kind, length, bytes)?;
    engine.bump(|counters| {
        checked_add(&mut counters.objects_validated, 1)?;
        checked_add(&mut counters.object_bytes_read, actual_length)?;
        Ok(())
    })?;
    Ok(summary.kind)
}

fn validate_payload_borrowed(
    id: ObjectId,
    kind: i64,
    length: i64,
    bytes: &[u8],
) -> EngineResult<(&[u8], u64)> {
    let expected_kind = ObjectKind::try_from(
        u8::try_from(kind).map_err(|_| EngineError::InvalidRecord("object kind"))?,
    )?;
    let expected_length =
        u64::try_from(length).map_err(|_| EngineError::InvalidRecord("object length"))?;
    let payload = validate_bytes_identity(bytes, id)
        .map_err(|cause| EngineError::MalformedObject { id, cause })?;
    let actual_length = u64::try_from(bytes.len()).map_err(|_| EngineError::CounterOverflow)?;
    if expected_kind != ObjectKind::Bytes || actual_length != expected_length {
        return Err(EngineError::MalformedObject {
            id,
            cause: CoreError::LengthMismatch {
                expected: expected_length,
                actual: actual_length,
            },
        });
    }
    Ok((payload, actual_length))
}

fn payload_batch_sql(count: usize) -> EngineResult<String> {
    if !(1..=64).contains(&count) {
        return Err(EngineError::InvalidRecord("payload batch size"));
    }
    Ok((0..count)
        .map(|index| {
            format!(
                "SELECT {index} AS ord, kind, canonical_length, canonical_bytes FROM layerfs_objects WHERE object_id = ?{}",
                index + 1
            )
        })
        .collect::<Vec<_>>()
        .join(" UNION ALL ")
        + " ORDER BY 1")
}

fn authenticate_borrowed_unaccounted(
    id: ObjectId,
    kind: i64,
    length: i64,
    bytes: &[u8],
) -> EngineResult<(ObjectSummary, u64)> {
    let expected_kind = ObjectKind::try_from(
        u8::try_from(kind).map_err(|_| EngineError::InvalidRecord("object kind"))?,
    )?;
    let expected_length =
        u64::try_from(length).map_err(|_| EngineError::InvalidRecord("object length"))?;
    let object = authenticate_identity(bytes, id)
        .map_err(|cause| EngineError::MalformedObject { id, cause })?;
    let actual_length = u64::try_from(bytes.len()).map_err(|_| EngineError::CounterOverflow)?;
    if object.kind != expected_kind || actual_length != expected_length {
        return Err(EngineError::MalformedObject {
            id,
            cause: CoreError::LengthMismatch {
                expected: expected_length,
                actual: actual_length,
            },
        });
    }
    Ok((object, actual_length))
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
    engine.bump(|counters| {
        checked_add(&mut counters.objects_validated, 1)?;
        checked_add(&mut counters.new_object_authentication_passes, 1)?;
        checked_add(&mut counters.put_lookup_statements, 1)
    })?;
    match with_authenticated_canonical_on_connection(
        engine,
        connection,
        id,
        false,
        false,
        |kind, stored| {
            if kind != object.kind() || stored != canonical_bytes {
                return Err(EngineError::ImmutableConflict("object", id));
            }
            Ok(())
        },
    ) {
        Ok(()) => {
            engine.bump(|counters| {
                checked_add(&mut counters.objects_reused, 1)?;
                checked_add(&mut counters.reused_rows, 1)?;
                checked_add(&mut counters.incumbent_authentication_passes, 1)
            })?;
            return Ok(PutOutcome::Reused);
        }
        Err(EngineError::MissingObject(missing)) if missing == id => {}
        Err(error) => return Err(error),
    }

    engine.mark_statement()?;
    engine.bump(|counters| checked_add(&mut counters.put_insert_statements, 1))?;
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
        checked_add(&mut counters.created_rows, 1)?;
        checked_add(&mut counters.object_bytes_written, canonical_len)?;
        checked_add(&mut counters.logical_object_bytes, canonical_len)
    })?;
    Ok(PutOutcome::Created)
}

fn authenticate_directory_object(
    engine: &Engine,
    connection: &Connection,
    id: ObjectId,
) -> EngineResult<()> {
    with_authenticated_canonical_on_connection(engine, connection, id, false, false, |kind, _| {
        if kind == ObjectKind::Directory {
            Ok(())
        } else {
            Err(EngineError::InvalidRecord("root directory object"))
        }
    })
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

#[cfg(test)]
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

#[cfg(test)]
fn root_record_len(root: &RootRecord) -> EngineResult<u64> {
    ROOT_RECORD_BASE_BYTES
        .checked_add(if root.parent.is_some() { 32 } else { 0 })
        .ok_or(EngineError::CounterOverflow)
}

#[cfg(test)]
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
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn foreign_hot_journal_child() {
        let Some(path) = std::env::var_os("LAYERFS_FOREIGN_HOT_JOURNAL") else {
            return;
        };
        let connection = Connection::open(path).unwrap();
        connection.execute_batch("BEGIN IMMEDIATE").unwrap();
        connection
            .execute("UPDATE foreign_table SET value = 'mutated'", [])
            .unwrap();
        std::process::exit(92);
    }

    fn test_path() -> PathBuf {
        let id = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("layerfs-engine-{}-{id}.sqlite", std::process::id()))
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
    fn payload_batch_union_preserves_order_without_sorting() {
        let path = test_path();
        let engine =
            Engine::open_with_mode(&path, integrity::IntegrityMode::TrustedLocalDev).unwrap();
        let (id, canonical) = bytes_object(b"payload");
        engine.put_object_if_absent(id, &canonical).unwrap();
        let connection = engine.lock_connection().unwrap();
        for count in [1, 4, 5, 6, 64] {
            let ids = vec![id; count];
            let sql = payload_batch_sql(count).unwrap();
            let mut statement = connection.prepare(&sql).unwrap();
            let rows_seen = {
                let mut rows = statement
                    .query(rusqlite::params_from_iter(
                        ids.iter().map(|id| id.as_bytes().as_slice()),
                    ))
                    .unwrap();
                let mut ordinal = 0;
                while let Some(row) = rows.next().unwrap() {
                    assert_eq!(row.get::<_, i64>(0).unwrap(), ordinal);
                    ordinal += 1;
                }
                ordinal
            };
            assert_eq!(rows_seen, count as i64);
            assert_eq!(statement.get_status(rusqlite::StatementStatus::Sort), 0);
        }
        drop(connection);
        drop(engine);
        std::fs::remove_file(path).unwrap();
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
            assert_eq!(engine.profile().cache_pages, 1280);
            assert_eq!(engine.profile().cache_spill_pages, 1280);
            assert_eq!(
                engine.put_object_if_absent(id, &bytes),
                Ok(PutOutcome::Created)
            );
            assert_eq!(
                engine.read_object_range(id, 2..7).expect("range"),
                bytes[2..7]
            );
            let reversed_start = 7;
            let reversed_end = 2;
            assert!(matches!(
                engine.read_object_range(id, reversed_start..reversed_end),
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
    fn sqlite_error_mapping_preserves_required_classes() {
        for (code, expected) in [
            (rusqlite::ErrorCode::DatabaseBusy, SqliteErrorKind::Busy),
            (rusqlite::ErrorCode::DatabaseLocked, SqliteErrorKind::Locked),
            (
                rusqlite::ErrorCode::PermissionDenied,
                SqliteErrorKind::PermissionDenied,
            ),
            (rusqlite::ErrorCode::DiskFull, SqliteErrorKind::NoSpace),
            (
                rusqlite::ErrorCode::DatabaseCorrupt,
                SqliteErrorKind::Corrupt,
            ),
            (rusqlite::ErrorCode::ReadOnly, SqliteErrorKind::ReadOnly),
            (
                rusqlite::ErrorCode::ConstraintViolation,
                SqliteErrorKind::Constraint,
            ),
            (rusqlite::ErrorCode::SystemIoFailure, SqliteErrorKind::Io),
        ] {
            assert!(matches!(
                map_sqlite_error(rusqlite::Error::SqliteFailure(
                    rusqlite::ffi::Error {
                        code,
                        extended_code: 0,
                    },
                    None,
                )),
                EngineError::Sqlite { kind, .. } if kind == expected
            ));
        }
    }

    #[test]
    fn admission_never_mutates_foreign_or_incomplete_databases() {
        let foreign = test_path();
        let connection = Connection::open(&foreign).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE foreign_data (value TEXT); INSERT INTO foreign_data VALUES ('keep');",
            )
            .unwrap();
        drop(connection);
        let before = fs::read(&foreign).unwrap();
        assert!(matches!(
            Engine::open(&foreign),
            Err(EngineError::SchemaMismatch)
        ));
        assert_eq!(fs::read(&foreign).unwrap(), before);
        let connection = Connection::open(&foreign).unwrap();
        assert_eq!(
            connection
                .query_row("SELECT value FROM foreign_data", [], |row| row
                    .get::<_, String>(0))
                .unwrap(),
            "keep"
        );
        drop(connection);
        fs::remove_file(&foreign).unwrap();

        for table in ["layerfs_store_meta", "layerfs_authority"] {
            let path = test_path();
            drop(Engine::open(&path).unwrap());
            let connection = Connection::open(&path).unwrap();
            connection
                .execute(&format!("DELETE FROM {table}"), [])
                .unwrap();
            drop(connection);
            let before = fs::read(&path).unwrap();
            assert!(matches!(
                Engine::open(&path),
                Err(EngineError::SchemaMismatch)
            ));
            assert_eq!(fs::read(&path).unwrap(), before, "opening replaced {table}");
            let connection = Connection::open(&path).unwrap();
            let count = connection
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap();
            assert_eq!(count, 0);
            drop(connection);
            fs::remove_file(path).unwrap();
        }

        let impostor = test_path();
        drop(Engine::open(&impostor).unwrap());
        let connection = Connection::open(&impostor).unwrap();
        connection
            .execute_batch(
                "ALTER TABLE layerfs_authority RENAME TO saved_authority;
                 CREATE TABLE layerfs_authority (
                    authority_id INTEGER PRIMARY KEY,
                    store_id BLOB NOT NULL,
                    next_inode_serial INTEGER NOT NULL,
                    trusted_history INTEGER NOT NULL
                 );
                 INSERT INTO layerfs_authority SELECT * FROM saved_authority;
                 DROP TABLE saved_authority;",
            )
            .unwrap();
        let authority = connection
            .query_row(
                "SELECT authority_id, store_id, next_inode_serial, trusted_history FROM layerfs_authority",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .unwrap();
        drop(connection);
        let before = fs::read(&impostor).unwrap();
        assert!(matches!(
            Engine::open(&impostor),
            Err(EngineError::SchemaMismatch)
        ));
        assert_eq!(fs::read(&impostor).unwrap(), before);
        let connection = Connection::open(&impostor).unwrap();
        let after = connection
            .query_row(
                "SELECT authority_id, store_id, next_inode_serial, trusted_history FROM layerfs_authority",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(after, authority);
        drop(connection);
        fs::remove_file(impostor).unwrap();

        let escaped = test_path();
        drop(Engine::open(&escaped).unwrap());
        let connection = Connection::open(&escaped).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE sqliteX_data (value TEXT);
                 INSERT INTO sqliteX_data VALUES ('keep');
                 CREATE TRIGGER sqliteX_trigger AFTER INSERT ON sqliteX_data
                 BEGIN UPDATE sqliteX_data SET value = value; END;",
            )
            .unwrap();
        drop(connection);
        let before = fs::read(&escaped).unwrap();
        assert!(matches!(
            Engine::open(&escaped),
            Err(EngineError::SchemaMismatch)
        ));
        assert_eq!(fs::read(&escaped).unwrap(), before);
        let connection = Connection::open(&escaped).unwrap();
        assert_eq!(
            connection
                .query_row("SELECT value FROM sqliteX_data", [], |row| {
                    row.get::<_, String>(0)
                })
                .unwrap(),
            "keep"
        );
        drop(connection);
        fs::remove_file(escaped).unwrap();
    }

    #[test]
    fn reconciliation_read_never_creates_missing_or_accepts_replaced_store() {
        let path = test_path();
        let original = Engine::open(&path).unwrap();
        let store_id = original.store_id().unwrap();
        drop(original);
        let saved = path.with_extension("saved");
        fs::rename(&path, &saved).unwrap();
        assert!(read_ref_reconcile_readonly(&path, "main", store_id).is_err());
        assert!(
            !path.exists(),
            "read-only reconciliation created a database"
        );

        let replacement = Engine::open(&path).unwrap();
        assert_ne!(replacement.store_id().unwrap(), store_id);
        drop(replacement);
        assert!(matches!(
            read_ref_reconcile_readonly(&path, "main", store_id),
            Err(EngineError::InvalidRecord("reconciliation StoreId"))
        ));
        fs::remove_file(path).unwrap();
        fs::remove_file(saved).unwrap();
    }

    #[test]
    fn read_only_admission_preserves_foreign_hot_journal_bytes() {
        let path = test_path();
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "PRAGMA journal_mode=DELETE;
                 CREATE TABLE foreign_table (value TEXT NOT NULL);
                 INSERT INTO foreign_table VALUES ('prior');",
            )
            .unwrap();
        drop(connection);
        let database_before = fs::read(&path).unwrap();
        let status = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tests::foreign_hot_journal_child"])
            .env("LAYERFS_FOREIGN_HOT_JOURNAL", &path)
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(92));
        let journal = PathBuf::from(format!("{}-journal", path.display()));
        let journal_before = fs::read(&journal).unwrap();
        assert!(Engine::open(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), database_before);
        assert_eq!(fs::read(&journal).unwrap(), journal_before);
        fs::remove_file(journal).unwrap();
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn two_verified_snapshot_readers_coexist() {
        let path = test_path();
        let first = Engine::open(&path).unwrap();
        let second = Engine::open(&path).unwrap();
        let guard = first.lock_connection().unwrap();
        guard
            .query_row(
                "SELECT trusted_history FROM layerfs_authority WHERE authority_id = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap();
        assert_eq!(second.read_ref("main").unwrap(), None);
        drop(guard);
        drop(first);
        drop(second);
        fs::remove_file(path).unwrap();
    }
}
