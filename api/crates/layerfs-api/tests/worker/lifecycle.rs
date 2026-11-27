//! A worker never stops by itself (#535 D11): it has no idle clock, so a
//! worker that answered nothing for any length of time is the same as one
//! that just started. With no clock left to simulate, this checks the
//! observable part: after its connections close the server keeps serving,
//! and `serve` takes no idle period.
use layerfs_machine::route::{self, Endpoint};
use layerfs_protocol::sdk::SdkValue;
use std::net::TcpListener;

/// One ping of `endpoint`, or why it did not answer yet.
fn ping(endpoint: &Endpoint) -> Result<SdkValue, layerfs_protocol::sdk::SdkRefusal> {
    route::call(
        endpoint,
        "worker.ping",
        SdkValue::Map(Vec::new()),
        &mut |_| {},
    )
}

#[test]
fn a_worker_serves_with_the_incarnation_the_host_persisted() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-worker-start-supplied-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let served = root.clone();
    let token = "p1:worker:0000000000000001:aa";
    std::thread::spawn(move || {
        layerfs_api::worker::serve::serve_with_incarnation(
            listener,
            "s".to_owned(),
            served,
            Some(token.to_owned()),
        )
    });
    let endpoint = Endpoint {
        address,
        secret: "s".to_owned(),
    };
    let mut answered = None;
    for _ in 0..200 {
        if let Ok(answer) = ping(&endpoint) {
            answered = Some(answer);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let answered = answered.expect("the worker answers");
    assert_eq!(answered.get_text("incarnation"), Some(token));
    // the same value was durable before the first request was served
    let recorded = layerfs_home::disk::read_record(&root.join("worker-incarnation.json"))
        .expect("the worker's incarnation record is readable")
        .expect("the worker recorded its incarnation");
    assert_eq!(recorded.get_text("incarnation"), Some(token));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_worker_without_a_supplied_incarnation_mints_and_records_one() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-worker-start-minted-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let served = root.clone();
    std::thread::spawn(move || layerfs_api::worker::serve::serve(listener, "s".to_owned(), served));
    let endpoint = Endpoint {
        address,
        secret: "s".to_owned(),
    };
    let mut answered = None;
    for _ in 0..200 {
        if let Ok(answer) = ping(&endpoint) {
            answered = Some(answer);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let answered = answered.expect("the worker answers");
    let reported = answered.get_text("incarnation").expect("an incarnation");
    assert!(reported.starts_with("p1:worker:"), "{reported}");
    let recorded = layerfs_home::disk::read_record(&root.join("worker-incarnation.json"))
        .expect("the worker's incarnation record is readable")
        .expect("the worker recorded its incarnation");
    assert_eq!(recorded.get_text("incarnation"), Some(reported));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_worker_with_no_requests_keeps_serving() {
    let root = std::env::temp_dir().join(format!("layerfs-worker-life-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let served = root.clone();
    // `serve` has no idle parameter: only the process ending stops it
    let server = std::thread::spawn(move || {
        layerfs_api::worker::serve::serve(listener, "s".to_owned(), served)
    });
    let endpoint = Endpoint {
        address,
        secret: "s".to_owned(),
    };
    let ping = || {
        route::call(
            &endpoint,
            "worker.ping",
            SdkValue::Map(Vec::new()),
            &mut |_| {},
        )
    };
    ping().unwrap();
    // a probe that is refused, then a quiet connection that closes at once
    let wrong = Endpoint {
        address,
        secret: "wrong".to_owned(),
    };
    let denied = route::call(
        &wrong,
        "worker.ping",
        SdkValue::Map(Vec::new()),
        &mut |_| {},
    );
    assert_eq!(denied.unwrap_err().code, "Denied");
    drop(std::net::TcpStream::connect(address).unwrap());
    assert!(!server.is_finished(), "the worker stopped by itself");
    ping().unwrap();
    assert!(!server.is_finished());
    let _ = std::fs::remove_dir_all(&root);
}
