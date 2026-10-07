//! Concrete Store composition, outside the provider-independent adapter.
use crate::store::Store;
use layerfs_persistence::{Handles, PersistenceConfig};
use layerfs_storage::{Storage, StorageError, StorageResult};
use std::sync::Arc;

/// Opens one writer first and a fixed number of independent read-only sessions.
/// Read capacity is selected before daemon readiness. No create, conversion,
/// retry or read-only-to-writable fallback occurs on an open failure.
pub fn open_store(
    config: PersistenceConfig,
    binding: &[u8],
    cursor_key: [u8; 32],
    read_handles: usize,
    cache_bytes: usize,
) -> StorageResult<Arc<Store>> {
    if read_handles == 0 {
        return Err(StorageError::Integrity("empty Store read set"));
    }
    let writer = Handles::open_writable(config.clone(), binding, cursor_key)?;
    let mut readers = Vec::with_capacity(read_handles);
    for _ in 0..read_handles {
        let opened = Handles::open_read_only(config.clone(), binding, cursor_key)?;
        readers.push(Storage::new(opened.storage)?);
    }
    Ok(Arc::new(Store::new(
        writer.storage,
        Arc::new(writer.history),
        readers,
        cache_bytes,
    )?))
}
