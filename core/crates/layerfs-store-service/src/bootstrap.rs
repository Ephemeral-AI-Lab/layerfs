//! Concrete Store composition and mount-time reader opening.
use crate::store::{Store, StoreReader};
use layerfs_persistence::{Handles, PersistenceConfig, SqlWork, StorageProvider};
use layerfs_storage::{ReservationBlocks, Storage, StorageError, StorageResult};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, Weak},
};

struct ReaderObservations {
    live: BTreeMap<usize, Weak<StorageProvider>>,
    retired: Option<SqlWork>,
}
pub struct OpenedStore {
    pub store: Arc<Store>,
    pub sqlite_version: String,
    writer: Arc<StorageProvider>,
    readers: Arc<Mutex<ReaderObservations>>,
}
/// The Store's SQL sessions, observed without holding the daemon's state.
pub struct SqlSessions {
    writer: Arc<StorageProvider>,
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
                total.accumulate(&provider.diagnostics().ok()?);
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
            .map(|provider| provider.diagnostics().map_err(StorageError::from))
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
    config: PersistenceConfig,
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
    config: PersistenceConfig,
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
    let writer = Handles::open_writable(config.clone(), binding, cursor_key)?;
    let sqlite_version = writer.profile().sqlite_version.clone();
    let observations = Arc::new(Mutex::new(ReaderObservations {
        live: BTreeMap::new(),
        retired: Some(SqlWork::default()),
    }));
    let binding = binding.to_vec();
    let registry = observations.clone();
    let factory: crate::store::ReaderFactory = Arc::new(move |index| {
        let opened = Handles::open_read_only(config.clone(), &binding, cursor_key)?;
        let provider = Arc::downgrade(&opened.storage);
        let reader = StoreReader::new(Storage::new(opened.storage)?, Arc::new(opened.history));
        registry
            .lock()
            .map_err(|_| StorageError::Integrity("reader observations poisoned"))?
            .live
            .insert(index, provider.clone());
        let observed = registry.clone();
        Ok(reader.on_close(move || {
            // Weak observation never keeps the SQL connection open after unmount.
            let mut rows = observed.lock().unwrap_or_else(|poison| poison.into_inner());
            if let Some(provider) = provider.upgrade() {
                match provider.diagnostics() {
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
        writer: writer.storage,
        readers: observations,
    })
}
