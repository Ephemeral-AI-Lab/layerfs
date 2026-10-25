//! The Windows-native Store route: creation and two concurrent sessions in
//! one process, then two foreign processes serialized through the writer
//! lane, all through public Stores and bounded waits.
#![cfg(windows)]
mod support;
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::{port::*, StoragePolicy};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const CHILD_STORE: &str = "LAYERFS_WINDOWS_STORE_CHILD_STORE";
const CHILD_BINDING: &str = "LAYERFS_WINDOWS_STORE_CHILD_BINDING";

fn config(path: &Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Disposable)
}

fn reserve() -> Reserve {
    Reserve {
        packs: 1,
        ordinals: 1,
    }
}

fn authority(name: &[u8]) -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: name.to_vec(),
        cursor_key: [67; 32],
        incarnation: 1,
    }
}

/// One child process's whole work: open the Store writable and write once.
fn child(path: PathBuf, binding: Vec<u8>) {
    let opened = Handles::open_writable(config(&path), &binding, [67; 32])
        .expect("the child opens the Store");
    opened.storage.reserve(reserve()).expect("the child writes");
    // A clean close: the child's Store is not crashed, so the sibling's
    // next open does not pay WAL recovery beside the lane under test.
    opened.shutdown();
}

/// When this binary was re-executed as a lane child, run its one job and
/// leave; otherwise this is the parent test run and work continues below.
fn dispatch_child() {
    let Some(path) = std::env::var_os(CHILD_STORE) else {
        return;
    };
    let binding = std::env::var(CHILD_BINDING)
        .expect("the child binding")
        .into_bytes();
    child(PathBuf::from(path), binding);
    std::process::exit(0);
}

/// Bounded completion of one foreign child; a hung child fails the test.
/// A failed child's own report (the harness prints it to its stdout) is
/// carried into the failure instead of being discarded.
fn finish(mut child: std::process::Child) {
    let until = Instant::now() + Duration::from_secs(60);
    loop {
        match child.try_wait().expect("child poll") {
            Some(status) => {
                assert!(
                    status.success(),
                    "a child writer failed: {status}; child report: {:?}",
                    String::from_utf8_lossy(&child.wait_with_output().expect("child drain").stdout)
                );
                return;
            }
            None => assert!(Instant::now() < until, "a child writer is hung"),
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn windows_native_store_serves_sessions_and_serializes_foreign_processes() {    dispatch_child();
    let temp = support::Temp::new("windows-store-native");
    let path = temp.join("store.sqlite");
    let catalog = authority(b"windows-store-native");
    let first = Handles::create(config(&path), StoragePolicy::frozen_default(), &catalog).unwrap();
    let second =
        Handles::open_writable(config(&path), &catalog.binding_key, catalog.cursor_key).unwrap();
    first.storage.reserve(reserve()).unwrap();
    second.storage.reserve(reserve()).unwrap();
    first.storage.reserve(reserve()).unwrap();
    second.storage.reserve(reserve()).unwrap();
    drop(first);
    drop(second);

    let lane = temp.join("lane");
    std::fs::create_dir_all(&lane).unwrap();
    let lane_path = lane.join("store.sqlite");
    let binding = b"windows-lane-processes".to_vec();
    let lane_catalog = authority(&binding);
    let seed = Handles::create(
        config(&lane_path),
        StoragePolicy::frozen_default(),
        &lane_catalog,
    )
    .unwrap();
    seed.storage.reserve(reserve()).unwrap();
    drop(seed);
    let executable = std::env::current_exe().unwrap();
    let mut children = Vec::new();
    for _ in 0..2 {
        children.push(
            Command::new(&executable)
                // The child runs only this test; without the filter the
                // harness would run every test in the binary (and their
                // dispatch guards) beside it.
                .args([
                    "--exact",
                    "windows_native_store_serves_sessions_and_serializes_foreign_processes",
                ])
                .env(CHILD_STORE, &lane_path)
                .env(CHILD_BINDING, "windows-lane-processes")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
    }
    for child in children {
        finish(child);
    }
    let reopened =
        Handles::open_writable(config(&lane_path), &binding, lane_catalog.cursor_key).unwrap();
    reopened.storage.reserve(reserve()).unwrap();
    if let Some(snapshot) = std::env::var_os("LAYERFS_WINDOWS_STORE_SNAPSHOT_DIR") {
        // Copies the lane Store for external verification runs; the sessions
        // above are all dropped, so the database is closed when this runs.
        let snapshot = PathBuf::from(snapshot);
        std::fs::create_dir_all(&snapshot).unwrap();
        std::fs::copy(&lane_path, snapshot.join("store.sqlite")).unwrap();
        for side in ["store.sqlite-wal", "store.sqlite-shm"] {
            if let Ok(bytes) = std::fs::read(lane.with_file_name(side)) {
                std::fs::write(snapshot.join(side), bytes).unwrap();
            }
        }
    }
}
