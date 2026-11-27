//! S2.2 write admission and budgets (§11, review round 3): a write whose
//! request is read while the worker drains is refused before any effect,
//! never queued for a write thread that has ended; a `file apply` list with
//! a change past the worker's budget for one change (4 MiB decoded) runs
//! direct, decided before anything is sent, whether it comes from a file or
//! a pipe (spooled to an unnamed private file first): no size refusal. The
//! worker itself refuses such a change before effect when sent anyway.
#![cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]

#[path = "common/host_worker.rs"]
mod fixture;
#[path = "common/host_writes.rs"]
mod writes;

use layerfs_home::json;
use layerfs_protocol::sdk::SdkValue;
use std::io::Write;
use std::os::unix::net::UnixStream;
use writes::*;

/// The text of `/big`: alone past the 4 MiB decoded estimate (three bytes
/// per input byte).
fn big() -> String {
    "b".repeat(3 << 19)
}

/// An edit of all of `/big`: its old text is past the worker's budget.
fn large_edit() -> String {
    format!(
        r#"[{{"op":"edit","path":"/big","old_string":"{}","new_string":"x"}}]"#,
        big()
    )
}

#[test]
fn a_write_read_while_the_worker_drains_is_refused_unrun() {
    let home = prepare(&["work"]);
    let path = &home.paths.home;
    ok(
        path,
        &["file", "write", "shop/api", "work", "/warm"],
        b"w\n",
    );
    let before = commits(path, "shop/api", "work");
    // greeted before the drain, its request read after it
    let exchange = home.dial_writes();
    home.drain();
    let mut socket: &UnixStream = exchange.socket();
    let frames = content_frames(&exchange, "00000000000000000000000000d7a1d0", b"late\n");
    // the worker may answer and close before reading it all
    let _ = socket.write_all(&frames);
    let answer = raw_answer(exchange.socket());
    let error = answer.get("error").unwrap_or_else(|| panic!("{answer:?}"));
    assert_eq!(error.get_text("code"), Some("Unavailable"), "{answer:?}");
    assert_eq!(
        error.get("detail").unwrap().get_text("state"),
        Some("draining"),
        "{answer:?}"
    );
    assert!(
        error
            .get_text("message")
            .unwrap()
            .contains("nothing was committed"),
        "{answer:?}"
    );
    drop(exchange);
    wait_for("the drained worker to exit", || {
        (!home.paths.socket.exists()).then_some(())
    });
    assert_eq!(commits(path, "shop/api", "work"), before);
    assert!(retained(path).is_empty());
    drain(&home);
}

#[test]
fn a_change_past_the_budget_runs_direct_from_a_pipe_or_a_file() {
    let home = prepare(&["work"]);
    let path = &home.paths.home;
    ok(
        path,
        &["file", "write", "shop/api", "work", "/big"],
        big().as_bytes(),
    );
    let before = commits(path, "shop/api", "work");
    let list = large_edit();
    // a pipe: spooled, checked, then run direct with one Commit
    let routed_before = routed(path);
    let (success, answer, peak) = run_sampled(
        path,
        &["file", "apply", "shop/api", "work"],
        list.as_bytes(),
    );
    assert!(success, "{answer:?}");
    eprintln!(
        "RECEIPT piped over-budget apply: list {} bytes, CLI sampled peak RSS {} bytes",
        list.len(),
        peak
    );
    // loose: the direct route holds the list once, not a copy per stage
    assert!(peak < 256 << 20, "CLI sampled peak RSS {peak}");
    assert_eq!(routed(path), routed_before, "the piped list was sent");
    assert_eq!(commits(path, "shop/api", "work"), before + 1);
    assert_eq!(size(path, "work", "/big"), 1);
    let spool = path.join("machine").join(host()).join("spool");
    let left = std::fs::read_dir(&spool).map_or(0, |dir| dir.count());
    assert_eq!(left, 0, "a spool file was left in {}", spool.display());
    // a file: checked before sending, so it is routed direct too
    ok(
        path,
        &["file", "write", "shop/api", "work", "/big"],
        big().as_bytes(),
    );
    let routed_before = routed(path);
    let from = path.join("large.json");
    std::fs::write(&from, &list).unwrap();
    ok(
        path,
        &[
            "file",
            "apply",
            "shop/api",
            "work",
            "--from",
            from.to_str().unwrap(),
        ],
        b"",
    );
    assert_eq!(routed(path), routed_before, "the file list was sent");
    assert_eq!(commits(path, "shop/api", "work"), before + 3);
    assert_eq!(size(path, "work", "/big"), 1);
    // lists within the budget still stream, from a file or a pipe
    let small = r#"[{"op":"write","path":"/s","content":"s\n"},{"op":"mkdir","path":"/d"}]"#;
    let from = path.join("small.json");
    std::fs::write(&from, small).unwrap();
    ok(
        path,
        &[
            "file",
            "apply",
            "shop/api",
            "work",
            "--from",
            from.to_str().unwrap(),
        ],
        b"",
    );
    ok(
        path,
        &["file", "apply", "shop/api", "work"],
        r#"[{"op":"write","path":"/t","content":"t\n"}]"#.as_bytes(),
    );
    assert_eq!(routed(path), routed_before + 2);
    assert_eq!(commits(path, "shop/api", "work"), before + 5);
    drain(&home);
}

#[test]
fn the_worker_refuses_a_spilled_change_past_its_budget_before_effect() {
    let home = prepare(&["work"]);
    let path = &home.paths.home;
    ok(
        path,
        &["file", "write", "shop/api", "work", "/big"],
        big().as_bytes(),
    );
    let before = commits(path, "shop/api", "work");
    let change = json::parse(&large_edit()).unwrap();
    let SdkValue::List(changes) = change else {
        panic!()
    };
    let text = json::print(&SdkValue::map([
        ("op", SdkValue::text("edit")),
        ("path", SdkValue::text("/big")),
        ("oldText", changes[0].get("old_string").unwrap().clone()),
        ("newText", SdkValue::text("x")),
    ]));
    let exchange = home.dial_writes();
    let fields = SdkValue::map([("branch", SdkValue::text("work"))]);
    let mut frames = write_request(
        &exchange,
        "00000000000000000000000000b0d6e7",
        "fs.apply",
        "changes",
        fields,
    );
    frames.extend(frame(br#"{"spilled":true}"#, false));
    for piece in text.as_bytes().chunks(64 << 10) {
        frames.extend(frame(piece, true));
    }
    frames.extend(frame(&[], true));
    frames.extend(frame(br#"{"end":{"changes":1,"bytes":0}}"#, false));
    let mut socket: &UnixStream = exchange.socket();
    let _ = socket.write_all(&frames);
    let answer = raw_answer(exchange.socket());
    let error = answer.get("error").unwrap_or_else(|| panic!("{answer:?}"));
    assert_eq!(error.get_text("code"), Some("InvalidInput"), "{answer:?}");
    let message = error.get_text("message").unwrap();
    assert!(
        message.contains("budget") && message.contains("nothing was committed"),
        "{answer:?}"
    );
    drop(exchange);
    assert_eq!(commits(path, "shop/api", "work"), before);
    assert_eq!(size(path, "work", "/big"), 3 << 19);
    drain(&home);
}
