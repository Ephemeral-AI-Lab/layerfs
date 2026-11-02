//! The worker's idle stop (#533 step 13): it stops serving only when no
//! call is in flight, no Store is open, no Sandbox is recorded under its
//! root (where the host Sandbox code records it), and the idle time has
//! passed since its last call.
use layerfs_api::home::{paths, write_record};
use layerfs_api::worker;
use layerfs_bridge::control::SdkValue;
use std::net::TcpListener;
use std::time::{Duration, Instant};

#[test]
fn an_idle_worker_stops_and_a_busy_one_does_not() {
    let root = std::env::temp_dir().join(format!("layerfs-idle-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    // a Sandbox the host Sandbox code recorded under the root keeps the
    // worker up, where a guest records it: store/<s>/project/<p>/sandboxes/<n>
    let sandbox = paths::project_dir(&root, "shop", "app").join("sandboxes/1/sandbox.record");
    write_record(&sandbox, &SdkValue::map([("name", SdkValue::text("1"))])).unwrap();
    assert!(!worker::idle(&root, Duration::ZERO, 0));
    std::fs::remove_dir_all(sandbox.parent().unwrap()).unwrap();
    assert!(worker::idle(&root, Duration::ZERO, 0));
    // the 1.1.0-style folder under machine/native is not where a guest
    // records its Sandboxes, and holds nothing up by itself
    let old = paths::machine_dir(&root, "native").join("sandbox/t1/record.json");
    write_record(&old, &SdkValue::map([("name", SdkValue::text("t1"))])).unwrap();
    assert!(worker::idle(&root, Duration::ZERO, 0));

    // a worker answered recently is not idle for a minute
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (r, started) = (root.clone(), Instant::now());
    let served = std::thread::spawn(move || {
        worker::serve_until_idle(
            listener,
            "s".to_owned(),
            r,
            Duration::from_millis(400),
            Duration::from_millis(50),
        )
    });
    let endpoint = worker::Endpoint {
        address,
        secret: "s".to_owned(),
    };
    worker::ping(&endpoint).unwrap();
    assert!(!worker::idle(&root, Duration::from_secs(60), 0));
    served.join().unwrap().unwrap();
    assert!(
        started.elapsed() >= Duration::from_millis(400),
        "stopped only once idle"
    );
    let _ = std::fs::remove_dir_all(root);
}
