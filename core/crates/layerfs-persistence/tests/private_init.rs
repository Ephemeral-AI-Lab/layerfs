//! Explicit private acquisition and the only transition into a shared WAL Store.
mod support;
use layerfs_content::{FinalizedObject, ObjectId, ObjectRole};
use layerfs_history::{HistoryCatalog, HistoryCatalogConfig, ReserveRequest};
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::{
    port::{PackPersistence, PersistenceError, Reserve},
    Storage, StoragePolicy,
};

fn authority() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: b"private-init".to_vec(),
        incarnation: 9,
        cursor_key: [72; 32],
    }
}
fn config(path: &std::path::Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Disposable)
}
fn create(path: &std::path::Path) -> Handles {
    Handles::create_for_init(config(path), StoragePolicy::frozen_default(), &authority()).unwrap()
}
fn open(path: &std::path::Path) -> Result<Handles, PersistenceError> {
    Handles::open_writable(
        config(path),
        &authority().binding_key,
        authority().cursor_key,
    )
}

#[test]
fn private_content_and_history_become_a_single_shared_wal_file_only_at_seal() {
    let temp = support::Temp::new("private-init");
    let path = temp.join("store.sqlite");
    let handles = create(&path);
    assert!(handles.profile().private_init);
    assert_eq!(
        handles.profile().identity,
        "sqlite-private-init-memory-off-v1"
    );
    assert_eq!(
        (
            &*handles.profile().journal_mode,
            handles.profile().synchronous
        ),
        ("memory", 0)
    );
    let object = FinalizedObject::new(
        ObjectRole::FileState,
        layerfs_content::object::codec::encode_bytes_object(b"private canonical bytes").unwrap(),
    )
    .unwrap();
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    save.accept(object.clone()).unwrap();
    save.finish().unwrap();
    let request = ReserveRequest {
        scope: ObjectId::for_bytes(b"private-scope"),
        count: 7,
    };
    assert_eq!(handles.history.reserve_inodes(&request).unwrap().start, 1);
    assert!(matches!(open(&path), Err(PersistenceError::Malformed)));
    assert!(!temp.join("store.sqlite-wal").exists());
    drop(storage);
    let sealed = handles.seal().unwrap();
    assert_eq!(sealed.path, path);
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
    assert_eq!(&std::fs::read(&path).unwrap()[18..20], &[2, 2]);
    let reopened = open(&path).unwrap();
    assert!(!reopened.profile().private_init);
    assert_eq!(reopened.profile().identity, "sqlite-wal-off-v2");
    assert_eq!(reopened.profile().journal_mode, "wal");
    let storage = Storage::new(reopened.storage.clone()).unwrap();
    assert_eq!(
        storage
            .reader()
            .unwrap()
            .read_objects(&[object.id()])
            .unwrap(),
        vec![object.canonical().to_vec()]
    );
    assert_eq!(reopened.history.reserve_inodes(&request).unwrap().start, 8);
    drop(storage);
    reopened.seal().unwrap();
}

#[test]
fn retained_provider_refuses_before_private_conversion_and_keeps_its_session() {
    let temp = support::Temp::new("private-owner");
    let path = temp.join("store.sqlite");
    let handles = create(&path);
    let provider = handles.storage.clone();
    let before = std::fs::read(&path).unwrap();
    assert!(matches!(handles.seal(), Err(PersistenceError::Busy)));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(!temp.join("store.sqlite-wal").exists());
    provider
        .reserve(Reserve {
            packs: 1,
            ordinals: 0,
        })
        .unwrap();
    drop(provider);
    assert!(matches!(open(&path), Err(PersistenceError::Malformed)));
}

#[test]
fn dropping_private_init_never_promotes_the_file() {
    let temp = support::Temp::new("private-drop");
    let path = temp.join("store.sqlite");
    drop(create(&path));
    assert!(matches!(open(&path), Err(PersistenceError::Malformed)));
    assert_eq!(&std::fs::read(&path).unwrap()[18..20], &[1, 1]);
    assert!(!temp.join("store.sqlite-wal").exists());
}
