//! S2.2 custody of engine-uncertain write outcomes (§8.5, review M3),
//! through a worker built with `layerfs-control/worker-fault`: its switch
//! file turns a write's outcome into an engine-style `Unknown`. The record
//! is written before the answer and outlives drain and replacement; when
//! it cannot be written the worker holds the outcome and refuses to drain
//! until it is. Without the feature the test reports NOT_RUN.
#![cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]

#[path = "common/host_worker.rs"]
mod fixture;
#[path = "common/host_writes.rs"]
mod writes;

use layerfs_api::json;
use writes::*;

#[test]
fn uncertain_outcomes_are_kept_before_their_answer_and_block_drain_until_kept() {
    let home = prepare(&["work"]);
    let path = &home.paths.home;
    ok(
        path,
        &["file", "write", "shop/api", "work", "/warm"],
        b"w\n",
    );
    if !home.ping().get_bool("faults") {
        eprintln!("NOT_RUN: the worker was built without layerfs-control/worker-fault");
        drain(&home);
        return;
    }
    let machine = path.join("machine").join(host());
    let switch = machine.join("fault-write-unknown");
    let before = commits(path, "shop/api", "work");

    // an uncertain outcome: its record exists when its answer arrives
    std::fs::write(&switch, b"").unwrap();
    let unknown = refused(
        path,
        &["file", "write", "shop/api", "work", "/u"],
        b"u\n",
        "Unknown",
    );
    std::fs::remove_file(&switch).unwrap();
    let id = unknown
        .get("detail")
        .and_then(|detail| detail.get_text("request_id"))
        .unwrap_or_else(|| panic!("no request id: {unknown:?}"))
        .to_owned();
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
    drain(&home);
    ok(path, &["file", "write", "shop/api", "work", "/v"], b"v\n");
    assert_eq!(retained(path).len(), 1);
    let (check, report) = retained_report(path);
    assert_eq!(check.get_text("outcome"), Some("notice"), "{check:?}");
    let report = json::print(&report);
    assert!(
        report.contains(&id) && report.contains("uncertain"),
        "{report}"
    );
    let pruned = ok(path, &["machine", "prune", host(), "--retained"], b"");
    assert_eq!(
        json::print(pruned.get("removed").unwrap()),
        format!(r#"["{id}"]"#)
    );

    // a record that cannot be written: the worker holds the outcome,
    // logs it and refuses to drain until the record is written
    let dir = machine.join("retained");
    std::fs::remove_dir_all(&dir).unwrap();
    std::fs::write(&dir, b"not a directory").unwrap();
    std::fs::write(&switch, b"").unwrap();
    let unknown = refused(
        path,
        &["file", "write", "shop/api", "work", "/w"],
        b"w\n",
        "Unknown",
    );
    std::fs::remove_file(&switch).unwrap();
    let id = unknown
        .get("detail")
        .and_then(|detail| detail.get_text("request_id"))
        .unwrap()
        .to_owned();
    assert_eq!(home.ping().get_int("unkept"), Some(1));
    let answer = home.dial(true).request("drain");
    let error = answer.get("error").unwrap_or_else(|| panic!("{answer:?}"));
    assert_eq!(error.get_text("code"), Some("Unavailable"), "{answer:?}");
    assert!(
        error.get_text("message").unwrap().contains("uncertain"),
        "{answer:?}"
    );
    // still serving, the outcome still held
    assert_eq!(home.ping().get_text("state"), Some("ready"));
    assert_eq!(home.ping().get_int("unkept"), Some(1));
    let log = std::fs::read_to_string(path.join("log").join("worker.log")).unwrap_or_default();
    assert!(log.contains(&id), "the held outcome is not in the log");

    // once the path is usable, the drain writes the record and proceeds
    std::fs::remove_file(&dir).unwrap();
    drain(&home);
    let records = retained(path);
    assert_eq!(records.len(), 1, "{records:?}");
    assert_eq!(records[0].get_text("request_id"), Some(id.as_str()));
    assert_eq!(records[0].get_text("kind"), Some("uncertain"));
    assert_eq!(commits(path, "shop/api", "work"), before + 3);
    ok(path, &["machine", "prune", host(), "--retained"], b"");
    assert!(retained(path).is_empty());
    drain(&home);
}
