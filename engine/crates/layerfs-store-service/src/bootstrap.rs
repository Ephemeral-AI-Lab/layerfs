//! Concrete Store composition and mount-time reader opening.
use crate::store::{Store, StoreReader};
use layerfs_persistence::{SqlWork, SqliteHandles, StorageProvider};
use layerfs_storage::{
    port::PersistenceError, ReservationBlocks, Storage, StorageError, StorageResult,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, Weak},
};

/// The Store's writer session on its engine (#506).
#[derive(Clone)]
enum Writer {
    Sqlite(Arc<StorageProvider>),
    #[cfg(feature = "postgresql")]
    Postgres(Arc<layerfs_persistence::PostgresStorage>),
}
impl Writer {
    fn diagnostics(&self) -> Result<SqlWork, PersistenceError> {
        match self {
            Self::Sqlite(provider) => provider.diagnostics(),
            #[cfg(feature = "postgresql")]
            Self::Postgres(provider) => provider.diagnostics(),
        }
    }
}
/// A reader session whose SQL counters can be read.
trait Observed: Send + Sync {
    fn work(&self) -> Result<SqlWork, PersistenceError>;
}
impl Observed for StorageProvider {
    fn work(&self) -> Result<SqlWork, PersistenceError> {
        self.diagnostics()
    }
}
#[cfg(feature = "postgresql")]
impl Observed for layerfs_persistence::PostgresStorage {
    fn work(&self) -> Result<SqlWork, PersistenceError> {
        self.diagnostics()
    }
}
struct ReaderObservations {
    live: BTreeMap<usize, Weak<dyn Observed>>,
    retired: Option<SqlWork>,
}
type Registry = Arc<Mutex<ReaderObservations>>;
fn registry() -> Registry {
    Arc::new(Mutex::new(ReaderObservations {
        live: BTreeMap::new(),
        retired: Some(SqlWork::default()),
    }))
}
/// Registers reader `index`'s session and folds its counters into the
/// retired total when the reader closes. Weak observation never keeps the
/// SQL connection open after unmount.
fn observe(
    registry: &Registry,
    index: usize,
    provider: Weak<dyn Observed>,
    reader: StoreReader,
) -> StorageResult<StoreReader> {
    registry
        .lock()
        .map_err(|_| StorageError::Integrity("reader observations poisoned"))?
        .live
        .insert(index, provider.clone());
    let observed = registry.clone();
    Ok(reader.on_close(move || {
        let mut rows = observed.lock().unwrap_or_else(|poison| poison.into_inner());
        if let Some(provider) = provider.upgrade() {
            match provider.work() {
                Ok(work) => {
                    if let Some(total) = &mut rows.retired {
                        total.accumulate(&work);
                    }
                }
                Err(_) => rows.retired = None,
            }
        }
        rows.live.remove(&index);
    }))
}
pub struct OpenedStore {
    pub store: Arc<Store>,
    /// The engine's version: SQLite's, or `postgresql <server version>`.
    pub sqlite_version: String,
    writer: Writer,
    readers: Arc<Mutex<ReaderObservations>>,
}
/// The Store's SQL sessions, observed without holding the daemon's state.
pub struct SqlSessions {
    writer: Writer,
    readers: Arc<Mutex<ReaderObservations>>,
}
impl SqlSessions {
    /// The writer's totals, and every reader's (live plus retired) summed
    /// under one registry turn so a reader closing meanwhile counts once.
    /// Either is None when a session's counters are unavailable. Last, the
    /// work the calling thread ran on any session (#367).
    pub fn totals(&self) -> crate::telemetry::SqlTotals {
        let writer = self.writer.diagnostics().ok();
        let readers = self.readers.lock().ok().and_then(|readers| {
            let mut total = readers.retired?;
            for provider in readers.live.values().filter_map(Weak::upgrade) {
                total.accumulate(&provider.work().ok()?);
            }
            Some(total)
        });
        (writer, readers, layerfs_persistence::thread_work())
    }
}
pub struct StoreDiagnostics {
    pub writer: SqlWork,
    pub readers: Vec<SqlWork>,
}
impl OpenedStore {
    /// The Store-file operations of a SQLite session (`None` for a
    /// PostgreSQL Store, which has no file).
    pub fn file(&self) -> Option<&dyn layerfs_persistence::StoreFile> {
        match &self.writer {
            Writer::Sqlite(provider) => {
                Some(provider.as_ref() as &dyn layerfs_persistence::StoreFile)
            }
            #[cfg(feature = "postgresql")]
            Writer::Postgres(_) => None,
        }
    }
    pub fn reader_indices(&self) -> Vec<usize> {
        let readers = self
            .readers
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        readers.live.keys().copied().chain([usize::MAX]).collect()
    }
    pub fn session_diagnostics(&self, reader: Option<usize>) -> StorageResult<Option<SqlWork>> {
        let Some(index) = reader else {
            return self
                .writer
                .diagnostics()
                .map(Some)
                .map_err(StorageError::from);
        };
        let readers = self
            .readers
            .lock()
            .map_err(|_| StorageError::Integrity("reader observations poisoned"))?;
        if index == usize::MAX {
            return Ok(readers.retired);
        }
        readers
            .live
            .get(&index)
            .and_then(Weak::upgrade)
            .map(|provider| provider.work().map_err(StorageError::from))
            .transpose()
    }
    pub fn sql_sessions(&self) -> SqlSessions {
        SqlSessions {
            writer: self.writer.clone(),
            readers: self.readers.clone(),
        }
    }
    pub fn diagnostics(&self) -> StorageResult<StoreDiagnostics> {
        Ok(StoreDiagnostics {
            writer: self.writer.diagnostics()?,
            readers: self
                .reader_indices()
                .into_iter()
                .map(|index| {
                    self.session_diagnostics(Some(index))?
                        .ok_or(StorageError::Integrity("reader observation unavailable"))
                })
                .collect::<StorageResult<Vec<_>>>()?,
        })
    }
}

