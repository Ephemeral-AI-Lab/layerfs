use layerfs_durable_store::DurableStore;
use layerfs_storage::integrity::IntegrityMode;
use layerfs_working_store::WorkingStore;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn full_durable_compaction_and_restore_preserve_identity_and_rollback() {
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
    let admitted =
        layerfs_storage::FullStorage::open_durable_verified(durable.database_path()).unwrap();
    assert_eq!(admitted.storage_id(), durable_id);
    assert_eq!(admitted.role(), layerfs_storage::StoreRole::Durable);
    drop(admitted);
    let backup = base.join("backup.sqlite");
    durable.backup(&backup).unwrap();
    let prior = durable.database_path();
    let durable = durable.compact().unwrap();
    assert_eq!(durable.storage_id(), durable_id);
    assert_ne!(durable.database_path(), prior);
    assert!(prior.exists());
    let generation_root = base.join("durable/durable.sqlite.generations");
    let current = layerfs_storage::generation::StoreSelector::decode(
        &fs::read(generation_root.join("CURRENT")).unwrap(),
    )
    .unwrap();
    let rollback = layerfs_storage::generation::StoreSelector::decode(
        &fs::read(generation_root.join("ROLLBACK")).unwrap(),
    )
    .unwrap();
    assert_eq!(rollback.generation + 1, current.generation);
    assert_eq!(rollback.store_id, current.store_id);
    drop(durable);
    let backup = layerfs_storage::FullStorage::open_durable_verified(&backup).unwrap();
    assert_eq!(backup.storage_id(), durable_id);
    assert_ne!(backup.storage_id(), working.storage_id());
    drop(backup);
    let restored =
        DurableStore::restore(&base.join("backup.sqlite"), &base.join("restored")).unwrap();
    assert_eq!(restored.storage_id(), durable_id);
    assert!(restored
        .database_path()
        .ends_with("generation-0000000000000000.sqlite"));
    assert!(!base.join("restored/durable.sqlite").exists());
    drop(restored);
    drop(working);
    fs::remove_dir_all(base).unwrap();
}
