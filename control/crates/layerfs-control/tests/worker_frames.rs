//! The worker's frames (#533 review, group calls): the secret comes first
//! in a small frame, so a wrong secret or an oversized first frame is
//! refused before a large body is read; reads are bounded in time; a
//! refusal before the request was sent is a plain refusal, not `Unknown`.
use layerfs_bridge::control::SdkValue;
use layerfs_machine::worker::{self, read_frame_within, write_frame, Endpoint, MAX_HELLO};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

fn root(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("layerfs-frames-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// A worker serving `secret` on a loopback port.
fn worker(tag: &str, secret: &str) -> (Endpoint, std::path::PathBuf) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let dir = root(tag);
    let (secret_owned, served) = (secret.to_owned(), dir.clone());
    std::thread::spawn(move || {
        layerfs_control::worker_server::serve(listener, secret_owned, served)
    });
    let endpoint = Endpoint {
        address,
        secret: secret.to_owned(),
    };
    (endpoint, dir)
}

#[test]
fn a_frame_longer_than_allowed_is_refused_from_its_length() {
    // only the length is there: reading any body would fail Unavailable
    let mut input = std::io::Cursor::new((1u32 << 30).to_be_bytes().to_vec());
    let refused = read_frame_within(&mut input, MAX_HELLO).unwrap_err();
    assert_eq!(refused.code, "TooLarge");
}

#[test]
fn a_large_first_frame_is_refused_before_its_body() {
    let (endpoint, dir) = worker("large", "k");
    let mut stream = TcpStream::connect(endpoint.address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    // claims 256 MiB and sends none of it: the worker closes at once
    stream.write_all(&(256u32 << 20).to_be_bytes()).unwrap();
    let started = Instant::now();
    let mut byte = [0u8; 1];
    let read = stream.read(&mut byte);
    assert!(matches!(read, Ok(0)) || read.is_err(), "{read:?}");
    assert!(started.elapsed() < Duration::from_secs(3), "closed at once");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_wrong_secret_is_refused_before_the_request_is_read() {
    let (endpoint, dir) = worker("secret", "k");
    // raw: the refusal answers the first frame, before any request
    let mut stream = TcpStream::connect(endpoint.address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write_frame(
        &mut stream,
        &SdkValue::map([("secret", SdkValue::text("no"))]),
    )
    .unwrap();
    let answer = read_frame_within(&mut stream, MAX_HELLO).unwrap().unwrap();
    let refused = answer.get("refused").expect("a refusal");
    assert_eq!(refused.get_text("code"), Some("Denied"));
    // through the client: a plain refusal, not Unknown
    let wrong = Endpoint {
        address: endpoint.address,
        secret: "no".to_owned(),
    };
    let refusal = worker::call(&wrong, "worker.ping", SdkValue::Null, &mut |_| {}).unwrap_err();
    assert_eq!(refusal.code, "Denied");
    // the right secret is answered
    let answer = worker::ping(&endpoint).unwrap();
    assert_eq!(answer.get_text("kind"), Some("worker"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_worker_that_does_not_answer_its_secret_is_unavailable_in_time() {
    // accepts and never answers
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let held = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        std::thread::sleep(worker::HELLO_WAIT + Duration::from_secs(5));
        drop(stream);
    });
    let endpoint = Endpoint {
        address,
        secret: "k".to_owned(),
    };
    let started = Instant::now();
    let refusal = worker::ping(&endpoint).unwrap_err();
    let waited = started.elapsed();
    assert_eq!(refusal.code, "Unavailable", "{refusal:?}");
    assert!(waited >= worker::HELLO_WAIT, "{waited:?}");
    assert!(
        waited < worker::HELLO_WAIT + Duration::from_secs(4),
        "{waited:?}"
    );
    drop(held);
}

#[test]
fn a_ping_whose_answer_never_comes_times_out_unknown() {
    // answers the secret, reads the request, then hangs
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = read_frame_within(&mut stream, MAX_HELLO);
        let ready = worker::Hello {
            protocol: Some(worker::PROTOCOL),
            bundle: None,
        };
        let _ = write_frame(&mut stream, &ready.to_frame());
        let _ = worker::read_frame(&mut stream);
        std::thread::sleep(worker::PING_WAIT + Duration::from_secs(5));
    });
    let endpoint = Endpoint {
        address,
        secret: "k".to_owned(),
    };
    let started = Instant::now();
    let refusal = worker::ping(&endpoint).unwrap_err();
    let waited = started.elapsed();
    // the request was sent: its outcome is not known
    assert_eq!(refusal.code, "Unknown", "{refusal:?}");
    assert!(
        waited < worker::PING_WAIT + Duration::from_secs(4),
        "{waited:?}"
    );
}

#[test]
fn the_worker_waits_for_the_client_to_close_after_its_answer() {
    // Apple Container's published port drops what it still holds for the
    // host when the guest closes first: the client must close first
    let (endpoint, dir) = worker("close", "k");
    let mut stream = TcpStream::connect(endpoint.address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write_frame(
        &mut stream,
        &SdkValue::map([("secret", SdkValue::text("k"))]),
    )
    .unwrap();
    let ready = read_frame_within(&mut stream, MAX_HELLO).unwrap().unwrap();
    assert!(ready.get("ready").is_some(), "{ready:?}");
    let request = worker::request("worker.ping", SdkValue::Null).unwrap();
    stream.write_all(&request).unwrap();
    let answer = worker::read_frame(&mut stream).unwrap().unwrap();
    assert!(answer.get("value").is_some(), "{answer:?}");
    // the answer is complete, and the worker has not closed
    stream
        .set_read_timeout(Some(Duration::from_millis(300)))
        .unwrap();
    let mut byte = [0u8; 1];
    let open = stream.read(&mut byte).unwrap_err();
    assert!(
        matches!(
            open.kind(),
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
        ),
        "{open:?}"
    );
    // once the client closes its side, the worker closes at once
    stream.shutdown(std::net::Shutdown::Write).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let started = Instant::now();
    let closed = stream.read(&mut byte);
    assert!(matches!(closed, Ok(0)) || closed.is_err(), "{closed:?}");
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "closed after the client"
    );
    let _ = std::fs::remove_dir_all(dir);
}
