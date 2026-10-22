//! Direct Store access is refused `Unsupported` where the Store's locks do
//! not reach every opener (#352 F-020): on macOS, and on Linux for a
//! filesystem outside the allowlist (here tmpfs). The refusal comes before any
//! file is opened: the Store file's bytes are unchanged and no overlay
//! template is created.
use layerfs_store_service::{OpenRefusal, StoreService};
use std::path::{Path, PathBuf};

#[cfg(not(target_os = "macos"))]
fn refused(directory: &Path) -> (String, Vec<u8>, Vec<u8>) {
    let store = directory.join("store.sqlite");
    std::fs::write(&store, b"not opened").unwrap();
    let template = directory.join("overlay.sqlite");
    let before = std::fs::read(&store).unwrap();
    let outcome = StoreService::open_direct(&store, b"binding", [7; 32], &template);
    let reason = match outcome {
        Err(OpenRefusal::Unsupported(reason)) => reason.0,
        Err(other) => panic!("expected Unsupported, got {other:?}"),
        Ok(_) => panic!("expected Unsupported, the Store opened"),
    };
    assert!(
        !template.exists(),
        "no overlay template before the guard passes"
    );
    (reason, before, std::fs::read(&store).unwrap())
}

fn scratch(parent: &Path, label: &str) -> PathBuf {
    let directory = parent.join(format!("layerfs-guard-{label}-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
#[test]
fn direct_access_is_unsupported_off_linux() {
    let directory = scratch(&std::env::temp_dir(), "host");
    let (reason, before, after) = refused(&directory);
    assert!(reason.contains("Linux"), "{reason}");
    assert_eq!(before, after);
    std::fs::remove_dir_all(directory).unwrap();
}

#[cfg(target_os = "linux")]
#[test]
fn direct_access_is_unsupported_on_a_filesystem_outside_the_allowlist() {
    let directory = scratch(Path::new("/dev/shm"), "tmpfs");
    let (reason, before, after) = refused(&directory);
    assert!(reason.contains("ext4, xfs or btrfs"), "{reason}");
    assert_eq!(before, after);
    std::fs::remove_dir_all(directory).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
fn direct_access_succeeds_on_local_apfs_and_refuses_non_apfs() {
    use layerfs_history::HistoryCatalogConfig;
    use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
    use layerfs_storage::StoragePolicy;

    let directory = scratch(&std::env::temp_dir(), "apfs");
    let store = directory.join("store.sqlite");
    let template = directory.join("overlay.sqlite");
    assert_eq!(layerfs_store_service::kernel_guard::check(&store), Ok("apfs"));

    let dev_template = directory.join("dev-overlay.sqlite");
    let outcome = StoreService::open_direct(
        Path::new("/dev/store.sqlite"),
        b"binding",
        [7; 32],
        &dev_template,
    );
    let reason = match outcome {
        Err(OpenRefusal::Unsupported(reason)) => reason.0,
        Err(other) => panic!("expected Unsupported on devfs, got {other:?}"),
        Ok(_) => panic!("expected Unsupported on devfs, the Store opened"),
    };
    assert!(reason.contains("apfs"), "{reason}");
    assert!(!dev_template.exists());

    let authority = HistoryCatalogConfig {
        binding_key: b"binding".to_vec(),
        cursor_key: [7; 32],
        incarnation: 1,
    };
    Handles::create(
        PersistenceConfig::sqlite(&store).with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        StoragePolicy::frozen_default(),
        &authority,
    )
    .unwrap()
    .seal()
    .unwrap();

    let service = StoreService::open_direct(&store, b"binding", [7; 32], &template).unwrap();
    assert!(template.exists());
    service.close().unwrap();
    std::fs::remove_dir_all(directory).unwrap();
}
