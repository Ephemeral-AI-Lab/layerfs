//! The worker-connection trace (#535 phase 8 item 6, R1): with
//! `LAYERFS_TRACE` set, each connection to a machine's worker appends one
//! JSON line naming its request and whether it was sent; one worker call is
//! one line. With `LAYERFS_TRACE_STEP` set too, a call appends one line per
//! step, the worker's sent back in its answer; unset, none, and the answer
//! is the same.
use layerfs_bridge::control::SdkValue;
use layerfs_home::trace;
use layerfs_machine::worker::{
    self, read_frame, read_frame_within, write_frame, Endpoint, Hello, MAX_HELLO, PROTOCOL,
};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

const DEPLOYED: &str = "abcd0123";

/// The tests here set the process's trace settings: one at a time.
static SETTINGS: Mutex<()> = Mutex::new(());

/// A fresh folder `tag` holding a bundle of [`DEPLOYED`], named by
/// `LAYERFS_BUNDLE`, and its trace file, named by `LAYERFS_TRACE`.
fn traced(tag: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("layerfs-trace-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("bundle.json"),
        format!("{{\"hash\":\"{DEPLOYED}\"}}"),
    )
    .unwrap();
    let trace = dir.join("trace.jsonl");
    std::env::set_var("LAYERFS_BUNDLE", &dir);
    std::env::set_var("LAYERFS_TRACE", &trace);
    (dir, trace)
}

/// A stand-in worker of this build answering one request `{ value: "served" }`.
fn stand_in() -> Endpoint {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let wait = Some(Duration::from_secs(10));
        stream.set_read_timeout(wait).unwrap();
        let _ = read_frame_within(&mut stream, MAX_HELLO);
        let hello = Hello {
            protocol: Some(PROTOCOL),
            bundle: Some(DEPLOYED.to_owned()),
        };
        write_frame(&mut stream, &hello.to_frame()).unwrap();
        if read_frame(&mut stream).ok().flatten().is_some() {
            let value = SdkValue::map([("value", SdkValue::text("served"))]);
            let _ = write_frame(&mut stream, &value);
        }
    });
    Endpoint {
        address,
        secret: "k".to_owned(),
    }
}

#[test]
fn one_worker_call_is_one_traced_connection() {
    let _settings = SETTINGS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    std::env::remove_var("LAYERFS_TRACE_STEP");
    let (dir, trace) = traced("connection");

    let endpoint = stand_in();
    let outcome = worker::call(&endpoint, "file.read", SdkValue::Null, &mut |_| {});
    assert_eq!(outcome.unwrap(), SdkValue::text("served"));
    let text = std::fs::read_to_string(&trace).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 1, "{text}");
    for part in [
        "\"event\":\"worker.connection\"",
        "\"op\":\"file.read\"",
        &format!("\"worker\":\"{}\"", endpoint.address),
        "\"sent\":true",
        "\"ms\":",
    ] {
        assert!(lines[0].contains(part), "{part} not in {}", lines[0]);
    }

    // a worker that is not reached: one line, nothing sent
    let gone = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let endpoint = Endpoint {
        address: gone,
        secret: "k".to_owned(),
    };
    let refusal = worker::call(&endpoint, "file.stat", SdkValue::Null, &mut |_| {}).unwrap_err();
    assert_eq!(refusal.code, "Unavailable", "{refusal:?}");
    let text = std::fs::read_to_string(&trace).unwrap();
    let last = text.lines().nth(1).unwrap();
    assert_eq!(text.lines().count(), 2, "{text}");
    assert!(
        last.contains("\"op\":\"file.stat\"") && last.contains("\"sent\":false"),
        "{last}"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

/// This build's worker, serving under a new root in `dir`.
fn served(dir: &Path) -> Endpoint {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let root = dir.join("root");
    std::thread::spawn(move || {
        layerfs_control::worker_server::serve(listener, "k".to_owned(), root)
    });
    Endpoint {
        address,
        secret: "k".to_owned(),
    }
}

/// The answer frame of one raw `worker.ping` request `request`.
fn raw_ping(endpoint: &Endpoint, request: SdkValue) -> SdkValue {
    let mut stream = TcpStream::connect(endpoint.address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    write_frame(
        &mut stream,
        &SdkValue::map([("secret", SdkValue::text("k"))]),
    )
    .unwrap();
    read_frame_within(&mut stream, MAX_HELLO).unwrap().unwrap();
    write_frame(&mut stream, &request).unwrap();
    read_frame(&mut stream).unwrap().unwrap()
}

/// The `step` names of the step lines of `text` for call `op`.
fn steps(text: &str, op: &str) -> Vec<String> {
    let at = format!("\"op\":\"{op}\"");
    (text.lines())
        .filter(|line| line.contains("\"event\":\"step\"") && line.contains(&at))
        .map(|line| {
            assert!(
                line.contains("\"us\":") && line.contains("\"ms\":"),
                "{line}"
            );
            let name = line.split("\"step\":\"").nth(1).unwrap();
            name.split('"').next().unwrap().to_owned()
        })
        .collect()
}

#[test]
fn a_stepped_call_traces_its_steps_and_the_workers() {
    let _settings = SETTINGS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (dir, trace) = traced("steps");
    let endpoint = served(&dir);
    let ping = || {
        trace::stepped("worker.ping", || {
            worker::call(&endpoint, "worker.ping", SdkValue::Null, &mut |_| {})
        })
    };

    // unset: no step line, and the worker is not asked for its steps
    std::env::remove_var("LAYERFS_TRACE_STEP");
    let plain = ping().unwrap();
    let text = std::fs::read_to_string(&trace).unwrap();
    assert!(steps(&text, "worker.ping").is_empty(), "{text}");
    assert_eq!(text.lines().count(), 1, "the connection line only: {text}");
    let request = SdkValue::map([("op", SdkValue::text("worker.ping"))]);
    let answer = raw_ping(&endpoint, request.clone());
    assert!(answer.get("steps").is_none(), "{answer:?}");

    // set: the caller's steps, then the worker's; the answer is the same
    std::env::set_var("LAYERFS_TRACE_STEP", "1");
    let stepped = ping().unwrap();
    assert_eq!(stepped, plain);
    let text = std::fs::read_to_string(&trace).unwrap();
    let names = steps(&text, "worker.ping");
    let caller = [
        "route",
        "connect",
        "ready",
        "request",
        "answer",
        "worker.result",
    ];
    assert_eq!(names, caller, "{text}");
    let asked = layerfs_home::home::with_field(request, "trace_step", SdkValue::Bool(true));
    let answer = raw_ping(&endpoint, asked);
    let Some(SdkValue::List(sent)) = answer.get("steps") else {
        panic!("no steps in {answer:?}");
    };
    assert_eq!(sent[0].get_text("step"), Some("result"), "{answer:?}");
    assert!(
        sent[0].get_int("us").is_some_and(|us| us >= 0),
        "{answer:?}"
    );

    // a worker that does not know `trace_step` answers as before: the
    // caller's own steps only
    let old = stand_in();
    let outcome = trace::stepped("file.read", || {
        worker::call(&old, "file.read", SdkValue::Null, &mut |_| {})
    });
    assert_eq!(outcome.unwrap(), SdkValue::text("served"));
    let text = std::fs::read_to_string(&trace).unwrap();
    assert_eq!(steps(&text, "file.read"), &caller[..5], "{text}");
    std::env::remove_var("LAYERFS_TRACE_STEP");
    std::fs::remove_dir_all(&dir).unwrap();
}
