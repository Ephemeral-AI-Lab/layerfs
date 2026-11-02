//! A call at the end of a worker's idle window is answered (#533 final
//! review, finding 1): the stop decision and the admission of an
//! authenticated connection take one lock, so a connection admitted before
//! the decision holds the worker up until its request is answered, however
//! late the request comes. A connection that comes after the decision is
//! refused `Unavailable` before its request is sent. Its own test binary:
//! the idle clock is one per process.
use layerfs_bridge::control::SdkValue;
use layerfs_control::machine::worker;
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

const IDLE: Duration = Duration::from_millis(800);

#[test]
fn a_call_admitted_at_the_edge_of_the_idle_window_is_answered() {
    let root = std::env::temp_dir().join(format!("layerfs-idle-edge-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let served = {
        let root = root.clone();
        std::thread::spawn(move || {
            worker::serve_until_idle(
                listener,
                "s".to_owned(),
                root,
                IDLE,
                Duration::from_millis(20),
            )
        })
    };
    let begun = Instant::now();

    // admitted just before the idle time runs out: the secret answered
    std::thread::sleep(IDLE - Duration::from_millis(150));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(1)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    worker::write_frame(
        &mut stream,
        &SdkValue::map([("secret", SdkValue::text("s"))]),
    )
    .unwrap();
    let ready = worker::read_frame(&mut stream).unwrap().unwrap();
    assert_eq!(ready.get("ready"), Some(&SdkValue::Bool(true)), "{ready:?}");

    // the request comes well after the idle time, and is still answered by
    // a worker that has not stopped
    std::thread::sleep(IDLE * 2);
    assert!(
        !served.is_finished(),
        "the worker stopped with a connection open"
    );
    let request = SdkValue::map([
        ("op", SdkValue::text("store.call")),
        ("args", SdkValue::Null),
    ]);
    worker::write_frame(&mut stream, &request).unwrap();
    let answer = worker::read_frame(&mut stream)
        .unwrap()
        .expect("an answer, not a closed connection");
    let refused = answer
        .get("refused")
        .unwrap_or_else(|| panic!("{answer:?}"));
    assert_eq!(refused.get_text("code"), Some("InvalidInput"), "{answer:?}");
    let answered = begun.elapsed();
    drop(stream);

    // the request did work: the worker stops an idle time after it
    served.join().unwrap().unwrap();
    let took = begun.elapsed();
    assert!(
        took >= answered + IDLE - Duration::from_millis(50),
        "stopped after {took:?}"
    );
    assert!(took < answered + IDLE * 3, "stopped only after {took:?}");

    // after the decision, a connection is refused before its request; the
    // server thread still listens here, as the process has not exited
    let endpoint = worker::Endpoint {
        address,
        secret: "s".to_owned(),
    };
    let late = worker::call(&endpoint, "store.call", SdkValue::Null, &mut |_| {}).unwrap_err();
    assert_eq!(late.code, "Unavailable", "{late:?}");
    assert!(
        late.message.contains("stopping") && late.message.contains("not sent"),
        "{}",
        late.message
    );
    let _ = std::fs::remove_dir_all(root);
}
