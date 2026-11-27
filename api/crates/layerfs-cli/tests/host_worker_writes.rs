//! S2.2: CLI file writes through the host worker, their input streamed in
//! bounded frames, each one logical call with one Commit (or none), and
//! outcomes a caller may not know kept on disk (§8.5).
#![cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]

#[path = "common/host_worker.rs"]
mod fixture;
#[path = "common/host_writes.rs"]
mod writes;

use fixture::Fixture;
use layerfs_home::json;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::process::{Child, Command, Output, Stdio};
use std::time::Duration;
use writes::*;

#[test]
fn every_write_streams_through_the_worker_as_one_commit() {
    let home = prepare(&["work"]);
    let path = &home.paths.home;
    let before = commits(path, "shop/api", "work");
    let binary = patterned(3 << 20);
    let from = path.join("binary.in");
    std::fs::write(&from, &binary).unwrap();
    let from = from.to_str().unwrap();
    let big = "z".repeat(1 << 20);
    let changes = format!(
        r#"[{{"op":"write","path":"/x","content":"x\n"}},{{"op":"mkdir","path":"/y"}},{{"op":"write","path":"/y/big","content":"{big}"}}]"#
    );
    let calls: Vec<(Vec<&str>, &[u8])> = vec![
        (
            vec!["file", "write", "shop/api", "work", "/a.txt"],
            b"alpha\n",
        ),
        (
            vec![
                "file", "write", "shop/api", "work", "/b.bin", "--from", from,
            ],
            b"",
        ),
        (
            vec![
                "file",
                "edit",
                "shop/api",
                "work",
                "/a.txt",
                "--old-string",
                "alpha",
                "--new-string",
                "beta",
            ],
            b"",
        ),
        (vec!["file", "mkdir", "shop/api", "work", "/d"], b""),
        (
            vec!["file", "cp", "shop/api", "work", "/a.txt", "/d/c.txt"],
            b"",
        ),
        (
            vec!["file", "mv", "shop/api", "work", "/d/c.txt", "/d/e.txt"],
            b"",
        ),
        (vec!["file", "rm", "shop/api", "work", "/hello"], b""),
        (
            vec!["file", "apply", "shop/api", "work"],
            changes.as_bytes(),
        ),
    ];
    for (index, (words, stdin)) in calls.iter().enumerate() {
        let answer = ok(path, words, stdin);
        assert_eq!(answer.get_text("outcome"), Some("committed"), "{answer:?}");
        assert_eq!(routed(path), index + 1, "{words:?} was not routed");
    }
    assert_eq!(commits(path, "shop/api", "work"), before + calls.len());
    assert_eq!(read(path, "work", "/a.txt"), "beta\n");
    assert_eq!(read(path, "work", "/d/e.txt"), "beta\n");
    assert_eq!(read(path, "work", "/x"), "x\n");
    assert_eq!(size(path, "work", "/b.bin"), binary.len() as i64);
    assert_eq!(size(path, "work", "/y/big"), 1 << 20);
    let (gone, _) = run(path, &["file", "stat", "shop/api", "work", "/hello"], b"");
    assert!(!gone);

    // the same content again changes nothing: one call, no Commit
    let again = ok(
        path,
        &[
            "file", "write", "shop/api", "work", "/b.bin", "--from", from,
        ],
        b"",
    );
    assert_eq!(again.get_text("outcome"), Some("unchanged"), "{again:?}");
    assert_eq!(commits(path, "shop/api", "work"), before + calls.len());
    // a shorter and a longer write over the same file
    ok(
        path,
        &["file", "write", "shop/api", "work", "/a.txt"],
        b"be",
    );
    assert_eq!(read(path, "work", "/a.txt"), "be");
    ok(
        path,
        &["file", "write", "shop/api", "work", "/a.txt"],
        b"beta and more\n",
    );
    assert_eq!(read(path, "work", "/a.txt"), "beta and more\n");
    // main stays read-only with the same refusal and fix
    let main = refused(
        path,
        &["file", "write", "shop/api", "main", "/m"],
        b"m",
        "InvalidInput",
    );
    assert_eq!(
        main.get("detail").unwrap().get_text("code"),
        Some("ReadOnly")
    );
    assert!(json::print(&main).contains("branch fork"), "{main:?}");
    drain(&home);
}

