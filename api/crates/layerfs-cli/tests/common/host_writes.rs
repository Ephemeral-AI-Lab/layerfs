//! Shared helpers of the CLI host-worker write tests (S2.2): CLI calls
//! with piped input, Commit counts, kept outcomes and raw streamed frames.
#![allow(dead_code)]
use super::fixture::Fixture;
use layerfs_api::json;
use layerfs_bridge::control::SdkValue;
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

pub const CLI: &str = env!("CARGO_BIN_EXE_layerfs");
pub const WAIT: Duration = Duration::from_secs(60);

pub fn cli(home: &Path, words: &[&str]) -> Command {
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

pub fn finish(mut child: Child) -> Output {
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
pub fn run(home: &Path, words: &[&str], stdin: &[u8]) -> (bool, SdkValue) {
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

/// [`run`], sampling the CLI's resident memory every few milliseconds:
/// also the largest sample, in bytes (a diagnostic, not a peak proof).
pub fn run_sampled(home: &Path, words: &[&str], stdin: &[u8]) -> (bool, SdkValue, u64) {
    let mut child = cli(home, words).stdin(Stdio::piped()).spawn().unwrap();
    let mut input = child.stdin.take().unwrap();
    let bytes = stdin.to_vec();
    let feeder = std::thread::spawn(move || {
        let _ = input.write_all(&bytes);
    });
    let (end, mut peak) = (Instant::now() + WAIT, 0);
    while child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < end, "a CLI call did not finish");
        peak = peak.max(resident(child.id()));
        std::thread::sleep(Duration::from_millis(2));
    }
    let output = child.wait_with_output().unwrap();
    feeder.join().unwrap();
    let text = if output.stdout.is_empty() {
        &output.stderr
    } else {
        &output.stdout
    };
    let value = json::parse(String::from_utf8_lossy(text).trim())
        .unwrap_or_else(|_| panic!("not JSON: {}", String::from_utf8_lossy(text)));
    (output.status.success(), value, peak)
}

/// A process's current resident bytes, or 0 once it is gone.
fn resident(pid: u32) -> u64 {
    #[cfg(target_os = "linux")]
    let kib = std::fs::read_to_string(format!("/proc/{pid}/status"))
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find_map(|line| line.strip_prefix("VmRSS:"))
                .and_then(|kib| kib.trim().trim_end_matches("kB").trim().parse().ok())
        });
    #[cfg(not(target_os = "linux"))]
    let kib = Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()
        .and_then(|out| String::from_utf8_lossy(&out.stdout).trim().parse().ok());
    kib.unwrap_or(0u64) * 1024
}

pub fn ok(home: &Path, words: &[&str], stdin: &[u8]) -> SdkValue {
    let (success, value) = run(home, words, stdin);
    assert!(success, "{words:?}: {value:?}");
    value
}

pub fn refused(home: &Path, words: &[&str], stdin: &[u8], code: &str) -> SdkValue {
    let (success, value) = run(home, words, stdin);
    assert!(!success, "{words:?} succeeded: {value:?}");
    assert_eq!(value.get_text("error"), Some(code), "{value:?}");
    value
}

pub fn prepare(branches: &[&str]) -> Fixture {
    prepare_in(Fixture::new(), branches)
}

