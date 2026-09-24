//! Store, history, grant and capacity construction shared by every entry point.
use crate::host::HistoryMode;
use crate::Grant;
use layerfs_bridge::contract::{Code, Failure};
use layerfs_history::{sqlite, HistoryCatalog, HistoryCatalogConfig};
use layerfs_storage::Store;
use layerfs_telemetry::timer::{Timing, TimingScope};
use std::{path::Path, sync::Arc};

pub(crate) fn create(path: &Path, scope: TimingScope<'_>) -> Result<Store, Failure> {
    Store::create(path, Store::default_policy(), scope).map_err(crate::service::error::storage)
}

pub(crate) fn open(path: &Path) -> Result<Store, Failure> {
    Timing::disabled("open", |scope| Store::open(path, scope.child("open")))
        .0
        .map_err(crate::service::error::storage)
}

/// The acceptor's session bound: the Store's own persisted write budget plus
/// the service read bound.
pub(crate) fn capacity(store: &Store) -> Result<usize, Failure> {
    store
        .max_concurrent_writes()
        .map(crate::host::acceptor::session_capacity)
        .map_err(crate::service::error::storage)
}

pub(crate) fn history(
    path: &Path,
    config: &HistoryCatalogConfig,
    mode: HistoryMode,
) -> Result<Arc<dyn HistoryCatalog>, Failure> {
    let catalog = match mode {
        HistoryMode::Create => sqlite::create(path, config),
        HistoryMode::OpenWritable => {
            sqlite::open_writable(path, &config.binding_key, config.cursor_key)
        }
    };
    catalog
        .map(|catalog| Arc::new(catalog) as Arc<dyn HistoryCatalog>)
        .map_err(crate::service::error::catalog)
}

pub(crate) fn grants(public_keys: &[[u8; 32]]) -> Result<Vec<Grant>, Failure> {
    if public_keys.is_empty() || public_keys.len() > 16 {
        return Err(Code::InvalidInput.into());
    }
    Ok(public_keys
        .iter()
        .map(|public_key| Grant {
            public_key: *public_key,
            operations: u8::MAX,
            expires_unix: u64::MAX,
        })
        .collect())
}
