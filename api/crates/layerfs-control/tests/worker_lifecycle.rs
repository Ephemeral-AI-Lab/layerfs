//! A worker never stops by itself (#535 D11): it has no idle clock, so a
//! worker that answered nothing for any length of time is the same as one
//! that just started. With no clock left to simulate, this checks the
//! observable part: after its connections close the server keeps serving,
//! and `serve` takes no idle period.
use layerfs_bridge::control::SdkValue;
use layerfs_control::machine::worker::{self, Endpoint};
use std::net::TcpListener;

#[test]
fn a_worker_with_no_requests_keeps_serving() {
    let root = std::env::temp_dir().join(format!("layerfs-worker-life-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let served = root.clone();
    // `serve` has no idle parameter: only the process ending stops it
    let server = std::thread::spawn(move || worker::serve(listener, "s".to_owned(), served));
    let endpoint = Endpoint {
        address,
        secret: "s".to_owned(),
    };
    let ping = || {
        worker::call(
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
    let denied = worker::call(
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
