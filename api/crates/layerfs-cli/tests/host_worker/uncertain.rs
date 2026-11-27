//! S2.2 records of engine-uncertain write outcomes (§8.5, review M3),
//! through a worker built with `layerfs-api/worker-fault`: its switch
//! file turns a write's outcome into an engine-style `Unknown`. The record
//! is written before the answer and outlives drain and replacement. A
//! record that cannot be written is one attempt: the outcome is logged,
//! named `recorded: false` in its answer and listed by the drain or stop
//! that ends the worker, which never waits on it. At 256 records the
//! oldest `undelivered` ones make room; `uncertain` ones never do. Without
//! the feature the test reports NOT_RUN.
#![cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]

use crate::fixture;
#[path = "../support/host_writes.rs"]
mod writes;

use layerfs_home::json;
use layerfs_protocol::sdk::SdkValue;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use writes::*;

struct Faulted {
    home: fixture::Fixture,
    machine: PathBuf,
    switch: PathBuf,
}

/// A prepared home whose worker has the fault switches, or None (NOT_RUN).
fn faulted() -> Option<Faulted> {
    let home = prepare(&["work"]);
    let path = home.paths.home.clone();
    ok(
        &path,
        &["file", "write", "shop/api", "work", "/warm"],
        b"w\n",
    );
    if !home.ping().get_bool("faults") {
        eprintln!("NOT_RUN: the worker was built without layerfs-api/worker-fault");
        drain(&home);
        return None;
    }
    let machine = path.join("machine").join(host());
    let switch = machine.join("fault-write-unknown");
    Some(Faulted {
        home,
        machine,
        switch,
    })
}

/// One write through the switch: its `Unknown` answer.
fn uncertain_write(f: &Faulted, target: &str) -> SdkValue {
    std::fs::write(&f.switch, b"").unwrap();
    let answer = refused(
        &f.home.paths.home,
        &["file", "write", "shop/api", "work", target],
        b"u\n",
        "Unknown",
    );
    std::fs::remove_file(&f.switch).unwrap();
    answer
}

fn detail(answer: &SdkValue) -> &SdkValue {
    answer
        .get("detail")
        .unwrap_or_else(|| panic!("no detail: {answer:?}"))
}

/// Writes `count` record files of `kind` with `at` from `first`.
fn fake_records(dir: &Path, kind: &str, first: i64, count: i64) {
    std::fs::create_dir_all(dir).unwrap();
    for at in first..first + count {
        let id = format!("{kind}{at:08}");
        let record = SdkValue::map([
            ("request_id", SdkValue::text(&id)),
            ("kind", SdkValue::text(kind)),
            ("at", SdkValue::Int(at)),
        ]);
        std::fs::write(dir.join(format!("{id}.json")), json::print(&record)).unwrap();
    }
}

fn clear(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
    let _ = std::fs::remove_file(dir);
}

#[test]
fn an_uncertain_outcome_is_recorded_before_its_answer_and_outlives_the_worker() {
    let Some(f) = faulted() else { return };
    let path = &f.home.paths.home;
    let before = commits(path, "shop/api", "work");
    let unknown = uncertain_write(&f, "/u");
    let id = detail(&unknown).get_text("request_id").unwrap().to_owned();
    assert_eq!(
        detail(&unknown).get("recorded"),
        Some(&SdkValue::Bool(true))
    );
    let records = retained(path);
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].get_text("request_id"), Some(id.as_str()));
    assert_eq!(records[0].get_text("kind"), Some("uncertain"));
    assert_eq!(
        records[0].get("outcome").unwrap().get_text("code"),
        Some("Unknown")
    );
    // the fault keeps the real effect: the Commit happened
    assert_eq!(commits(path, "shop/api", "work"), before + 1);
    // it outlives drain and a replacement worker, and the doctor lists it
    // with the folder's `rm` as its fix
    drain(&f.home);
    ok(path, &["file", "write", "shop/api", "work", "/v"], b"v\n");
    assert_eq!(retained(path).len(), 1);
    let (check, report) = retained_report(path);
    assert_eq!(check.get_text("outcome"), Some("notice"), "{check:?}");
    let report = json::print(&report);
    assert!(
        report.contains(&id) && report.contains("uncertain"),
        "{report}"
    );
    assert!(
        report.contains("rm ") && report.contains("retained"),
        "{report}"
    );
    drain(&f.home);
}

