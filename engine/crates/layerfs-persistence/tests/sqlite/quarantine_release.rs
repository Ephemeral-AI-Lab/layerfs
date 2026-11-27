//! A Session quarantined by an uncertain write outcome lets the Store's
//! writer lane go (finding #2). Before the fix the quarantined Session kept
//! the directory `flock`, and with it its open SQLite write transaction, until
//! the Session dropped, so every other Session and process waited forever.
//!
//! The uncertain outcome comes from ordinary public behaviour: an
//! uncoordinated raw SQLite connection adds a trigger that calls an unknown
//! function, so the Store's own `UPDATE store_policy` fails with a generic
//! `SQLITE_ERROR` inside its write transaction, which the Store classes as an
//! unknown outcome and which leaves SQLite's transaction open.
#![cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "../support/mod.rs"]
mod support;
use layerfs_history::{HistoryCatalog, HistoryCatalogConfig};
use layerfs_persistence::{PersistenceConfig, SqliteHandles, SqlitePersistenceProfile};
use layerfs_storage::{port::*, StoragePolicy};
use std::{
    path::Path,
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

/// How long the second Session may wait for the lane once the first one is
/// quarantined; the fixed path takes it at once.
const BOUND: Duration = Duration::from_secs(10);

fn authority() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: b"sqlite-quarantine-release".to_vec(),
        cursor_key: [83; 32],
        incarnation: 1,
    }
}
fn config(path: &Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Normal)
}
fn reserve() -> Reserve {
    Reserve {
        packs: 1,
        ordinals: 0,
    }
}

/// One bounded raw SQLite statement from another process, with a 3 s busy
/// wait of its own. `Ok(())` when it ran, `Err(message)` when SQLite refused
/// it (for example `database is locked` while a write transaction is open).
fn raw(path: &Path, sql: &str) -> Result<(), String> {
    let script = "import sqlite3,sys\ntry:\n c=sqlite3.connect(sys.argv[1],timeout=3,isolation_level=None)\n c.execute(sys.argv[2])\n c.close()\n print('ok')\nexcept Exception as e:\n print('refused',e)\n";
    let mut child = Command::new("python3")
        .args(["-I", "-c", script])
        .arg(path)
        .arg(sql)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= until {
            let _ = child.kill();
            let _ = child.wait();
            panic!("bounded raw SQLite statement");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    if text == "ok\n" {
        Ok(())
    } else {
        Err(text)
    }
}

const ADD_TRIGGER: &str = "CREATE TRIGGER quarantine_probe BEFORE UPDATE ON store_policy \
                           BEGIN SELECT layerfs_no_such_function(); END";
const DROP_TRIGGER: &str = "DROP TRIGGER quarantine_probe";

/// The second Session's reservation, run on its own thread and waited for
/// within [`BOUND`]. A reservation still waiting at the bound is the bug.
fn reserve_within_bound(second: &SqliteHandles) -> Reserved {
    // A detached thread, so a reservation stuck in the kernel wait cannot
    // hold the test past its bound; process exit ends it.
    let storage = second.storage.clone();
    let (done, finished) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = done.send(storage.reserve(reserve()));
    });
    finished
        .recv_timeout(BOUND)
        .unwrap_or_else(|_| {
            panic!("the second Session still waits for the writer lane after {BOUND:?}")
        })
        .expect("the second Session reserves")
}

struct Stores {
    _temp: support::Temp,
    path: std::path::PathBuf,
    first: SqliteHandles,
    second: SqliteHandles,
    /// The pack id the next successful reservation gets.
    next_pack: i64,
}

fn stores(name: &str) -> Stores {
    let temp = support::Temp::new(name);
    let path = temp.join("store.sqlite");
    let first = SqliteHandles::create(config(&path), StoragePolicy::frozen_default(), &authority())
        .unwrap();
    let second = SqliteHandles::open_writable(
        config(&path),
        &authority().binding_key,
        authority().cursor_key,
    )
    .unwrap();
    first.storage.reserve(reserve()).unwrap();
    let last = second.storage.reserve(reserve()).unwrap().first_pack_id;
    raw(&path, ADD_TRIGGER).expect("the probe trigger is added while no writer is active");
    Stores {
        _temp: temp,
        path,
        first,
        second,
        next_pack: last + 1,
    }
}

/// After the first Session's uncertain write: SQLite's transaction is gone
/// (a raw writer can drop the trigger), the second Session takes the lane
/// within the bound and gets the next pack id, and the first Session stays
/// quarantined.
fn check_released(stores: Stores) {
    raw(&stores.path, DROP_TRIGGER)
        .expect("the quarantined Session left no SQLite write transaction open");
    let reserved = reserve_within_bound(&stores.second);
    assert_eq!(reserved.first_pack_id, stores.next_pack);
    assert!(matches!(
        stores.first.storage.reserve(reserve()),
        Err(PersistenceError::Uncertain)
    ));
}

#[test]
fn a_quarantined_write_releases_the_lane_to_another_session() {
    let stores = stores("sqlite-quarantine-release-run");
    assert!(matches!(
        stores.first.storage.reserve(reserve()),
        Err(PersistenceError::Uncertain)
    ));
    check_released(stores);
}

#[test]
fn a_quarantined_write_inside_a_held_lane_closes_before_the_lane_is_released() {
    let stores = stores("sqlite-quarantine-release-lane");
    let lane = stores
        .first
        .history
        .publish_lane()
        .expect("publication lane");
    assert!(matches!(
        stores.first.storage.reserve(reserve()),
        Err(PersistenceError::Uncertain)
    ));
    drop(lane);
    check_released(stores);
}

/// #776 S4: a quarantined Session is refused the publication lane before it
/// waits for the writer lock, as every other write on it is. Waiting there
/// with its own writer still held would never end.
#[test]
fn a_quarantined_session_is_refused_the_publication_lane() {
    let stores = stores("sqlite-quarantine-lane-refused");
    assert!(matches!(
        stores.first.storage.reserve(reserve()),
        Err(PersistenceError::Uncertain)
    ));
    let refused = stores
        .first
        .history
        .publish_lane()
        .expect_err("a quarantined Session is refused the lane");
    assert!(refused.unknown(), "{refused:?}");
    check_released(stores);
}
