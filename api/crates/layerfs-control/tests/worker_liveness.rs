//! The worker's frame-level liveness waits (#545 B9-209): a worker that
//! dies mid-wait, or trickles a frame instead of sending it, must surface
//! `Unknown` in bounded time, and a large answer that keeps arriving must not
//! be cut off. The operation-level wait (claim, then progress; a beat does
//! not refresh it) is in `worker_progress.rs`.
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
fn a_trickled_answer_is_unknown_within_its_wait() {
    // the worker claimed, then sends its answer one byte at a time: a frame
    // that never really progresses is dead, not slow, and the whole-frame
    // rule bounds it instead of each socket read extending it forever
    let wait = Duration::from_secs(1);
    let endpoint = stand_in(true, move |mut stream| {
        let frame =
            worker::frame_bytes(&SdkValue::map([("value", SdkValue::text("trickled"))])).unwrap();
        for byte in frame {
            if stream.write_all(&[byte]).is_err() {
                return;
            }
            std::thread::sleep(wait / 3);
        }
    });
    let request = worker::request("worker.ping", SdkValue::Null).unwrap();
    let started = Instant::now();
    let refusal = worker::send(&endpoint, "worker.ping", &request, &mut |_| {}, Some(wait))
        .unwrap()
        .unwrap_err();
    assert_eq!(refusal.code, "Unknown", "{refusal:?}");
    assert!(
        started.elapsed() < wait + Duration::from_secs(3),
        "{:?}",
        started.elapsed()
    );
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
        started.elapsed() > Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
}
