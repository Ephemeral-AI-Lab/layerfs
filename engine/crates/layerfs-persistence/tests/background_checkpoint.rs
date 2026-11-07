//! A Disposable session's checkpoint thread (owner approval, 2026-11-07):
//! COMMIT does not checkpoint (`wal_autocheckpoint=0`); a log longer than
//! 4 MiB is copied into the Store file while the session stays open, with
//! no idle checkpoint, seal or close.
#![cfg(any(target_os = "macos", target_os = "linux"))]
mod support;

use layerfs_content::{construct_bytes, read_range};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{PersistenceConfig, SqliteHandles, SqlitePersistenceProfile};
use layerfs_storage::{Storage, StoragePolicy};
use layerfs_telemetry::timer::Timing;
use std::path::Path;
use std::time::{Duration, Instant};
use support::Temp;

const BYTES: usize = 24 << 20;

fn create(path: &Path) -> SqliteHandles {
    SqliteHandles::create(
        PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: b"background-checkpoint".to_vec(),
            cursor_key: [47; 32],
            incarnation: 1,
        },
    )
    .unwrap()
}

/// Incompressible bytes, so the Store file must grow by about their length.
fn random(len: usize) -> Vec<u8> {
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        out.extend_from_slice(&x.to_le_bytes());
    }
    out.truncate(len);
    out
}

fn size(path: &Path) -> u64 {
    std::fs::metadata(path).map_or(0, |m| m.len())
}

#[test]
fn a_long_log_is_copied_into_the_store_while_the_session_stays_open() {
    let temp = Temp::new("background-checkpoint");
    let path = temp.join("store.sqlite");
    let handles = create(&path);
    assert_eq!(handles.profile().wal_autocheckpoint, 0);
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let bytes = random(BYTES);
    let save = storage.begin_save().unwrap();
    let root = Timing::disabled("construct", |scope| {
        construct_bytes(
            storage.policy().construction(),
            &storage.policy().construction().capacities(),
            &bytes,
            &mut save.sink(),
            scope.child("content"),
        )
    })
    .0
    .unwrap()
    .root;
    save.finish().unwrap();
    // Readiness wait, bounded: the thread copies the log while nothing else
    // runs; no idle checkpoint, seal or close is called here.
    let deadline = Instant::now() + Duration::from_secs(10);
    while size(&path) < BYTES as u64 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        size(&path) >= BYTES as u64,
        "the Store file holds the saved pages: {} bytes",
        size(&path)
    );
    let mut read = Vec::new();
    Timing::disabled("read", |scope| {
        read_range(
            &storage.reader().unwrap(),
            root,
            0..BYTES as u64,
            &mut read,
            scope.child("read"),
        )
    })
    .0
    .unwrap();
    assert!(read == bytes);
    drop(storage);
    drop(handles);
    // The last close leaves no log behind.
    let mut wal = path.as_os_str().to_os_string();
    wal.push("-wal");
    assert_eq!(size(Path::new(&wal)), 0);
}
