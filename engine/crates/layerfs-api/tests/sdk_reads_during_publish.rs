//! An SDK read does not wait for another Commit's publication (#352 F-018).
//! A Commit holds the Store's publication lane from its head check through
//! publish, including any merge with a moved head. Reads only need published
//! history, so they must not queue behind that lane. Range 8 of p2-main showed
//! reads that overlapped a held lane taking 90-100 ms instead of 6 ms.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use support::{write, Fixture};

/// How long the test holds the lane; a read that waits for it takes this long
/// (or the Store's 2 s admission window, whichever ends first).
const HELD: Duration = Duration::from_secs(3);
/// A read that does not wait finishes far within this.
const READ_LIMIT: Duration = Duration::from_millis(750);

#[test]
fn reads_do_not_wait_for_a_held_publication_lane() {
    let fixture = Fixture::new("sdk-reads-during-publish", |source| {
        std::fs::write(source.join("seed.txt"), "seed\n").unwrap();
        1
    });
    fixture.fork("work");
    fixture.apply("work", vec![write("/small.txt", "small\n")]);

    let store = fixture.service.store.clone();
    let (held, holding) = mpsc::channel();
    let holder = std::thread::spawn(move || {
        let lane = store.history().publish_lane().expect("publication lane");
        held.send(()).ok();
        std::thread::sleep(HELD);
        drop(lane);
    });
    holding
        .recv_timeout(Duration::from_secs(10))
        .expect("the lane is held");

    let mut slow = Vec::new();
    for (method, fields) in [
        (
            "fs.read",
            vec![
                ("branch", SdkValue::text("work")),
                ("path", SdkValue::text("/small.txt")),
            ],
        ),
        (
            "fs.stat",
            vec![
                ("branch", SdkValue::text("work")),
                ("path", SdkValue::text("/small.txt")),
            ],
        ),
        (
            "fs.list",
            vec![
                ("branch", SdkValue::text("work")),
                ("path", SdkValue::text("/")),
            ],
        ),
        (
            "fs.read",
            vec![
                ("branch", SdkValue::text("main")),
                ("path", SdkValue::text("/seed.txt")),
            ],
        ),
    ] {
        let started = Instant::now();
        fixture.call(method, fields);
        let took = started.elapsed();
        if took > READ_LIMIT {
            slow.push(format!("{method} {took:?}"));
        }
    }
    holder.join().expect("lane holder");
    assert!(slow.is_empty(), "reads waited for the held lane: {slow:?}");
    fixture.cleanup();
}
