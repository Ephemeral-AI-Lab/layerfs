//! The worker-connection trace (#535 phase 8 item 6, R1): with
//! `LAYERFS_TRACE` set, each connection to a machine's worker appends one
//! JSON line naming its request and whether it was sent; one worker call is
//! one line.
use layerfs_bridge::control::SdkValue;
use layerfs_machine::worker::{
    self, read_frame, read_frame_within, write_frame, Endpoint, Hello, MAX_HELLO, PROTOCOL,
};
use std::net::TcpListener;
use std::time::Duration;

const DEPLOYED: &str = "abcd0123";

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
    let dir = std::env::temp_dir().join(format!("layerfs-trace-{}", std::process::id()));
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
