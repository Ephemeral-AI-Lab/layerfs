//! A sealed Store opens: the fresh file.
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{
    PersistenceConfig, SqliteAcquisitionSchema, SqliteHandles, SqlitePersistenceProfile,
};
use layerfs_storage::StoragePolicy;

const BINDING: &[u8] = b"sealed-open";
const CURSOR: [u8; 32] = [61; 32];

fn catalog() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: BINDING.to_vec(),
        cursor_key: CURSOR,
        incarnation: 1,
    }
}

/// A fresh Normal Store with the acquisition tables `ProjectApi::init`
/// creates, sealed to its one closed file.
fn sealed_store(directory: &std::path::Path) -> std::path::PathBuf {
    let path = directory.join("sealed.sqlite");
    let handles = SqliteHandles::create(
        PersistenceConfig::sqlite(&path)
            .with_sqlite_profile(SqlitePersistenceProfile::Normal)
            .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables),
        StoragePolicy::frozen_default(),
        &catalog(),
    )
    .unwrap();
    handles.seal().unwrap();
    path
}

/// The profile every installed Store opens with (the daemon's `Normal`).
fn installed(path: &std::path::Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Normal)
}

#[test]
fn a_sealed_store_opens_read_only() {
    let directory = std::env::temp_dir().join(format!("sealed-open-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let path = sealed_store(&directory);
    SqliteHandles::open_writable(installed(&path), BINDING, CURSOR).unwrap();
    SqliteHandles::open_read_only(installed(&path), BINDING, CURSOR).unwrap();
    std::fs::remove_dir_all(&directory).unwrap();
}
