//! S2.2 writes under load and slow producers (§11, review round 5): calls
//! wait. A piped `file write` is spooled before the CLI dials, so a
//! producer slower than the worker's progress window still commits once;
//! twenty writes at once all commit, or past the worker's accept limit an
//! extra one is refused by its hello bound before it sends anything.
#![cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]

#[path = "common/host_worker.rs"]
mod fixture;
#[path = "common/host_writes.rs"]
mod writes;

use std::io::Write;
use std::process::Stdio;
use std::time::{Duration, Instant};
use writes::*;

#[test]
fn a_piped_write_whose_producer_sleeps_past_the_progress_window_commits_once() {
    let home = prepare(&["work"]);
    let path = &home.paths.home;
    ok(
        path,
        &["file", "write", "shop/api", "work", "/warm"],
        b"w\n",
    );
    let before = commits(path, "shop/api", "work");
    let routed_before = routed(path);
    let began = Instant::now();
    let mut child = cli(path, &["file", "write", "shop/api", "work", "/late"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    // longer than the worker's 30 s minimum-progress window
    let producer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(35));
        input.write_all(b"late\n").unwrap();
    });
    let end = began + Duration::from_secs(80);
    while child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < end, "the piped write did not finish");
        std::thread::sleep(Duration::from_millis(50));
    }
    producer.join().unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(began.elapsed() >= Duration::from_secs(35));
    assert_eq!(
        routed(path),
        routed_before + 1,
        "it went through the worker"
    );
    assert_eq!(commits(path, "shop/api", "work"), before + 1);
    assert_eq!(read(path, "work", "/late"), "late\n");
    drain(&home);
}

#[test]
fn twenty_writes_at_once_all_commit_or_are_refused_before_sending() {
    let home = prepare(&["work"]);
    let path = &home.paths.home;
    ok(
        path,
        &["file", "write", "shop/api", "work", "/warm"],
        b"w\n",
    );
    let before = commits(path, "shop/api", "work");
    let writers: Vec<_> = (0..20)
        .map(|index| {
            let target = format!("/c-{index:02}");
            let mut child = cli(path, &["file", "write", "shop/api", "work", &target])
                .stdin(Stdio::piped())
                .spawn()
                .unwrap();
            let mut input = child.stdin.take().unwrap();
            input.write_all(&patterned(256 << 10)).unwrap();
            drop(input);
            child
        })
        .collect();
    let mut committed = 0;
    for writer in writers {
        let output = finish(writer);
        if output.status.success() {
            committed += 1;
            continue;
        }
        // only the pre-send refusal of a full worker: nothing was sent
        let text = String::from_utf8_lossy(&output.stderr);
        let answer = layerfs_api::json::parse(text.trim()).unwrap();
        assert_eq!(answer.get_text("error"), Some("Unavailable"), "{answer:?}");
        assert!(
            !text.contains("request ") && !text.contains("Unknown"),
            "{answer:?}"
        );
    }
    eprintln!("RECEIPT twenty concurrent writes: {committed} committed");
    assert_eq!(commits(path, "shop/api", "work"), before + committed);
    for index in 0..20 {
        let target = format!("/c-{index:02}");
        let (found, _) = run(path, &["file", "stat", "shop/api", "work", &target], b"");
        assert!(found || committed < 20, "{target} is missing");
    }
    assert!(retained(path).is_empty());
    drain(&home);
}
