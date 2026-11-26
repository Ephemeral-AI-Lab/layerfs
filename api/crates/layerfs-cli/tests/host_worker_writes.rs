//! S2.2: CLI file writes through the host worker, their input streamed in
//! bounded frames, each one logical call with one Commit (or none), and
//! outcomes a caller may not know kept on disk (§8.5).
#![cfg(all(feature = "machine", any(target_os = "macos", target_os = "linux")))]

#[path = "common/host_worker.rs"]
mod fixture;

use fixture::Fixture;
use layerfs_api::json;
use layerfs_bridge::control::SdkValue;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

const CLI: &str = env!("CARGO_BIN_EXE_layerfs");
const WAIT: Duration = Duration::from_secs(60);

fn cli(home: &Path, words: &[&str]) -> Command {
    let mut command = Command::new(CLI);
    command
        .args(words)
        .args(["--json", "--detail"])
        .env("LAYERFS_HOME", home)
        .env("LAYERFS_TRACE", home.join("trace.jsonl"))
        .env_remove("LAYERFS_TRACE_STEP")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

fn finish(mut child: Child) -> Output {
    let end = Instant::now() + WAIT;
    while child.try_wait().unwrap().is_none() {
        if Instant::now() > end {
            let _ = child.kill();
            panic!("a CLI call did not finish");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    child.wait_with_output().unwrap()
}

/// Runs one CLI call with `stdin` piped in; its JSON answer.
fn run(home: &Path, words: &[&str], stdin: &[u8]) -> (bool, SdkValue) {
    let mut child = cli(home, words).stdin(Stdio::piped()).spawn().unwrap();
    let mut input = child.stdin.take().unwrap();
    let bytes = stdin.to_vec();
    let feeder = std::thread::spawn(move || {
        let _ = input.write_all(&bytes);
    });
    let output = finish(child);
    feeder.join().unwrap();
    let text = if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    let value = json::parse(String::from_utf8_lossy(text).trim())
        .unwrap_or_else(|_| panic!("not JSON: {}", String::from_utf8_lossy(text)));
    (output.status.success(), value)
}

fn ok(home: &Path, words: &[&str], stdin: &[u8]) -> SdkValue {
    let (success, value) = run(home, words, stdin);
    assert!(success, "{words:?}: {value:?}");
    value
}

fn refused(home: &Path, words: &[&str], stdin: &[u8], code: &str) -> SdkValue {
    let (success, value) = run(home, words, stdin);
    assert!(!success, "{words:?} succeeded: {value:?}");
    assert_eq!(value.get_text("error"), Some(code), "{value:?}");
    value
}

fn prepare(branches: &[&str]) -> Fixture {
    let home = Fixture::new();
    let path = &home.paths.home;
    let source = path.join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("hello"), "hello\n").unwrap();
    ok(path, &["store", "create", "shop"], b"");
    ok(
        path,
        &["project", "init", "shop/api", source.to_str().unwrap()],
        b"",
    );
    for branch in branches {
        ok(path, &["branch", "fork", "shop/api", branch], b"");
    }
    home
}

fn commits(home: &Path, project: &str, branch: &str) -> usize {
    let listed = ok(
        home,
        &["commit", "list", project, branch, "--limit", "200"],
        b"",
    );
    match listed.get("items") {
        Some(SdkValue::List(items)) => items.len(),
        _ => panic!("no Commit list: {listed:?}"),
    }
}

/// The routed write exchanges the trace saw so far.
fn routed(home: &Path) -> usize {
    std::fs::read_to_string(home.join("trace.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| json::parse(line).ok())
        .filter(|line| {
            line.get_text("event") == Some("worker.connection")
                && line.get_text("op") == Some("store.write")
                && line.get_bool("sent")
        })
        .count()
}

fn read(home: &Path, branch: &str, path: &str) -> String {
    let value = ok(home, &["file", "read", "shop/api", branch, path], b"");
    value.get_text("text").unwrap_or_default().to_owned()
}

fn size(home: &Path, branch: &str, path: &str) -> i64 {
    let value = ok(home, &["file", "stat", "shop/api", branch, path], b"");
    value.get_int("size").unwrap()
}

fn patterned(len: usize) -> Vec<u8> {
    let mut state = 0x9e37_79b9_u32;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect()
}

fn drain(home: &Fixture) {
    if home.paths.control.exists() {
        home.drain();
    }
    let end = Instant::now() + Duration::from_secs(10);
    while home.paths.socket.exists() {
        assert!(Instant::now() < end, "worker did not drain");
        std::thread::sleep(Duration::from_millis(10));
    }
}

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

fn retained(home: &Path) -> Vec<SdkValue> {
    let dir = home.join("machine").join(host()).join("retained");
    let mut records: Vec<_> = std::fs::read_dir(&dir)
        .map(|entries| entries.filter_map(Result::ok).map(|e| e.path()).collect())
        .unwrap_or_else(|_| Vec::<PathBuf>::new());
    records.sort();
    records
        .iter()
        .map(|file| json::parse(&std::fs::read_to_string(file).unwrap()).unwrap())
        .collect()
}

fn host() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos-host"
    } else {
        "linux-host"
    }
}

fn wait_for<T>(what: &str, mut ready: impl FnMut() -> Option<T>) -> T {
    let end = Instant::now() + WAIT;
    loop {
        if let Some(value) = ready() {
            return value;
        }
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn retained_check(home: &Path) -> SdkValue {
    retained_report(home).0
}

fn retained_report(home: &Path) -> (SdkValue, SdkValue) {
    let report = ok(home, &["machine", "doctor", host()], b"");
    let Some(SdkValue::List(checks)) = report.get("tree").and_then(|tree| tree.get("checks"))
    else {
        panic!("{report:?}");
    };
    let check = checks
        .iter()
        .find(|check| check.get_text("check") == Some("machine.worker.retained"))
        .cloned()
        .unwrap_or_else(|| panic!("no retained check: {report:?}"));
    (check, report)
}

/// One raw streamed write on the worker's socket, its answer never read.
fn unread_write(home: &Fixture, id: &str, content: &[u8]) {
    let exchange = home.dial(false);
    let socket: &UnixStream = exchange.socket();
    let request = SdkValue::map([
        ("op", SdkValue::text("store.write")),
        (
            "args",
            SdkValue::map([
                ("method", SdkValue::text("fs.write")),
                ("store", SdkValue::text("shop")),
                ("project", SdkValue::text("api")),
                ("body", SdkValue::text("content")),
                (
                    "fields",
                    SdkValue::map([
                        ("branch", SdkValue::text("work")),
                        ("path", SdkValue::text("/unread")),
                        ("streamed", SdkValue::Bool(true)),
                    ]),
                ),
            ]),
        ),
        (
            "generation",
            exchange.hello.get("generation").unwrap().clone(),
        ),
        ("request_id", SdkValue::text(id)),
    ]);
    let frame = |bytes: &[u8], attached: bool| {
        let mut out = ((bytes.len() as u32) | if attached { 1 << 31 } else { 0 })
            .to_be_bytes()
            .to_vec();
        out.extend_from_slice(bytes);
        out
    };
    let mut stream = socket;
    stream
        .write_all(&frame(json::print(&request).as_bytes(), false))
        .unwrap();
    for piece in content.chunks(256 << 10) {
        stream.write_all(&frame(piece, true)).unwrap();
    }
    stream.write_all(&frame(&[], true)).unwrap();
    let end = format!(r#"{{"end":{{"changes":0,"bytes":{}}}}}"#, content.len());
    stream.write_all(&frame(end.as_bytes(), false)).unwrap();
    drop(exchange);
}

#[test]
fn lost_inputs_and_unread_answers_leave_inspectable_evidence() {
    let home = prepare(&["work"]);
    let path = &home.paths.home;
    let before = commits(path, "shop/api", "work");
    assert_eq!(
        retained_check(path).get_text("outcome"),
        Some("pass"),
        "{:?}",
        retained_check(path)
    );

    // SIGKILL mid-stream: nothing commits, the worker keeps the outcome
    let mut child = cli(path, &["file", "write", "shop/api", "work", "/killed"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    input.write_all(&patterned(2 << 20)).unwrap();
    wait_for("the stream to reach the worker", || {
        (home.ping().get_int("admitted") >= Some(2)).then_some(())
    });
    std::thread::sleep(Duration::from_millis(300));
    child.kill().unwrap();
    child.wait().unwrap();
    drop(input);
    let killed = wait_for("the killed stream's record", || {
        let records = retained(path);
        (!records.is_empty()).then_some(records)
    });
    assert_eq!(killed.len(), 1, "{killed:?}");
    assert_eq!(killed[0].get_text("kind"), Some("undelivered"));
    let outcome = killed[0].get("outcome").unwrap();
    assert_eq!(outcome.get_text("code"), Some("Unavailable"), "{outcome:?}");
    assert!(outcome
        .get_text("message")
        .unwrap()
        .contains("nothing was committed"));
    assert_eq!(commits(path, "shop/api", "work"), before);

    // a whole input whose answer is never read: committed, and kept
    unread_write(
        &home,
        "00000000000000000000000000c0ffee",
        &patterned(1 << 20),
    );
    let records = wait_for("the unread answer's record", || {
        let records = retained(path);
        (records.len() == 2).then_some(records)
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
    assert_eq!(retained(path).len(), 2);
    let (check, report) = retained_report(path);
    assert_eq!(check.get_text("outcome"), Some("notice"), "{check:?}");
    let report = json::print(&report);
    assert!(
        report.contains("c0ffee") && report.contains("undelivered"),
        "{report}"
    );
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