#[test]
fn a_record_that_cannot_be_written_is_reported_and_never_wedges_drain_or_stop() {
    let Some(f) = faulted() else { return };
    let path = &f.home.paths.home;
    let dir = f.machine.join("retained");
    clear(&dir);
    std::fs::write(&dir, b"not a directory").unwrap();
    // the answer says the outcome has no record
    let unknown = uncertain_write(&f, "/w");
    let id = detail(&unknown).get_text("request_id").unwrap().to_owned();
    assert_eq!(
        detail(&unknown).get("recorded"),
        Some(&SdkValue::Bool(false))
    );
    assert_eq!(f.home.ping().get_int("unrecorded"), Some(1));
    let log = std::fs::read_to_string(path.join("log").join("worker.log")).unwrap_or_default();
    assert!(
        log.contains(&id),
        "the unrecorded outcome is not in the log"
    );
    // the drain is not refused, waits on nothing and lists the outcome
    let began = Instant::now();
    let answer = f.home.dial(true).request("drain");
    let drained = answer.get("ok").unwrap_or_else(|| panic!("{answer:?}"));
    assert_eq!(drained.get_int("unrecorded_count"), Some(1), "{answer:?}");
    assert!(json::print(drained).contains(&id), "{answer:?}");
    wait_for("the drained worker to exit", || {
        (!f.home.paths.socket.exists()).then_some(())
    });
    assert!(began.elapsed() < Duration::from_secs(10));
    // machine stop reports what its drain listed, and start works again
    let unknown = uncertain_write(&f, "/x");
    let id = detail(&unknown).get_text("request_id").unwrap().to_owned();
    let stopped = ok(path, &["machine", "stop", host()], b"");
    assert_eq!(
        stopped.get_int("unrecorded_writes_count"),
        Some(1),
        "{stopped:?}"
    );
    assert!(json::print(&stopped).contains(&id), "{stopped:?}");
    ok(path, &["machine", "start", host()], b"");
    // no record was written for either, and none is retried
    std::fs::remove_file(&dir).unwrap();
    ok(path, &["file", "write", "shop/api", "work", "/y"], b"y\n");
    assert!(retained(path).is_empty());
    drain(&f.home);
}

#[test]
fn at_the_cap_the_oldest_undelivered_records_make_room_and_uncertain_ones_stay() {
    let Some(f) = faulted() else { return };
    let path = &f.home.paths.home;
    let dir = f.machine.join("retained");
    clear(&dir);
    // 256 records: 100 undelivered (the oldest first), 156 uncertain
    fake_records(&dir, "undelivered", 1, 100);
    fake_records(&dir, "uncertain", 1000, 156);
    let unknown = uncertain_write(&f, "/c1");
    let id = detail(&unknown).get_text("request_id").unwrap().to_owned();
    assert_eq!(
        detail(&unknown).get("recorded"),
        Some(&SdkValue::Bool(true))
    );
    let records = retained(path);
    assert_eq!(records.len(), 256);
    let ids: Vec<_> = records
        .iter()
        .filter_map(|r| r.get_text("request_id"))
        .collect();
    assert!(!ids.contains(&"undelivered00000001"), "the oldest stayed");
    assert!(ids.contains(&"undelivered00000002"));
    assert!(ids.contains(&id.as_str()));
    // only uncertain records: a new one is refused, logged and reported,
    // and nothing waits on it
    clear(&dir);
    fake_records(&dir, "uncertain", 1, 256);
    let unknown = uncertain_write(&f, "/c2");
    assert_eq!(
        detail(&unknown).get("recorded"),
        Some(&SdkValue::Bool(false))
    );
    assert_eq!(retained(path).len(), 256);
    assert_eq!(f.home.ping().get_int("unrecorded"), Some(1));
    clear(&dir);
    drain(&f.home);
}
