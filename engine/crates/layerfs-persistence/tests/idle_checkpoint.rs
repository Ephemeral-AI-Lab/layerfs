//! #86: the idle checkpoint of a live shared session. One attempt of
//! `wal_checkpoint(TRUNCATE)` empties the write-ahead log while the Store
//! stays open; one that cannot complete now is left, never retried.
#![cfg(any(target_os = "macos", target_os = "linux"))]
mod support;

use layerfs_content::{construct_bytes, read_range, ObjectId};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{PersistenceConfig, SqliteHandles, SqlitePersistenceProfile, StoreFile};
use layerfs_storage::{location::PackInfo, port::*, Storage, StoragePolicy};
use layerfs_telemetry::timer::Timing;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;
use support::Temp;

const BINDING: &[u8] = b"idle-checkpoint";
const CURSOR: [u8; 32] = [86; 32];

fn config(path: &Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Disposable)
}

fn create(path: &Path) -> SqliteHandles {
    SqliteHandles::create(
        config(path),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: CURSOR,
            incarnation: 1,
        },
    )
    .unwrap()
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

fn content(seed: u32) -> Vec<u8> {
    (0..40_000u32)
        .map(|i| (i.wrapping_mul(seed + 7) % 251) as u8)
        .collect()
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn wal_bytes(path: &Path) -> u64 {
    std::fs::metadata(sidecar(path, "-wal")).map_or(0, |m| m.len())
}

/// Statements this session has executed so far.
fn statements(handles: &SqliteHandles) -> u64 {
    handles.diagnostics().unwrap().statements
}

#[test]
fn last_release_truncates_the_log_and_the_file_equals_its_closed_copy() {
    let temp = Temp::new("idle-checkpoint-truncate");
    let path = temp.join("store.sqlite");
    let handles = create(&path);
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let ids: Vec<_> = (0..4).map(|seed| save(&storage, &content(seed))).collect();
    assert!(wal_bytes(&path) > 0, "saves leave frames in the log");

    let before = statements(&handles);
    assert!(handles.storage.checkpoint().unwrap());
    assert_eq!(statements(&handles) - before, 1, "one attempted statement");
    assert_eq!(wal_bytes(&path), 0);
    // still open and serving: a read on the same session sees every object
    for (seed, id) in ids.iter().enumerate() {
        assert_eq!(read(&storage, *id, 40_000), content(seed as u32));
    }
    // an empty log needs no statement at all
    let before = statements(&handles);
    assert!(handles.storage.checkpoint().unwrap());
    assert_eq!(statements(&handles), before);

    // The file at quiesce is exactly the closed Store: closing the last
    // connection has nothing left to copy in.
    let quiesced = std::fs::read(&path).unwrap();
    drop(storage);
    drop(handles);
    assert!(!sidecar(&path, "-wal").exists());
    assert_eq!(std::fs::read(&path).unwrap(), quiesced);
    let reopened = SqliteHandles::open_read_only(config(&path), BINDING, CURSOR).unwrap();
    let storage = Storage::new(reopened.storage.clone()).unwrap();
    for (seed, id) in ids.iter().enumerate() {
        assert_eq!(read(&storage, *id, 40_000), content(seed as u32));
    }
}

#[test]
fn a_reader_in_the_log_leaves_it_for_the_last_close() {
    let temp = Temp::new("idle-checkpoint-reader");
    let path = temp.join("store.sqlite");
    let handles = create(&path);
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let kept = save(&storage, &content(1));
    // another opener (a daemon) reads a snapshot that is in the log
    let reader = rusqlite::Connection::open_with_flags(
        &path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .unwrap();
    reader.execute_batch("BEGIN").unwrap();
    let _: i64 = reader
        .query_row("SELECT count(*) FROM sqlite_master", [], |row| row.get(0))
        .unwrap();
    let later = save(&storage, &content(2));
    let logged = wal_bytes(&path);
    assert!(logged > 0);

    let before = statements(&handles);
    assert!(!handles.storage.checkpoint().unwrap(), "busy: left");
    assert_eq!(statements(&handles) - before, 1, "attempted once, no retry");
    assert_eq!(wal_bytes(&path), logged, "the log is not truncated");
    assert_eq!(read(&storage, kept, 40_000), content(1));
    assert_eq!(read(&storage, later, 40_000), content(2));

    // the reader ends; a later release (the test's own second call) empties it
    reader.execute_batch("COMMIT").unwrap();
    drop(reader);
    assert!(handles.storage.checkpoint().unwrap());
    assert_eq!(wal_bytes(&path), 0);
}

#[cfg(target_os = "linux")]
#[test]
fn another_process_writer_lane_leaves_it_without_waiting() {
    use nix::fcntl::{Flock, FlockArg};
    let temp = Temp::new("idle-checkpoint-lane");
    let path = temp.join("store.sqlite");
    let handles = create(&path);
    let storage = Storage::new(handles.storage.clone()).unwrap();
    save(&storage, &content(3));
    let logged = wal_bytes(&path);
    assert!(logged > 0);
    // a writer of another process holds the directory lane (its own
    // description of the directory conflicts as another process's would)
    let lane = Flock::lock(
        std::fs::File::open(temp.path()).unwrap(),
        FlockArg::LockExclusive,
    )
    .unwrap();
    let before = statements(&handles);
    let started = std::time::Instant::now();
    assert!(!handles.storage.checkpoint().unwrap(), "lane taken: left");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "it did not wait"
    );
    assert_eq!(statements(&handles), before, "no statement was attempted");
    assert_eq!(wal_bytes(&path), logged);
    drop(lane);
    assert!(handles.storage.checkpoint().unwrap());
    assert_eq!(wal_bytes(&path), 0);
}

struct HeldPlan {
    entered: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
}
impl PackReadPlan for HeldPlan {
    fn select(&mut self, _: PackInfo, _: &[u8]) -> Result<PackReadChoice, PersistenceError> {
        self.entered
            .send(())
            .map_err(|e| PersistenceError::Refused {
                status: e.to_string(),
            })?;
        self.release
            .recv_timeout(Duration::from_secs(10))
            .map_err(|e| PersistenceError::Refused {
                status: e.to_string(),
            })?;
        Ok(PackReadChoice::Whole)
    }
}

#[test]
fn a_checkpoint_racing_calls_and_a_new_opener_loses_nothing() {
    let temp = Temp::new("idle-checkpoint-race");
    let path = temp.join("store.sqlite");
    let handles = create(&path);
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let first = save(&storage, &content(0));
    let object = {
        let mut locations = Vec::new();
        handles.storage.locate(&[first], &mut locations).unwrap();
        locations[0].pack.pack_id
    };

    // 1. A call holding the connection's turn: the checkpoint queues behind
    // it (first come, first served) and runs once it returns.
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let call = {
        let provider = handles.storage.clone();
        std::thread::spawn(move || {
            provider
                .read_pack_selection(
                    object,
                    &mut HeldPlan {
                        entered: entered_tx,
                        release: release_rx,
                    },
                )
                .map(|_| ())
        })
    };
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let (done_tx, done_rx) = mpsc::channel();
    let checkpoint = {
        let provider = handles.storage.clone();
        std::thread::spawn(move || done_tx.send(provider.checkpoint()).unwrap())
    };
    assert!(done_rx.recv_timeout(Duration::from_millis(100)).is_err());
    release_tx.send(()).unwrap();
    assert!(call.join().unwrap().is_ok(), "the call is not lost");
    assert!(done_rx
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap());
    checkpoint.join().unwrap();
    assert_eq!(wal_bytes(&path), 0);

    // 2. Saves on the shared session while checkpoints run alongside: every
    // save lands, and each checkpoint is one definite answer.
    let saver = {
        let provider = handles.storage.clone();
        std::thread::spawn(move || {
            let storage = Storage::new(provider).unwrap();
            (1..=12)
                .map(|seed| save(&storage, &content(seed)))
                .collect::<Vec<_>>()
        })
    };
    let mut answers = Vec::new();
    while !saver.is_finished() && answers.len() < 2000 {
        answers.push(handles.storage.checkpoint().unwrap());
    }
    let ids = saver.join().unwrap();

    // 3. A new opener arrives while a checkpoint runs.
    let opener = {
        let path = path.clone();
        std::thread::spawn(move || {
            let opened = SqliteHandles::open_read_only(config(&path), BINDING, CURSOR).unwrap();
            let storage = Storage::new(opened.storage.clone()).unwrap();
            read(&storage, first, 40_000)
        })
    };
    let last = handles.storage.checkpoint().unwrap();
    assert_eq!(opener.join().unwrap(), content(0));
    assert!(last || wal_bytes(&path) > 0);
    for (index, id) in ids.iter().enumerate() {
        assert_eq!(read(&storage, *id, 40_000), content(index as u32 + 1));
    }
    assert!(handles.storage.checkpoint().unwrap());
    assert_eq!(wal_bytes(&path), 0);
}
