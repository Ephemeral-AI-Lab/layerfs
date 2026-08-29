use layerfs_storage::{StorageError, StoreDb, StoreRole};

#[test]
fn live_owner_is_exclusive_and_dead_owner_is_reclaimed() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-owner-recovery-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let path = root.join("store.sqlite");
    let owner_path = root.join("store.sqlite.owner");
    let store = StoreDb::create(&path, StoreRole::Branch).unwrap();
    assert_eq!(
        std::fs::read_to_string(&owner_path).unwrap().trim(),
        std::process::id().to_string()
    );
    assert!(matches!(
        StoreDb::connect(&path, StoreRole::Branch),
        Err(StorageError::StoreBusy)
    ));
    drop(store);
    std::fs::write(&owner_path, format!("{}\n", u32::MAX)).unwrap();
    drop(StoreDb::connect(&path, StoreRole::Branch).unwrap());
    std::fs::remove_dir_all(root).unwrap();
}
