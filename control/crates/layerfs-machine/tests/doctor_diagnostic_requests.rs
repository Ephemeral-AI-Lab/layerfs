//! Allowlisted diagnostic streaming preserves deadlines and original uncertainty.
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::refused;
use layerfs_machine::worker::{
    diagnostic_connect, read_frame, read_frame_within, write_frame, DiagnosticOperation, Endpoint,
    Hello, MAX_HELLO, PROTOCOL,
};
use std::net::TcpListener;
use std::time::{Duration, Instant};
fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}
fn server(
    delay: Duration,
    events: bool,
    lost: bool,
) -> (Endpoint, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let thread = std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(2);
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < until =>
                {
                    std::thread::sleep(Duration::from_millis(2))
                }
                Err(error) => panic!("bounded diagnostic fixture accept: {error}"),
            }
        };
        socket.set_nonblocking(false).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let hello = read_frame_within(&mut socket, MAX_HELLO).unwrap().unwrap();
        assert_eq!(hello.get_text("secret"), Some("k"));
        write_frame(
            &mut socket,
            &Hello {
                protocol: Some(PROTOCOL),
                bundle: None,
            }
            .to_frame(),
        )
        .unwrap();
        let request = read_frame(&mut socket).unwrap().unwrap();
        let op = request.get_text("op").unwrap().to_owned();
        std::thread::sleep(delay);
        if lost {
            return op;
        }
        if events {
            let _ = write_frame(&mut socket, &SdkValue::map([("event", t("observation"))]));
        }
        let _ = write_frame(&mut socket, &SdkValue::map([("value", t("done"))]));
        op
    });
    (
        Endpoint {
            address,
            secret: "k".into(),
        },
        thread,
    )
}
#[test]
fn full_store_audit_can_outlive_hello_deadline_and_stream_bounded_observations() {
    let (endpoint, server) = server(Duration::from_millis(100), true, false);
    let worker = diagnostic_connect(&endpoint, Instant::now() + Duration::from_millis(70)).unwrap();
    let mut events = Vec::new();
    let result = worker
        .diagnose_store(SdkValue::Null, None, &mut |event| {
            events.push(event);
            Ok(())
        })
        .unwrap();
    assert_eq!(result, t("done"));
    assert_eq!(events, vec![t("observation")]);
    assert_eq!(server.join().unwrap(), "store.diagnose");
}
#[test]
fn finite_audit_deadline_and_lost_answer_preserve_unknown_without_resend() {
    for lost in [false, true] {
        let (endpoint, server) = server(Duration::from_millis(100), false, lost);
        let worker =
            diagnostic_connect(&endpoint, Instant::now() + Duration::from_secs(1)).unwrap();
        let result = worker
            .diagnose_store(
                SdkValue::Null,
                Some(Instant::now() + Duration::from_millis(40)),
                &mut |_| Ok(()),
            )
            .unwrap_err();
        assert_eq!(result.code, "Unknown");
        assert_eq!(server.join().unwrap(), "store.diagnose");
    }
}
#[test]
fn sandbox_operation_is_allowlisted_and_event_refusal_is_not_swallowed() {
    let (endpoint, server) = server(Duration::ZERO, true, false);
    let worker = diagnostic_connect(&endpoint, Instant::now() + Duration::from_secs(1)).unwrap();
    let result = worker
        .diagnose(DiagnosticOperation::Sandbox, SdkValue::Null, &mut |_| {
            Err(refused("Cancelled", "explicit cancellation"))
        })
        .unwrap_err();
    assert_eq!(result.code, "Cancelled");
    assert_eq!(server.join().unwrap(), "doctor.sandbox");
}
