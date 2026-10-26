//! Reading a session's SQL counters does not wait for the publication lane
//! (#352 F-018). A traced daemon call reads the writer's counters before and
//! after its operation. When that read queued for the writer's admission, a
//! traced read waited for whole Commits holding the lane, merges included.
mod support;

use layerfs_history::{HistoryCatalog, HistoryCatalogConfig};
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePersistenceProfile};
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
    config.sqlite_profile = SqlitePersistenceProfile::Disposable;
    let handles = Handles::create(
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