pub fn open_store(
    config: impl Into<layerfs_persistence::StoreSelection>,
    binding: &[u8],
    cursor_key: [u8; 32],
    workspace_capacity: usize,
    cache_bytes: usize,
    reservations: ReservationBlocks,
) -> StorageResult<Arc<Store>> {
    open_store_observed(
        config,
        binding,
        cursor_key,
        workspace_capacity,
        cache_bytes,
        reservations,
    )
    .map(|opened| opened.store)
}
/// One control reader opens at startup. Each Workspace reader opens once at bind,
/// never in the middle of a request. Persistence verifies its 2 MiB page cache.
pub fn open_store_observed(
    config: impl Into<layerfs_persistence::StoreSelection>,
    binding: &[u8],
    cursor_key: [u8; 32],
    workspace_capacity: usize,
    cache_bytes: usize,
    reservations: ReservationBlocks,
) -> StorageResult<OpenedStore> {
    if workspace_capacity == 0 {
        return Err(StorageError::Integrity("empty Workspace reader capacity"));
    }
    reservations.validate()?;
    let config = match config.into() {
        layerfs_persistence::StoreSelection::Sqlite(config) => config,
        layerfs_persistence::StoreSelection::Postgres(selection) => {
            let _ = &selection;
            #[cfg(feature = "postgresql")]
            return open_postgres(
                &selection,
                binding,
                cursor_key,
                workspace_capacity,
                cache_bytes,
                reservations,
            );
            #[cfg(not(feature = "postgresql"))]
            return Err(PersistenceError::BackendUnavailable.into());
        }
    };
    let writer = SqliteHandles::open_writable(config.clone(), binding, cursor_key)?;
    let sqlite_version = writer.profile().sqlite_version.clone();
    let observations = registry();
    let binding = binding.to_vec();
    let registry = observations.clone();
    let factory: crate::store::ReaderFactory = Arc::new(move |index| {
        let opened = SqliteHandles::open_read_only(config.clone(), &binding, cursor_key)?;
        let provider: Weak<dyn Observed> = Arc::downgrade(&opened.storage) as Weak<_>;
        let reader = StoreReader::new(Storage::new(opened.storage)?, Arc::new(opened.history));
        observe(&registry, index, provider, reader)
    });
    let control = factory(0)?;
    let store = Arc::new(
        Store::new(
            writer.storage.clone(),
            Arc::new(writer.history),
            control,
            factory,
            cache_bytes,
            reservations,
            workspace_capacity,
        )?
        .with_merge_scratch(writer.storage.merge_scratch()),
    );
    Ok(OpenedStore {
        store,
        sqlite_version,
        writer: Writer::Sqlite(writer.storage),
        readers: observations,
    })
}

/// A PostgreSQL Store (#506): one writer session, checked as `selection`
/// says, and read-only sessions with the light check (#63), each observed as
/// on SQLite. Readers are shared (#64): the control reader opens at its first
/// use, and an unmounted Workspace's healthy reader serves the next mount,
/// so the daemon holds at most the writer, its lane connection, the control
/// reader, one reader per mounted Workspace and `IDLE_READERS` idle ones.
/// A merge's records live in the writer connection's temporary tables
/// (#506 d).
#[cfg(feature = "postgresql")]
fn open_postgres(
    selection: &layerfs_persistence::PostgresSelection,
    binding: &[u8],
    cursor_key: [u8; 32],
    workspace_capacity: usize,
    cache_bytes: usize,
    reservations: ReservationBlocks,
) -> StorageResult<OpenedStore> {
    use layerfs_persistence::PostgresHandles;
    let writer = PostgresHandles::open_writable(selection, binding, cursor_key)?;
    let version = format!("postgresql {}", writer.storage.server_version());
    let selection = (selection.clone()).with_check(layerfs_persistence::PostgresCheck::Light);
    let binding = binding.to_vec();
    let observations = registry();
    let registry = observations.clone();
    let factory: crate::store::ReaderFactory = Arc::new(move |index| {
        let opened = PostgresHandles::open_read_only(&selection, &binding, cursor_key)?;
        let provider: Weak<dyn Observed> = Arc::downgrade(&opened.storage) as Weak<_>;
        let session = opened.storage.clone();
        let reader = StoreReader::new(Storage::new(opened.storage)?, Arc::new(opened.history))
            .with_usable(move || session.usable());
        observe(&registry, index, provider, reader)
    });
    let store = Arc::new(
        Store::with_shared_readers(
            writer.storage.clone(),
            Arc::new(writer.history),
            factory,
            cache_bytes,
            reservations,
            workspace_capacity,
        )?
        .with_merge_scratch(writer.storage.merge_scratch()),
    );
    Ok(OpenedStore {
        store,
        sqlite_version: version,
        writer: Writer::Postgres(writer.storage),
        readers: observations,
    })
}
