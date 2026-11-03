//! A traced SDK file call accounts for its time (#352 F-025 fix 0).
//!
//! Ranges 7-9 showed 3-5% of slow `fs.write` calls with 50-175 ms outside
//! every daemon timing root, and Commits whose phases wait for another
//! Commit's publication lane without saying so. Every span of a file call is
//! now a timing root (resolve, bind, change, commit, close), and time spent
//! queued for the Store writer's admission, a reader's admission or the shared
//! read session is a `wait.*` node below the phase that waited.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_store_service::telemetry::Recording;
use layerfs_telemetry::timer::TimingNode;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use support::{write, Fixture};

/// How long another thread holds the publication lane.
const HELD: Duration = Duration::from_millis(400);

fn find<'a>(node: &'a TimingNode, name: &str, out: &mut Vec<&'a TimingNode>) {
    if node.name() == name {
        out.push(node);
    }
    for child in node.children() {
        find(child, name, out);
    }
}

#[test]
fn a_file_call_names_its_spans_and_its_waits() {
    let fixture = Fixture::new("sdk-timing-coverage", |source| {
        std::fs::write(source.join("seed.txt"), "seed\n").unwrap();
        1
    });
    fixture.fork("work");

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

    let recording = Recording::start();
    let started = Instant::now();
    fixture.apply("work", vec![write("/a.txt", "a\n")]);
    let wall = started.elapsed();
    let reports = recording.finish();
    holder.join().expect("lane holder");

    let roots: Vec<&TimingNode> = reports.iter().filter_map(|r| r.root()).collect();
    let names: Vec<&str> = roots.iter().map(|r| r.name()).collect();
    for expected in [
        "files.resolve",
        "files.bind",
        "files.change",
        "commit",
        "files.close",
    ] {
        assert!(names.contains(&expected), "no {expected} root in {names:?}");
    }
    let covered: Duration = roots.iter().map(|r| r.elapsed()).sum();
    assert!(
        wall.saturating_sub(covered) < Duration::from_millis(25),
        "{:?} of {wall:?} outside every root {names:?}",
        wall.saturating_sub(covered)
    );
    let mut waits = Vec::new();
    for root in &roots {
        find(root, "wait.writer", &mut waits);
    }
    let waited: Duration = waits.iter().map(|n| n.elapsed()).sum();
    assert!(
        waited >= HELD / 2,
        "the Commit waited for the held lane ({wall:?}) but its wait.writer nodes total {waited:?}"
    );
    fixture.cleanup();
}