/// Store `shop`, project `shop/api` and `branches` in `home`.
pub fn prepare_in(home: Fixture, branches: &[&str]) -> Fixture {
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

pub fn commits(home: &Path, project: &str, branch: &str) -> usize {
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
pub fn routed(home: &Path) -> usize {
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

pub fn read(home: &Path, branch: &str, path: &str) -> String {
    let value = ok(home, &["file", "read", "shop/api", branch, path], b"");
    value.get_text("text").unwrap_or_default().to_owned()
}

pub fn size(home: &Path, branch: &str, path: &str) -> i64 {
    let value = ok(home, &["file", "stat", "shop/api", branch, path], b"");
    value.get_int("size").unwrap()
}

pub fn patterned(len: usize) -> Vec<u8> {
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

pub fn drain(home: &Fixture) {
    if home.paths.control.exists() {
        home.drain();
    }
    let end = Instant::now() + Duration::from_secs(10);
    while home.paths.socket.exists() {
        assert!(Instant::now() < end, "worker did not drain");
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub fn retained(home: &Path) -> Vec<SdkValue> {
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

pub fn host() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos-host"
    } else {
        "linux-host"
    }
}

pub fn wait_for<T>(what: &str, mut ready: impl FnMut() -> Option<T>) -> T {
    let end = Instant::now() + WAIT;
    loop {
        if let Some(value) = ready() {
            return value;
        }
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

pub fn retained_check(home: &Path) -> SdkValue {
    retained_report(home).0
}

pub fn retained_report(home: &Path) -> (SdkValue, SdkValue) {
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

/// One frame: a 4-byte big-endian length, the top bit for data.
pub fn frame(bytes: &[u8], attached: bool) -> Vec<u8> {
    let mut out = ((bytes.len() as u32) | if attached { 1 << 31 } else { 0 })
        .to_be_bytes()
        .to_vec();
    out.extend_from_slice(bytes);
    out
}

/// A framed `store.write` request on `exchange`'s connection.
pub fn write_request(
    exchange: &super::fixture::Exchange,
    id: &str,
    method: &str,
    body: &str,
    fields: SdkValue,
) -> Vec<u8> {
    let request = SdkValue::map([
        ("op", SdkValue::text("store.write")),
        (
            "args",
            SdkValue::map([
                ("method", SdkValue::text(method)),
                ("store", SdkValue::text("shop")),
                ("project", SdkValue::text("api")),
                ("body", SdkValue::text(body)),
                ("fields", fields),
            ]),
        ),
        (
            "generation",
            exchange.hello.get("generation").unwrap().clone(),
        ),
        ("request_id", SdkValue::text(id)),
    ]);
    frame(json::print(&request).as_bytes(), false)
}

/// A streamed `file write` to `/unread` on Branch `work`: its request,
/// content and end frames, sent together (the worker reads the input only
/// after its `go`, which the raw caller need not wait for).
pub fn content_frames(exchange: &super::fixture::Exchange, id: &str, content: &[u8]) -> Vec<u8> {
    let mut out = content_request(exchange, id, "/unread");
    out.extend(content_tail(content));
    out
}

/// The request frame of a streamed `file write` to `path` on `work`.
pub fn content_request(exchange: &super::fixture::Exchange, id: &str, path: &str) -> Vec<u8> {
    let fields = SdkValue::map([
        ("branch", SdkValue::text("work")),
        ("path", SdkValue::text(path)),
        ("streamed", SdkValue::Bool(true)),
    ]);
    write_request(exchange, id, "fs.write", "content", fields)
}

/// `content`'s data frames, its end and the input's end.
pub fn content_tail(content: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for piece in content.chunks(64 << 10) {
        out.extend(frame(piece, true));
    }
    out.extend(frame(&[], true));
    let end = format!(r#"{{"end":{{"changes":0,"bytes":{}}}}}"#, content.len());
    out.extend(frame(end.as_bytes(), false));
    out
}

fn next_frame(socket: &UnixStream) -> SdkValue {
    layerfs_api::worker::read_host_frame(
        socket,
        layerfs_api::worker::HOST_MAX_REPLY,
        Some(Instant::now() + WAIT),
    )
    .unwrap()
    .expect("a frame")
}

/// Waits for the worker's `go` for request `id`: a write thread reads it.
pub fn read_go(socket: &UnixStream, id: &str) {
    let frame = next_frame(socket);
    assert!(
        frame.get_text("request_id") == Some(id) && frame.get_bool("go"),
        "expected go: {frame:?}"
    );
}

/// The answer to a raw request on `socket`, after any `go` frame.
pub fn raw_answer(socket: &UnixStream) -> SdkValue {
    loop {
        let frame = next_frame(socket);
        if !frame.get_bool("go") {
            return frame;
        }
    }
}

/// One raw streamed write on the worker's socket, its answer never read.
pub fn unread_write(home: &Fixture, id: &str, content: &[u8]) {
    let exchange = home.dial_writes();
    let mut socket: &UnixStream = exchange.socket();
    socket
        .write_all(&content_frames(&exchange, id, content))
        .unwrap();
    drop(exchange);
}
