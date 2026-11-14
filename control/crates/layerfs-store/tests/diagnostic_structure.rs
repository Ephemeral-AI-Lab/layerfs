//! Control preserves confirmed header corruption despite schema-open refusal.
use layerfs_store::{
    diagnostic::{self, Options, Outcome},
    identity::{Identity, Kind},
};

/// A private (`0700`) scratch directory for one Store file: the kernel guard
/// admits a Store's own directory only, as in a home.
fn scratch(label: &str) -> std::path::PathBuf {
    use std::os::unix::fs::DirBuilderExt;
    let directory =
        std::env::temp_dir().join(format!("layerfs-doctor-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .unwrap();
    directory
}

#[test]
fn selected_malformed_store_has_structural_failure_and_incomplete_access() {
    let path = scratch("header").join("store.sqlite");
    std::fs::write(&path, vec![0xA5; 8192]).unwrap();
    let before = std::fs::read(&path).unwrap();
    let mut observations = Vec::new();
    let result = diagnostic::sqlite(
        &path,
        &Identity {
            kind: Kind::Sqlite,
            binding: b"raw-fixture".to_vec(),
            cursor_key: [71; 32],
        },
        Options::default(),
        &mut |o| observations.push(o),
    );
    assert!(result.is_err());
    assert!(
        observations
            .iter()
            .any(|o| o.id == "store.database.structure" && o.outcome == Outcome::Fail),
        "{observations:?}"
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn binding_mismatch_keeps_logical_access_unavailable_without_corruption() {
    use layerfs_persistence::{PersistenceConfig, SqliteHandles, SqlitePersistenceProfile};
    use layerfs_storage::StoragePolicy;
    let path = scratch("binding").join("store.sqlite");
    let handles = SqliteHandles::create(
        PersistenceConfig::sqlite(&path).with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        StoragePolicy::frozen_default(),
        &layerfs_history::HistoryCatalogConfig {
            binding_key: b"correct-binding".to_vec(),
            cursor_key: [71; 32],
            incarnation: 7,
        },
    )
    .unwrap();
    handles.seal().unwrap();
    let before = std::fs::read(&path).unwrap();
    let mut observations = Vec::new();
    let result = diagnostic::sqlite(
        &path,
        &Identity {
            kind: Kind::Sqlite,
            binding: b"wrong-binding".to_vec(),
            cursor_key: [71; 32],
        },
        Options::default(),
        &mut |o| observations.push(o),
    );
    assert!(result.is_err());
    assert!(
        !observations.iter().any(|o| o.outcome == Outcome::Fail),
        "{observations:?}"
    );
    assert!(
        observations
            .iter()
            .any(|o| o.id == "store.database.structure"
                && o.outcome == Outcome::Pass
                && o.message.contains("separate")),
        "{observations:?}"
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}
