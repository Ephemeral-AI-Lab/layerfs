//! Diagnostic worker probes enforce one absolute socket budget, including trickling frames.
use layerfs_bridge::control::SdkValue;
use layerfs_machine::worker::{
    diagnostic_connect, frame_bytes, read_frame_within, write_frame, Endpoint, MAX_HELLO, PROTOCOL,
};
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

fn mock(body: impl FnOnce(TcpStream) + Send + 'static) -> (Endpoint, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = Endpoint {
        address: listener.local_addr().unwrap(),
        secret: "private-test-secret".into(),
    };
    let thread = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(1)))
                        .unwrap();
                    stream
                        .set_write_timeout(Some(Duration::from_secs(1)))
                        .unwrap();
                    body(stream);
                    return;
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(1))
                }
                Err(error) => panic!("mock worker accept: {error}"),
            }
        }
    });
    (endpoint, thread)
}
fn ready(protocol: i64) -> SdkValue {
    SdkValue::map([
        ("ready", SdkValue::Bool(true)),
        ("protocol", SdkValue::Int(protocol)),
    ])
}
fn auth(stream: &mut TcpStream) {
    assert_eq!(
        read_frame_within(stream, MAX_HELLO)
            .unwrap()
            .unwrap()
            .get_text("secret"),
        Some("private-test-secret")
    );
}

#[test]
fn authenticated_probe_sends_only_read_only_ping_with_capacity() {
    let (endpoint, server) = mock(|mut stream| {
        auth(&mut stream);
        write_frame(&mut stream, &ready(PROTOCOL)).unwrap();
        let request = read_frame_within(&mut stream, MAX_HELLO).unwrap().unwrap();
        assert_eq!(request.get_text("op"), Some("worker.ping"));
        assert_eq!(
            request.get("args").unwrap().get("capacity"),
            Some(&SdkValue::Bool(true))
        );
        write_frame(
            &mut stream,
            &SdkValue::map([(
                "value",
                SdkValue::map([
                    ("kind", SdkValue::text("worker")),
                    ("pid", SdkValue::Int(42)),
                    ("version", SdkValue::text(env!("CARGO_PKG_VERSION"))),
                    ("free_disk", SdkValue::Int(1234)),
                ]),
            )]),
        )
        .unwrap();
    });
    let worker = diagnostic_connect(&endpoint, Instant::now() + Duration::from_secs(1)).unwrap();
    assert_eq!(worker.ping().unwrap().get_int("free_disk"), Some(1234));
    server.join().unwrap();
}

#[test]
fn incompatible_worker_receives_no_request() {
    let (endpoint, server) = mock(|mut stream| {
        auth(&mut stream);
        write_frame(&mut stream, &ready(PROTOCOL - 1)).unwrap();
        assert_eq!(read_frame_within(&mut stream, MAX_HELLO).unwrap(), None);
    });
    let worker = diagnostic_connect(&endpoint, Instant::now() + Duration::from_secs(1)).unwrap();
    assert!(worker.ping().is_err());
    server.join().unwrap();
}

#[test]
fn denied_secret_and_false_ready_are_not_authenticated() {
    for frame in [
        SdkValue::map([(
            "refused",
            SdkValue::map([("code", SdkValue::text("Denied"))]),
        )]),
        SdkValue::map([("ready", SdkValue::Bool(false))]),
    ] {
        let (endpoint, server) = mock(move |mut stream| {
            auth(&mut stream);
            write_frame(&mut stream, &frame).unwrap();
        });
        assert!(diagnostic_connect(&endpoint, Instant::now() + Duration::from_secs(1)).is_err());
        server.join().unwrap();
    }
}

#[test]
fn trickling_hello_cannot_extend_absolute_deadline() {
    let (endpoint, server) = mock(|mut stream| {
        auth(&mut stream);
        let bytes = frame_bytes(&ready(PROTOCOL)).unwrap();
        for byte in bytes {
            if stream.write_all(&[byte]).is_err() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    });
    let started = Instant::now();
    assert!(diagnostic_connect(&endpoint, started + Duration::from_millis(70)).is_err());
    assert!(started.elapsed() < Duration::from_secs(1));
    server.join().unwrap();
}

#[test]
fn authenticated_worker_with_no_ping_answer_is_bounded() {
    let (endpoint, server) = mock(|mut stream| {
        auth(&mut stream);
        write_frame(&mut stream, &ready(PROTOCOL)).unwrap();
        assert!(read_frame_within(&mut stream, MAX_HELLO).unwrap().is_some());
        std::thread::sleep(Duration::from_millis(150));
    });
    let started = Instant::now();
    let worker = diagnostic_connect(&endpoint, started + Duration::from_millis(70)).unwrap();
    assert!(worker.ping().is_err());
    assert!(started.elapsed() < Duration::from_secs(1));
    server.join().unwrap();
}

#[test]
fn oversized_hello_is_refused_before_reading_body() {
    let (endpoint, server) = mock(|mut stream| {
        auth(&mut stream);
        stream
            .write_all(&((MAX_HELLO + 1) as u32).to_be_bytes())
            .unwrap();
    });
    let error = diagnostic_connect(&endpoint, Instant::now() + Duration::from_secs(1))
        .err()
        .unwrap();
    assert_eq!(error.code, "TooLarge");
    server.join().unwrap();
}
