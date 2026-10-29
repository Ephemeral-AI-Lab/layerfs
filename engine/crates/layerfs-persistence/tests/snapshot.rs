//! #506: a live Store copies to one consistent closed file that opens as a Store.
#![cfg(any(target_os = "macos", target_os = "linux"))]
mod support;

use layerfs_content::{construct_bytes, read_range, ObjectId};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{
    Handles, PersistenceConfig, SqliteAcquisitionSchema, SqlitePackLayout, SqlitePersistenceProfile,
};
use layerfs_storage::{Storage, StoragePolicy};
use layerfs_telemetry::timer::Timing;
use support::Temp;

const BINDING: &[u8] = b"snapshot-binding";
const CURSOR: [u8; 32] = [23; 32];

fn config(path: std::path::PathBuf) -> PersistenceConfig {
    PersistenceConfig::sqlite(path)
        .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
        .with_sqlite_pack_layout(SqlitePackLayout::GroupRowsIndexed)
        .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables)
}

fn save(storage: &Storage, bytes: &[u8]) -> ObjectId {
    let save = storage.begin_save().unwrap();
    let root = Timing::disabled("construct", |scope| {
        construct_bytes(
            storage.policy().construction(),
            &storage.policy().construction().capacities(),
            bytes,
            &mut save.sink(),
            scope.child("content"),
        )
    })
    .0
    .unwrap()
    .root;
    save.finish().unwrap();
    root
}

fn read(storage: &Storage, id: ObjectId, len: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    Timing::disabled("read", |scope| {
        read_range(
            &storage.reader().unwrap(),
            id,
            0..len,
            &mut bytes,
            scope.child("read"),
        )
    })
    .0
    .unwrap();
    bytes
}

#[test]
fn live_store_snapshot_opens_with_exactly_the_saved_state() {
    let temp = Temp::new("snapshot");
    let handles = Handles::create(
        config(temp.join("store")),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: CURSOR,
            incarnation: 3,
        },
    )
    .unwrap();
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let before: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
    let kept = save(&storage, &before);

    let snapshot = handles.snapshot(&temp.join("copy")).unwrap();
    assert_eq!(snapshot.path, temp.join("copy"));
    assert_eq!(snapshot.profile, SqlitePersistenceProfile::Disposable);
    assert_eq!(
        snapshot.bytes,
        std::fs::metadata(temp.join("copy")).unwrap().len()
    );
    for suffix in ["-wal", "-shm", "-journal"] {
        assert!(!temp.join(&format!("copy{suffix}")).exists(), "{suffix}");
    }
    // The live Store keeps serving writes after the copy; the copy does not see them.
    let later = save(&storage, b"written after the snapshot");
    assert_eq!(read(&storage, later, 26), b"written after the snapshot");
    // An existing destination is refused, never overwritten.
    assert!(handles.snapshot(&temp.join("copy")).is_err());

    let copy = Handles::open_writable(config(temp.join("copy")), BINDING, CURSOR).unwrap();
    let copied = Storage::new(copy.storage.clone()).unwrap();
    assert_eq!(read(&copied, kept, before.len() as u64), before);
    assert!(copied.reader().unwrap().read_objects(&[later]).is_err());
    assert_eq!(copy.profile().pack_layout, handles.profile().pack_layout);
    assert_eq!(copy.profile().acquisition, handles.profile().acquisition);
}
