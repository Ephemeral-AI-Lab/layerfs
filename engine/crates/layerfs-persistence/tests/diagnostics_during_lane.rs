//! Reading a session's SQL counters does not wait for the publication lane
//! (#352 F-018). A traced daemon call reads the writer's counters before and
//! after its operation. When that read queued for the writer's admission, a
//! traced read waited for whole Commits holding the lane, merges included.
mod support;

use layerfs_history::{HistoryCatalog, HistoryCatalogConfig};
use layerfs_persistence::{PersistenceConfig, SqliteHandles, SqlitePersistenceProfile};
use layerfs_storage::StoragePolicy;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use support::Temp;

const HELD: Duration = Duration::from_secs(3);
const LIMIT: Duration = Duration::from_millis(750);

#[test]
fn counters_are_read_while_the_lane_is_held() {
    let temp = Temp::new("diagnostics-during-lane");
    let mut config = PersistenceConfig::sqlite(temp.join("store"));
    config.sqlite_profile = SqlitePersistenceProfile::Normal;
    let handles = SqliteHandles::create(
        config,
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: b"diagnostics".to_vec(),
            cursor_key: [7; 32],
            incarnation: 9,
        },
    )
    .unwrap();
    let took = std::thread::scope(|scope| {
        let (held, holding) = mpsc::channel();
        let history = &handles.history;
        let holder = scope.spawn(move || {
            let lane = history.publish_lane().expect("publication lane");
            held.send(()).ok();
            std::thread::sleep(HELD);
            drop(lane);
        });
        holding
            .recv_timeout(Duration::from_secs(10))
            .expect("the lane is held");
        let started = Instant::now();
        let counters = handles.storage.diagnostics();
        let took = started.elapsed();
        holder.join().expect("lane holder");
        counters.expect("writer counters");
        took
    });
    assert!(took < LIMIT, "counters waited {took:?} for the held lane");
}

/// A writer that waits for another Session's lane (on Linux and macOS, the
/// Store directory's `flock`, which separate Sessions and processes share)
/// waits outside its Session's state lock, so that Session's counters stay
/// readable meanwhile (finding #20). Before the fix the waiter held the state
/// lock across the kernel wait and `diagnostics()` queued behind it.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn counters_are_read_while_the_session_waits_for_another_sessions_lane() {
    use layerfs_storage::port::{PackPersistence, Reserve};
    let temp = Temp::new("diagnostics-during-foreign-lane");
    let path = temp.join("store");
    let config =
        PersistenceConfig::sqlite(&path).with_sqlite_profile(SqlitePersistenceProfile::Normal);
    let authority = HistoryCatalogConfig {
        binding_key: b"diagnostics-foreign".to_vec(),
        cursor_key: [9; 32],
        incarnation: 3,
    };
    let holder =
        SqliteHandles::create(config.clone(), StoragePolicy::frozen_default(), &authority).unwrap();
    let waiter =
        SqliteHandles::open_writable(config, &authority.binding_key, authority.cursor_key).unwrap();
    let (took, reserved) = std::thread::scope(|scope| {
        let (held, holding) = mpsc::channel();
        let history = &holder.history;
        let lane = scope.spawn(move || {
            let lane = history.publish_lane().expect("publication lane");
            held.send(()).ok();
            std::thread::sleep(HELD);
            drop(lane);
        });
        holding
            .recv_timeout(Duration::from_secs(10))
            .expect("the lane is held");
        let storage = &waiter.storage;
        let writer = scope.spawn(move || {
            storage.reserve(Reserve {
                packs: 1,
                ordinals: 0,
            })
        });
        // Setup, not a threshold: lets the writer reach the kernel wait.
        std::thread::sleep(Duration::from_millis(500));
        let started = Instant::now();
        let counters = waiter.diagnostics();
        let took = started.elapsed();
        lane.join().expect("lane holder");
        let reserved = writer.join().expect("waiting writer");
        counters.expect("waiting Session counters");
        (took, reserved)
    });
    reserved.expect("the waiting writer reserves once the lane is released");
    assert!(
        took < LIMIT,
        "counters waited {took:?} for another Session's lane"
    );
}
