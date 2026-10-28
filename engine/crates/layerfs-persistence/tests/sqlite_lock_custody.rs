//! Cooperative readiness must preserve SQLite's own POSIX lock custody.
//! Uses public Stores and an independently bounded raw SQLite writer process.
#![cfg(target_os = "linux")]
mod support;
use layerfs_content::{FinalizedObject, ObjectRole};
use layerfs_history::HistoryCatalogConfig;
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::{port::*, Storage, StoragePolicy};
use std::{
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

struct RawWriter(Child);
impl RawWriter {
    fn acquire(path: &Path) -> Self {
        let script = "import sqlite3,sys,select\nc=sqlite3.connect(sys.argv[1],timeout=0)\nc.execute('BEGIN IMMEDIATE')\nprint('held',flush=True)\nselect.select([sys.stdin],[],[],5)\nc.rollback()\nc.close()\n";
        let child = Command::new("python3")
            .args(["-c", script])
            .arg(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut held = Self(child);
        let mut output = BufReader::new(held.0.stdout.take().unwrap());
        let (tx, rx) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut line = String::new();
            let _ = tx.send(output.read_line(&mut line).map(|_| line));
        });
        let ready = rx.recv_timeout(Duration::from_secs(3)).unwrap().unwrap();
        let until = Instant::now() + Duration::from_secs(3);
        while !reader.is_finished() {
            assert!(Instant::now() < until, "bounded raw writer readiness");
            std::thread::yield_now();
        }
        reader.join().unwrap();
        assert_eq!(ready, "held\n");
        held
    }
    fn release(mut self) {
        self.0
            .stdin
            .take()
            .unwrap()
            .write_all(b"release\n")
            .unwrap();
        let until = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(status) = self.0.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < until, "bounded raw writer exit");
            std::thread::yield_now();
        }
    }
}
impl Drop for RawWriter {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
fn config(path: &Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Disposable)
}
fn reserve() -> Reserve {
    Reserve {
        packs: 1,
        ordinals: 1,
    }
}
fn main_shared_lock(path: &Path) -> bool {
    // SQLite's unix SHARED range is 510 bytes at PENDING_BYTE+2. A
    // different process can acquire its exclusive fcntl lock only when the
    // live SQLite connection's SHARED ownership has disappeared.
    let script = "import os,fcntl,sys\nf=os.open(sys.argv[1],os.O_RDWR)\ntry:\n fcntl.lockf(f,fcntl.LOCK_EX|fcntl.LOCK_NB,510,1073741826,os.SEEK_SET)\n print('unlocked')\nexcept BlockingIOError:\n print('shared')\nfinally:\n os.close(f)\n";
    let mut child = Command::new("python3")
        .args(["-c", script])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let until = Instant::now() + Duration::from_secs(3);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= until {
            let _ = child.kill();
            let _ = child.wait();
            panic!("bounded independent main-lock observation");
        }
        std::thread::yield_now();
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    match output.stdout.as_slice() {
        b"shared\n" => true,
        b"unlocked\n" => false,
        other => panic!("unexpected lock observation: {other:?}"),
    }
}
fn check(drop_peer: bool) {
    let temp = support::Temp::new("sqlite-lock-custody");
    let path = temp.join("store.sqlite");
    let authority = HistoryCatalogConfig {
        binding_key: b"sqlite-lock-custody".to_vec(),
        cursor_key: [67; 32],
        incarnation: 1,
    };
    let first =
        Handles::create(config(&path), StoragePolicy::frozen_default(), &authority).unwrap();
    let mut second = Some(
        Handles::open_writable(config(&path), &authority.binding_key, authority.cursor_key)
            .unwrap(),
    );
    // Each successful cooperative transaction acquires and releases its own
    // real advisory owner. Closing a non-SQLite FD to the main inode here must
    // not erase SQLite's SHARED locks and let a new process reset the live WAL.
    first.storage.reserve(reserve()).unwrap();
    second.as_ref().unwrap().storage.reserve(reserve()).unwrap();
    let storage = Storage::new(first.storage.clone()).unwrap();
    let object = FinalizedObject::new(
        ObjectRole::FileState,
        layerfs_content::object::codec::encode_bytes_object(&vec![47; 32000]).unwrap(),
    )
    .unwrap();
    let save = storage.begin_save().unwrap();
    save.accept(object).unwrap();
    save.finish().unwrap();
    if drop_peer {
        drop(second.take());
    }
    let main_shared = main_shared_lock(&path);
    let held = RawWriter::acquire(&path);
    let before = first.diagnostics().unwrap();
    let result = first.storage.reserve(reserve());
    let after = first.diagnostics().unwrap();
    held.release();
    println!("SQLITE_LOCK_CUSTODY peer_dropped={drop_peer} main_shared={main_shared} busy={} begin_statements={} write_transactions={} write_commits={} rollbacks={}", matches!(result, Err(PersistenceError::Busy)), after.statements-before.statements, after.write_transactions-before.write_transactions, after.write_commits-before.write_commits, after.rollbacks-before.rollbacks);
    assert!(
        matches!(result, Err(PersistenceError::Busy)),
        "raw SQLite writer was bypassed after cooperative writes: {result:?}"
    );
    assert!(
        main_shared,
        "cooperative FD close erased SQLite main-file SHARED ownership"
    );
    assert_eq!(after.statements - before.statements, 1);
    assert_eq!(after.write_transactions, before.write_transactions);
    assert_eq!(after.write_commits, before.write_commits);
    assert_eq!(after.rollbacks, before.rollbacks);
    // This is a distinct later request after the definite Busy, not a replay.
    first.storage.reserve(reserve()).unwrap();
}
#[test]
fn cooperative_release_preserves_sqlite_locks_with_two_live_sessions() {
    check(false);
}
#[test]
fn dropping_one_session_preserves_the_other_sessions_sqlite_locks() {
    check(true);
}
