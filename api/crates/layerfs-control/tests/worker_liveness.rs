//! The worker's liveness waits (#545 B9-209): a worker that accepts a
//! request and then stalls, or dies mid-wait, must surface `Unknown` in
//! bounded time; a long operation that keeps beating must not be cut off.
use layerfs_bridge::control::SdkValue;
use layerfs_control::machine::worker::{
    self, read_frame, read_frame_within, write_frame, Endpoint, Hello, MAX_HELLO, PROTOCOL,
};
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

/// A stand-in worker that greets the caller, reads one request, and hands
/// the stream to `then`. With `claim` it writes the worker's claim first.
fn stand_in(claim: bool, then: impl FnOnce(TcpStream) + Send + 'static) -> Endpoint {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let _ = read_frame_within(&mut stream, MAX_HELLO);
        let hello = Hello {
            protocol: Some(PROTOCOL),
            bundle: None,
        };
        write_frame(&mut stream, &hello.to_frame()).unwrap();
        let _ = read_frame(&mut stream);
        if claim {
            let claimed = SdkValue::map([("claimed", SdkValue::Bool(true))]);
            write_frame(&mut stream, &claimed).unwrap();
        }
        then(stream);
    });
    Endpoint {
        address,
        secret: "k".to_owned(),
    }
}

fn beat() -> SdkValue {
    SdkValue::map([("beat", SdkValue::Bool(true))])
}

#[test]
fn a_worker_that_reads_a_request_and_never_claims_is_unknown_in_time() {
    // it accepted the connection and read the request, then went quiet
    let endpoint = stand_in(false, |stream| {
        std::thread::sleep(worker::CLAIM_WAIT + Duration::from_secs(5));
        drop(stream);
    });
    let started = Instant::now();
    let refusal = worker::call(&endpoint, "store.call", SdkValue::Null, &mut |_| {}).unwrap_err();
    let waited = started.elapsed();
    // the request was sent: its outcome is not known, and it is not resent
    assert_eq!(refusal.code, "Unknown", "{refusal:?}");
    assert!(waited >= worker::CLAIM_WAIT, "{waited:?}");
    assert!(
        waited < worker::CLAIM_WAIT + Duration::from_secs(4),
        "{waited:?}"
    );
}

#[test]
fn a_worker_that_dies_mid_wait_is_unknown_at_once() {
    // the request was sent, then the worker's connection went
    let endpoint = stand_in(true, drop);
    let started = Instant::now();
    let refusal = worker::call(&endpoint, "store.call", SdkValue::Null, &mut |_| {}).unwrap_err();
    assert_eq!(refusal.code, "Unknown", "{refusal:?}");
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn a_long_operation_is_not_cut_off_while_the_worker_beats() {
    // longer than the claim and heartbeat waits together: only the beats
    // keep the call alive, and they do, however long it runs
    let runs = worker::CLAIM_WAIT + Duration::from_secs(2);
    let endpoint = stand_in(true, move |mut stream| {
        let until = Instant::now() + runs;
        while Instant::now() < until {
            std::thread::sleep(worker::HEARTBEAT);
            if write_frame(&mut stream, &beat()).is_err() {
                return;
            }
        }
        let value = SdkValue::map([("value", SdkValue::text("done"))]);
        let _ = write_frame(&mut stream, &value);
    });
    let started = Instant::now();
    let outcome = worker::call(&endpoint, "store.call", SdkValue::Null, &mut |_| {}).unwrap();
    assert_eq!(outcome, SdkValue::text("done"));
    assert!(started.elapsed() >= runs, "{:?}", started.elapsed());
}

#[test]
fn a_large_answer_is_not_cut_off_while_it_keeps_arriving() {
    // the body takes longer than the heartbeat wait to transfer, but it
    // keeps arriving: only silence is dead, not a slow large answer
    const BODY: usize = 1 << 20;
    const CHUNK: usize = 64 << 10;
    let endpoint = stand_in(true, |mut stream| {
        let value = SdkValue::text("x".repeat(BODY));
        let frame = worker::frame_bytes(&SdkValue::map([("value", value)])).unwrap();
        let (length, body) = frame.split_at(4);
        stream.write_all(length).unwrap();
        for chunk in body.chunks(CHUNK) {
            stream.write_all(chunk).unwrap();
            std::thread::sleep(Duration::from_millis(800));
        }
    });
    let started = Instant::now();
    let outcome = worker::call(&endpoint, "store.call", SdkValue::Null, &mut |_| {}).unwrap();
    assert_eq!(outcome, SdkValue::text("x".repeat(BODY)));
    assert!(
        started.elapsed() > worker::HEARTBEAT_WAIT,
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn the_library_worker_claims_a_request_before_it_answers() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let root = std::env::temp_dir().join(format!("layerfs-claim-root-{}", std::process::id()));
    let served = root.clone();
    std::thread::spawn(move || worker::serve(listener, "k".to_owned(), served));
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let secret = SdkValue::map([("secret", SdkValue::text("k"))]);
    write_frame(&mut stream, &secret).unwrap();
    let hello = read_frame(&mut stream).unwrap().unwrap();
    assert!(hello.get("ready").is_some(), "{hello:?}");
    let request = worker::request("worker.ping", SdkValue::Null).unwrap();
    stream.write_all(&request).unwrap();
    let claimed = read_frame(&mut stream).unwrap().unwrap();
    assert!(claimed.get("claimed").is_some(), "{claimed:?}");
    let answer = read_frame(&mut stream).unwrap().unwrap();
    assert!(answer.get("value").is_some(), "{answer:?}");
    let _ = std::fs::remove_dir_all(root);
}
