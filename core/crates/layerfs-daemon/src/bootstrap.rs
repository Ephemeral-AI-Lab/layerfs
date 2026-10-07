//! Concrete Store composition, outside the provider-independent adapter.
use crate::store::Store;
use layerfs_persistence::{Handles, PersistenceConfig, SqlWork, StorageProvider};
use layerfs_storage::{ReservationBlocks, Storage, StorageError, StorageResult};
use std::sync::Arc;

/// Concrete startup metadata stays outside the provider-independent Store.
pub struct OpenedStore {
    pub store: Arc<Store>,
    pub sqlite_version: String,
    writer: Arc<StorageProvider>,
    readers: Vec<Arc<StorageProvider>>,
}

/// Cumulative observations, separately scoped to the writer and each fixed reader.
pub struct StoreDiagnostics {
    pub writer: SqlWork,
    pub readers: Vec<SqlWork>,
}
impl OpenedStore {
    /// Reads existing session counters; this issues no SQL or provider reopen.
    pub fn diagnostics(&self) -> StorageResult<StoreDiagnostics> {
        Ok(StoreDiagnostics {
            writer: self.writer.diagnostics()?,
            readers: self
                .readers
                .iter()
                .map(|reader| reader.diagnostics().map_err(StorageError::from))
                .collect::<StorageResult<Vec<_>>>()?,
        })
    }
}

/// Opens one writer first and a fixed number of independent read-only sessions.
/// Read capacity is selected before daemon readiness. No create, conversion,
/// retry or read-only-to-writable fallback occurs on an open failure.
pub fn open_store(
    config: PersistenceConfig,
    binding: &[u8],
    cursor_key: [u8; 32],
    read_handles: usize,
    cache_bytes: usize,
    reservations: ReservationBlocks,
) -> StorageResult<Arc<Store>> {
    open_store_observed(
        config,
        binding,
        cursor_key,
        read_handles,
        cache_bytes,
        reservations,
    )
    .map(|opened| opened.store)
}

/// Opens once and retains the version reported by the actual writable provider.
pub fn open_store_observed(
    config: PersistenceConfig,
    binding: &[u8],
    cursor_key: [u8; 32],
    read_handles: usize,
    cache_bytes: usize,
    reservations: ReservationBlocks,
) -> StorageResult<OpenedStore> {
    if read_handles == 0 {
        return Err(StorageError::Integrity("empty Store read set"));
    }
    reservations.validate()?;
    let writer = Handles::open_writable(config.clone(), binding, cursor_key)?;
    let sqlite_version = writer.profile().sqlite_version.clone();
    let mut readers = Vec::with_capacity(read_handles);
    let mut observed_readers = Vec::with_capacity(read_handles);
    for _ in 0..read_handles {
        let opened = Handles::open_read_only(config.clone(), binding, cursor_key)?;
        observed_readers.push(opened.storage.clone());
        readers.push(Storage::new(opened.storage)?);
    }
    let store = Arc::new(Store::new(
        writer.storage.clone(),
        Arc::new(writer.history),
        readers,
        cache_bytes,
        reservations,
    )?);
    Ok(OpenedStore {
        store,
        sqlite_version,
        writer: writer.storage,
        readers: observed_readers,
    })
}
