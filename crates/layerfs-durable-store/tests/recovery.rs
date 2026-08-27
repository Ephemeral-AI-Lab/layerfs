use layerfs_durable_store::DurableStore;
use layerfs_storage::integrity::IntegrityMode;
use layerfs_working_store::WorkingStore;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn working_and_durable_are_distinct_and_backup_restores_exact_identity() {
    let base = std::env::temp_dir().join(format!(
        "layerfs-durable-recovery-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    let working =
        WorkingStore::open(&base.join("working"), IntegrityMode::TrustedLocalDev).unwrap();
    let durable = DurableStore::open(&base.join("durable")).unwrap();
    assert_ne!(working.storage_id(), durable.storage_id());
    assert_ne!(working.database_path(), durable.database_path().as_path());

    let durable_id = durable.storage_id();
    let backup = base.join("backup.sqlite");
    durable.backup(&backup).unwrap();
    drop(durable);
    let restored = DurableStore::restore(&backup, &base.join("restored")).unwrap();
    assert_eq!(restored.storage_id(), durable_id);
    assert_ne!(restored.storage_id(), working.storage_id());
    drop(restored);
    drop(working);
    fs::remove_dir_all(base).unwrap();
}

#[cfg(unix)]
#[test]
fn store_open_refuses_symlinks_without_chmod_or_initialization() {
    use std::os::unix::fs::{symlink, PermissionsExt};

    let base = std::env::temp_dir().join(format!(
        "layerfs-store-symlink-open-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&base).unwrap();
    let target = base.join("target");
    fs::create_dir(&target).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();

    let working_link = base.join("working-link");
    symlink(&target, &working_link).unwrap();
    assert!(WorkingStore::open(&working_link, IntegrityMode::Verified).is_err());
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o755
    );
    assert_eq!(fs::read_dir(&target).unwrap().count(), 0);

    let durable_link = base.join("durable-link");
    symlink(&target, &durable_link).unwrap();
    assert!(DurableStore::open(&durable_link).is_err());
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o755
    );
    assert_eq!(fs::read_dir(&target).unwrap().count(), 0);

    fs::remove_dir_all(base).unwrap();
}
