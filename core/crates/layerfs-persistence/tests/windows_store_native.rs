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

/// The lane's dead-holder path: a thread that ends while owning the named
/// mutex leaves it abandoned, and the kernel hands ownership to the next
/// waiter. The abandoned state is produced deterministically here - a
/// thread waits on the product's own mutex name and returns without
/// releasing - and the Store must still open and write through that
/// `WAIT_ABANDONED` acquisition.
#[test]
fn a_dead_lane_holder_does_not_block_the_next_writer() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let temp = support::Temp::new("windows-store-abandoned");
    let path = temp.join("store.sqlite");
    let catalog = authority(b"windows-store-abandoned");
    let seed = Handles::create(config(&path), StoragePolicy::frozen_default(), &catalog).unwrap();
    seed.storage.reserve(reserve()).unwrap();
    drop(seed);

    let directory = temp.path().to_path_buf();
    let holder = std::thread::spawn(move || {
        // The product's own lane name, derived exactly as windows_lane does.
        let mut hasher = DefaultHasher::new();
        match std::fs::canonicalize(&directory) {
            Ok(canonical) => canonical.hash(&mut hasher),
            Err(_) => directory.hash(&mut hasher),
        }
        let name: Vec<u16> = format!("Local\\layerfs-writer-lane-{:016x}", hasher.finish())
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let handle = unsafe {
            windows_sys::Win32::System::Threading::CreateMutexW(
                std::ptr::null(),
                0,
                name.as_ptr(),
            )
        };
        assert!(!handle.is_null());
        let waited = unsafe { windows_sys::Win32::System::Threading::WaitForSingleObject(handle, windows_sys::Win32::System::Threading::INFINITE) };
        assert_eq!(waited, windows_sys::Win32::Foundation::WAIT_OBJECT_0);
        // The thread owns the mutex and now ends without ReleaseMutex: the
        // kernel marks it abandoned for the next waiter.
    });
    holder.join().unwrap();

    let reopened = Handles::open_writable(config(&path), &catalog.binding_key, catalog.cursor_key)
        .expect("an abandoned lane is taken by the next writer");
    reopened.storage.reserve(reserve()).unwrap();
    drop(reopened);
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