#[test]
fn apply_rows_past_the_fused_limit_still_publish_one_commit() {
    let home = prepare(&["rows"]);
    let path = &home.paths.home;
    for (count, folder) in [(8_191, "a"), (8_192, "b")] {
        let before = commits(path, "shop/api", "rows");
        let mut changes = format!(r#"[{{"op":"mkdir","path":"/{folder}"}}"#);
        for index in 1..count {
            changes.push_str(&format!(
                r#",{{"op":"write","path":"/{folder}/{index}","content":"{index}"}}"#
            ));
        }
        changes.push(']');
        let file = path.join(format!("{folder}.json"));
        std::fs::write(&file, changes).unwrap();
        let answer = ok(
            path,
            &[
                "file",
                "apply",
                "shop/api",
                "rows",
                "--from",
                file.to_str().unwrap(),
            ],
            b"",
        );
        assert_eq!(answer.get_text("outcome"), Some("committed"), "{answer:?}");
        assert_eq!(commits(path, "shop/api", "rows"), before + 1, "{count}");
        let last = format!("/{folder}/{}", count - 1);
        assert_eq!(read(path, "rows", &last), (count - 1).to_string());
    }
    assert_eq!(routed(path), 2);
    drain(&home);
}

#[test]
fn stopped_or_refused_inputs_commit_nothing() {
    let home = prepare(&["work"]);
    let path = &home.paths.home;
    let before = commits(path, "shop/api", "work");
    // invalid JSON after a valid change: the CLI stops before the end
    let broken = refused(
        path,
        &["file", "apply", "shop/api", "work"],
        br#"[{"op":"mkdir","path":"/q"}, {"op": "#,
        "InvalidInput",
    );
    assert!(json::print(&broken).contains("not JSON"), "{broken:?}");
    // a change refused while more input follows: the worker's own refusal
    let large = "w".repeat(8 << 20);
    let changes = format!(
        r#"[{{"op":"mkdir","path":"/r"}},{{"op":"rm","path":"/missing"}},{{"op":"write","path":"/w","content":"{large}"}}]"#
    );
    refused(
        path,
        &["file", "apply", "shop/api", "work"],
        changes.as_bytes(),
        "NotFound",
    );
    // a write to a folder is refused before its content is read
    refused(
        path,
        &["file", "write", "shop/api", "work", "/"],
        &patterned(4 << 20),
        "InvalidInput",
    );
    assert_eq!(commits(path, "shop/api", "work"), before);
    assert_eq!(routed(path), 3);
    drain(&home);
}

#[test]
fn lost_inputs_and_unread_answers_leave_inspectable_evidence() {
    let home = prepare(&["work"]);
    let path = &home.paths.home;
    assert_eq!(
        retained_check(path).get_text("outcome"),
        Some("pass"),
        "{:?}",
        retained_check(path)
    );

    // SIGKILL while the CLI spools its piped input: nothing was sent, so
    // nothing commits, nothing is kept and no spool file is left
    ok(
        path,
        &["file", "write", "shop/api", "work", "/warm"],
        b"w\n",
    );
    let before = commits(path, "shop/api", "work");
    let routed_before = routed(path);
    let mut child = cli(path, &["file", "write", "shop/api", "work", "/killed"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    input.write_all(&patterned(2 << 20)).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    child.kill().unwrap();
    child.wait().unwrap();
    drop(input);
    let spool = path.join("machine").join(host()).join("spool");
    let left = std::fs::read_dir(&spool).map_or(0, |dir| dir.count());
    assert_eq!(left, 0, "a spool file was left in {}", spool.display());
    assert_eq!(routed(path), routed_before);

    // a raw caller gone mid-stream, after `go`: a definite refusal it
    // never reads; nothing commits and nothing is kept
    let exchange = home.dial_writes();
    let id = "000000000000000000000000000dead0";
    let mut socket: &UnixStream = exchange.socket();
    socket
        .write_all(&content_request(&exchange, id, "/killed"))
        .unwrap();
    read_go(exchange.socket(), id);
    socket
        .write_all(&frame(&patterned(64 << 10), true))
        .unwrap();
    wait_for("the stream to reach the worker", || {
        (home.ping().get_int("admitted") >= Some(2)).then_some(())
    });
    drop(exchange);
    wait_for("the dropped stream to end", || {
        (home.ping().get_int("admitted") == Some(1)).then_some(())
    });
    assert!(retained(path).is_empty(), "{:?}", retained(path));
    assert_eq!(commits(path, "shop/api", "work"), before);

    // a whole input whose answer is never read: committed, and kept
    unread_write(
        &home,
        "00000000000000000000000000c0ffee",
        &patterned(1 << 20),
    );
    let records = wait_for("the unread answer's record", || {
        let records = retained(path);
        (records.len() == 1).then_some(records)
    });
    let unread = records
        .iter()
        .find(|r| r.get_text("request_id") == Some("00000000000000000000000000c0ffee"))
        .unwrap();
    assert_eq!(unread.get_text("kind"), Some("undelivered"));
    assert_eq!(
        unread.get("outcome").unwrap().get_text("kind"),
        Some("Committed"),
        "{unread:?}"
    );
    assert_eq!(commits(path, "shop/api", "work"), before + 1);

    // the records survive drain, a SIGKILLed worker and its replacement
    drain(&home);
    let mut worker = home.start();
    wait_for("a replacement worker", || {
        home.paths.control.exists().then_some(())
    });
    worker.kill();
    ok(
        path,
        &["file", "write", "shop/api", "work", "/after"],
        b"after\n",
    );
    assert_eq!(commits(path, "shop/api", "work"), before + 2);
    assert_eq!(retained(path).len(), 1);
    let (check, report) = retained_report(path);
    assert_eq!(check.get_text("outcome"), Some("notice"), "{check:?}");
    let report = json::print(&report);
    assert!(
        report.contains("c0ffee") && report.contains("undelivered"),
        "{report}"
    );
    // the finding names the folder; its fix removes the checked records
    let dir = path.join("machine").join(host()).join("retained");
    assert!(
        report.contains(&format!("rm {}/*.json", dir.display())),
        "{report}"
    );
    for record in std::fs::read_dir(&dir).unwrap() {
        std::fs::remove_file(record.unwrap().path()).unwrap();
    }
    assert!(retained(path).is_empty());
    assert_eq!(retained_check(path).get_text("outcome"), Some("pass"));
    drain(&home);
}

/// C2 and C3 (§16.1): ten concurrent CLI writes to one Branch, then to ten
/// Stores, all through the worker, none refused `Busy`, each one Commit.
#[test]
fn ten_concurrent_writes_each_commit_once() {
    let home = prepare(&["work"]);
    let path = &home.paths.home;
    let before = commits(path, "shop/api", "work");
    let spawn = |words: Vec<String>, content: Vec<u8>| {
        let words: Vec<&str> = words.iter().map(String::as_str).collect();
        let mut child = cli(path, &words).stdin(Stdio::piped()).spawn().unwrap();
        let mut input = child.stdin.take().unwrap();
        std::thread::spawn(move || {
            let _ = input.write_all(&content);
        });
        child
    };
    let calls: Vec<Child> = (0..10)
        .map(|index| {
            let words = ["file", "write", "shop/api", "work", &format!("/c2-{index}")]
                .map(str::to_owned)
                .to_vec();
            spawn(words, patterned(300 << 10))
        })
        .collect();
    for call in calls {
        let output = finish(call);
        assert!(output.status.success(), "{output:?}");
    }
    assert_eq!(commits(path, "shop/api", "work"), before + 10);
    assert_eq!(routed(path), 10);

    let source = path.join("source");
    for index in 0..10 {
        let store = format!("s{index}");
        ok(path, &["store", "create", &store], b"");
        let project = format!("{store}/p");
        ok(
            path,
            &["project", "init", &project, source.to_str().unwrap()],
            b"",
        );
        ok(path, &["branch", "fork", &project, "work"], b"");
    }
    let fresh = commits(path, "s0/p", "work");
    let calls: Vec<Child> = (0..10)
        .map(|index| {
            let words = ["file", "write", &format!("s{index}/p"), "work", "/c3"]
                .map(str::to_owned)
                .to_vec();
            spawn(words, patterned(300 << 10))
        })
        .collect();
    for call in calls {
        let output = finish(call);
        assert!(output.status.success(), "{output:?}");
    }
    for index in 0..10 {
        assert_eq!(commits(path, &format!("s{index}/p"), "work"), fresh + 1);
    }
    assert_eq!(routed(path), 20);
    drain(&home);
}

/// C9 for writes (§8.5): the worker SIGKILLed while a write waits for its
/// Store. A caller still sending gets a definite refusal; a caller whose
/// whole input was sent gets `Unknown` with its request ID. Nothing is
/// committed and nothing is resent when a new worker serves the next call.
#[test]
fn a_killed_worker_leaves_unsent_writes_refused_and_sent_ones_unknown() {
    let home = prepare(&["work"]);
    let path = &home.paths.home;
    let before = commits(path, "shop/api", "work");
    ok(
        path,
        &["file", "write", "shop/api", "work", "/warm"],
        b"warm\n",
    );
    let pid = home.ping().get_int("pid").unwrap();
    let gate = layerfs_api::home::entity_lock::take(
        &layerfs_api::home::paths::store_dir(path, "shop").join("lock"),
        WAIT,
        "proof Store lock",
    )
    .unwrap();
    let spawn = |target: &str, content: Vec<u8>| {
        let mut child = cli(path, &["file", "write", "shop/api", "work", target])
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        std::thread::spawn(move || {
            let _ = input.write_all(&content);
        });
        child
    };
    let sent = spawn("/sent", b"small\n".to_vec());
    let unsent = spawn("/unsent", patterned(16 << 20));
    wait_for("both writes to be admitted", || {
        (home.ping().get_int("admitted") >= Some(3)).then_some(())
    });
    std::thread::sleep(Duration::from_millis(300));
    let killed = Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status()
        .unwrap();
    assert!(killed.success());
    drop(gate);
    let parse = |output: Output| {
        assert!(!output.status.success(), "{output:?}");
        let text = if output.stdout.is_empty() {
            output.stderr
        } else {
            output.stdout
        };
        json::parse(String::from_utf8_lossy(&text).trim()).unwrap()
    };
    let unknown = parse(finish(sent));
    assert_eq!(unknown.get_text("error"), Some("Unknown"), "{unknown:?}");
    assert!(json::print(&unknown).contains("request_id"), "{unknown:?}");
    let refused = parse(finish(unsent));
    assert_eq!(
        refused.get_text("error"),
        Some("Unavailable"),
        "{refused:?}"
    );
    assert!(
        refused
            .get_text("message")
            .unwrap()
            .contains("nothing was committed"),
        "{refused:?}"
    );
    // the next call starts a new worker; neither write was applied
    ok(
        path,
        &["file", "write", "shop/api", "work", "/next"],
        b"next\n",
    );
    assert_ne!(home.ping().get_int("pid"), Some(pid));
    assert_eq!(commits(path, "shop/api", "work"), before + 2);
    for gone in ["/sent", "/unsent"] {
        let (found, _) = run(path, &["file", "stat", "shop/api", "work", gone], b"");
        assert!(!found, "{gone} was applied");
    }
    drain(&home);
}

/// A PostgreSQL Store whose record holds a login revision routes its
/// writes to the worker too (#819 P-B writes): each write, streamed input
/// or not, commits once there. Needs `LAYERFS_TEST_POSTGRES_URL`.
#[test]
fn writes_on_a_postgresql_store_go_to_the_worker() {
    let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL unset");
        return;
    };
    let home = Fixture::new();
    let path = &home.paths.home;
    let (scheme, rest) = url.split_once("://").unwrap();
    let (user, address) = rest.split_once('@').unwrap();
    let address = format!("{scheme}://{address}");
    let words = [
        "store",
        "connect",
        "q",
        &address,
        "--user",
        user,
        "--password-stdin",
    ];
    ok(path, &words, b"unused\n");
    // the project lives in the shared database: a name of its own per run
    let project = format!(
        "q/w{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let project = project.as_str();
    let source = path.join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("hello"), "hello\n").unwrap();
    ok(
        path,
        &["project", "init", project, source.to_str().unwrap()],
        b"",
    );
    ok(path, &["branch", "fork", project, "work"], b"");
    let before = commits(path, project, "work");
    ok(
        path,
        &["file", "write", project, "work", "/big"],
        &patterned(1 << 20),
    );
    let changes = br#"[{"op":"mkdir","path":"/d"},{"op":"write","path":"/d/f","content":"x\n"}]"#;
    ok(path, &["file", "apply", project, "work"], changes);
    ok(path, &["file", "mkdir", project, "work", "/e"], b"");
    assert_eq!(commits(path, project, "work"), before + 3);
    let stat = ok(path, &["file", "stat", project, "work", "/big"], b"");
    assert_eq!(stat.get_int("size"), Some(1 << 20));
    // every write was routed, once each
    assert_eq!(routed(path), 3);
    drain(&home);
}
