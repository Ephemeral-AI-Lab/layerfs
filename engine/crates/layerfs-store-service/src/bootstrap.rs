//! Concrete Store composition and mount-time reader opening.
use crate::store::{Store, StoreReader};
use layerfs_persistence::{
    DeferredSqliteWriter, PersistenceConfig, SqlWork, SqliteHandles, StorageProvider,
};
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
    /// A one-shot or held PostgreSQL read service's only session.
    ReadOnly(Arc<layerfs_persistence::PostgresStorage>),
    Sqlite(Arc<StorageProvider>),
    DeferredSqlite {
        writer: Arc<DeferredSqliteWriter>,
        reader: Arc<StorageProvider>,
    },
    Postgres(Arc<layerfs_persistence::PostgresStorage>),
}
impl Writer {
    fn diagnostics(&self) -> Result<Option<SqlWork>, PersistenceError> {
        match self {
            Self::ReadOnly(_) => Ok(None),
            Self::Sqlite(provider) => provider.diagnostics().map(Some),
            Self::DeferredSqlite { writer, .. } => writer
                .get()
                .map(|writer| writer.storage.diagnostics())
                .transpose(),
            Self::Postgres(provider) => provider.diagnostics().map(Some),
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
        let writer = self.writer.diagnostics().ok().flatten();
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
    /// Prepares the writer's mount and unmount statements in one flight
    /// (#768): on PostgreSQL one round trip and one read transaction, so a
    /// mount's admission and an unmount's release prepare nothing. Only a
    /// service that mounts calls it, the daemon at its Store open; an SDK
    /// call, an install or a restore does not pay it, and a mount on such a
    /// service prepares its statements at first use instead. Nothing on
    /// SQLite.
    pub fn prepare_mounts(&self) -> StorageResult<()> {
        match &self.writer {
            Writer::ReadOnly(_) | Writer::Sqlite(_) | Writer::DeferredSqlite { .. } => Ok(()),
            Writer::Postgres(storage) => storage.prepare_mount_writes().map_err(StorageError::from),
        }
    }
    /// Prepares the writer's create statements in one flight (#818 W4): on
    /// PostgreSQL one round trip and one read transaction, so a first create
    /// (an SDK write that does not take the fused Commit) prepares none of
    /// them, and its serial reservation is one message. The SDK's Store open
    /// calls it (`StoreService::open_postgres`); the daemon does not.
    /// Nothing on SQLite (its deferred writer included) or on a read-only
    /// PostgreSQL service, which sends no writes.
    pub fn prepare_sdk_writes(&self) -> StorageResult<()> {
        match &self.writer {
            Writer::ReadOnly(_) | Writer::Sqlite(_) | Writer::DeferredSqlite { .. } => Ok(()),
            Writer::Postgres(storage) => storage.prepare_sdk_writes().map_err(StorageError::from),
        }
    }
    /// The Store-file operations of a SQLite session (`None` for a
    /// PostgreSQL Store, which has no file).
    pub fn file(&self) -> Option<&dyn layerfs_persistence::StoreFile> {
        match &self.writer {
            Writer::Sqlite(provider) => {
                Some(provider.as_ref() as &dyn layerfs_persistence::StoreFile)
            }
            Writer::DeferredSqlite { writer, reader } => Some(
                writer
                    .get()
                    .map_or(reader, |writer| &writer.storage)
                    .as_ref(),
            ),
            Writer::ReadOnly(_) | Writer::Postgres(_) => None,
        }
    }
    /// Checkpoints an owned SQLite writer. Explicit maintenance can start
    /// one. An idle checkpoint following a Store call only uses an open one.
    pub fn checkpoint(&self, explicit: bool) -> Result<Option<bool>, PersistenceError> {
        use layerfs_persistence::StoreFile;
        let provider = match &self.writer {
            Writer::Sqlite(provider) => Some(provider),
            Writer::DeferredSqlite { writer, .. } if explicit => Some(&writer.start()?.storage),
            Writer::DeferredSqlite { writer, .. } => writer.get().map(|writer| &writer.storage),
            Writer::ReadOnly(_) | Writer::Postgres(_) => None,
        };
        provider.map(|provider| provider.checkpoint()).transpose()
    }
    /// Whether a held PostgreSQL service can serve another call at the
    /// generation it opened at (`PostgresStorage::current`: one round trip):
    /// a read service's one session, or a writable service's writer
    /// session (#819 P-B writes), whose readers check themselves when used.
    /// `None` for a SQLite service.
    pub fn postgres_session_current(&self) -> Option<bool> {
        match &self.writer {
            Writer::ReadOnly(session) | Writer::Postgres(session) => Some(session.current()),
            _ => None,
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
            return self.writer.diagnostics().map_err(StorageError::from);
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
            writer: self.writer.diagnostics()?.unwrap_or_default(),
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
            return open_postgres(
                &selection,
                binding,
                cursor_key,
                workspace_capacity,
                cache_bytes,
                reservations,
            );
        }
    };
    let writer = SqliteHandles::open_writable(config.clone(), binding, cursor_key)?;
    let sqlite_version = writer.profile().sqlite_version.clone();
    let observations = registry();
    let binding = binding.to_vec();
    let factory = sqlite_readers(config, binding, cursor_key, observations.clone());
    let control = factory(0)?;
    let committer = writer.history.committer();
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
        .with_merge_scratch(writer.storage.merge_scratch())
        .with_committer(committer),
    );
    Ok(OpenedStore {
        store,
        sqlite_version,
        writer: Writer::Sqlite(writer.storage),
        readers: observations,
    })
}

fn sqlite_reader(
    opened: SqliteHandles,
    observations: &Registry,
    index: usize,
) -> StorageResult<StoreReader> {
    let provider: Weak<dyn Observed> = Arc::downgrade(&opened.storage) as Weak<_>;
    let reader = StoreReader::new(Storage::new(opened.storage)?, Arc::new(opened.history));
    observe(observations, index, provider, reader)
}

fn sqlite_readers(
    config: PersistenceConfig,
    binding: Vec<u8>,
    cursor_key: [u8; 32],
    observations: Registry,
) -> crate::store::ReaderFactory {
    Arc::new(move |index| {
        let opened = SqliteHandles::open_read_only(config.clone(), &binding, cursor_key)?;
        sqlite_reader(opened, &observations, index)
    })
}

/// In-process SQLite Store calls have one validated control reader and an
/// unopened writer. The daemon and standalone install use eager assembly.
pub(crate) fn open_sqlite_for_calls(
    config: PersistenceConfig,
    binding: &[u8],
    cursor_key: [u8; 32],
    workspace_capacity: usize,
    cache_bytes: usize,
    reservations: ReservationBlocks,
) -> StorageResult<OpenedStore> {
    let (opened, history, writer) =
        DeferredSqliteWriter::open(config.clone(), binding, cursor_key)?;
    let sqlite_version = opened.profile().sqlite_version.clone();
    let reader = opened.storage.clone();
    let observations = registry();
    let control = sqlite_reader(opened, &observations, 0)?;
    let factory = sqlite_readers(config, binding.to_vec(), cursor_key, observations.clone());
    let store = Arc::new(
        Store::with_control_reader(
            Arc::new(history),
            control,
            factory,
            cache_bytes,
            reservations,
            workspace_capacity,
        )?
        .with_deferred_sqlite_writer(writer.clone()),
    );
    Ok(OpenedStore {
        store,
        sqlite_version,
        writer: Writer::DeferredSqlite { writer, reader },
        readers: observations,
    })
}

/// A one-shot read over a single PostgreSQL session with Full validation.
pub(crate) fn open_postgres_read_only(
    selection: &layerfs_persistence::PostgresSelection,
    binding: &[u8],
    cursor_key: [u8; 32],
    cache_bytes: usize,
) -> StorageResult<OpenedStore> {
    use layerfs_persistence::{PostgresCheck, PostgresHandles};
    let selection = selection.clone().with_check(PostgresCheck::Full);
    let opened = PostgresHandles::open_read_only(&selection, binding, cursor_key)?;
    let session = opened.storage.clone();
    let version = format!("postgresql {}", opened.storage.server_version());
    let observations = registry();
    let provider: Weak<dyn Observed> = Arc::downgrade(&opened.storage) as Weak<_>;
    let history = Arc::new(opened.history);
    let control = StoreReader::new(Storage::new(opened.storage)?, history.clone());
    let control = observe(&observations, 0, provider, control)?;
    let batches = layerfs_storage::port::PackPersistence::batched_chain_reads(&*session);
    let store = Store::with_control_reader(
        history,
        control,
        Arc::new(|_| {
            Err(StorageError::Integrity(
                "one-shot read has no Workspace reader",
            ))
        }),
        cache_bytes,
        ReservationBlocks::default(),
        1,
    )?
    .with_batched_read_session(batches);
    Ok(OpenedStore {
        store: Arc::new(store),
        sqlite_version: version,
        writer: Writer::ReadOnly(session),
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
    // A lost reply quarantines the writer; its next call reopens it once,
    // settling what the closed connection left, instead of refusing every
    // later Commit, mount and release until the service closes.
    writer.storage.enable_reopen();
    let version = format!("postgresql {}", writer.storage.server_version());
    let selection = (selection.clone()).with_check(layerfs_persistence::PostgresCheck::Light);
    let binding = binding.to_vec();
    let observations = registry();
    let registry = observations.clone();
    let factory: crate::store::ReaderFactory = Arc::new(move |index| {
        let opened = PostgresHandles::open_read_only(&selection, &binding, cursor_key)?;
        // A reader reopens too: a read changes nothing on the server, so a
        // lost reply leaves no custody, and a reader whose connection
        // closed (a lost reply, a restart) serves its next call on a new
        // one instead of failing every later read of its Workspace.
        opened.storage.enable_reopen();
        // #768: a Workspace reader (the control reader is index 0) prepares
        // its mount statements in one flight when it opens: at the mount
        // that opens it, or after readiness for the idle reader.
        if index != 0 {
            opened.storage.prepare_mount_reads()?;
        }
        let provider: Weak<dyn Observed> = Arc::downgrade(&opened.storage) as Weak<_>;
        let session = opened.storage.clone();
        let reader = StoreReader::new(Storage::new(opened.storage)?, Arc::new(opened.history))
            .with_usable(move || session.usable())
            .reopening();
        observe(&registry, index, provider, reader)
    });
    let committer = writer.history.committer();
    let store = Arc::new(
        Store::with_shared_readers(
            writer.storage.clone(),
            Arc::new(writer.history),
            factory,
            cache_bytes,
            reservations,
            workspace_capacity,
        )?
        .with_merge_scratch(writer.storage.merge_scratch())
        .with_committer(committer)
        .with_mount_on_writer(),
    );
    Ok(OpenedStore {
        store,
        sqlite_version: version,
        writer: Writer::Postgres(writer.storage),
        readers: observations,
    })
}
