//! Probing a worker never keeps it up (#533 step 19, delegated owner
//! decision 2026-11-02; closes the rest of review P1-26): a bare
//! connection, the secret alone (what `machine.status` and `machine.list`
//! send), `worker.ping`, a wrong secret and a malformed frame do not restart
//! its idle clock. A request that does work does. Its own test binary: the
//! idle clock is one per process.
use layerfs_api::worker;
use layerfs_bridge::control::SdkValue;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const IDLE: Duration = Duration::from_millis(800);

fn frame(text: &str) -> Vec<u8> {
    let mut bytes = (text.len() as u32).to_be_bytes().to_vec();
    bytes.extend_from_slice(text.as_bytes());
    bytes
}

/// Each kind of probe once; none of them carries a request.
fn probe(address: SocketAddr, endpoint: &worker::Endpoint) {
    let connect = || TcpStream::connect_timeout(&address, Duration::from_secs(1));
    // a bare connection, closed
    drop(connect());
    // the secret alone, answered `ready`, then closed
    if let Ok(mut stream) = connect() {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let _ = stream.write_all(&frame(r#"{"secret":"s"}"#));
        let mut answer = [0u8; 64];
        let _ = stream.read(&mut answer);
    }
    // a wrong secret, and a frame that is not JSON
    for hello in [frame(r#"{"secret":"no"}"#), frame("not json")] {
        if let Ok(mut stream) = connect() {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let _ = stream.write_all(&hello);
            let mut answer = [0u8; 64];
            let _ = stream.read(&mut answer);
        }
    }
    // a ping: authenticated, but a probe
    let _ = worker::ping(endpoint);
}

fn serve(root: &std::path::Path) -> (SocketAddr, std::thread::JoinHandle<std::io::Result<()>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let root = root.to_path_buf();
    let served = std::thread::spawn(move || {
        worker::serve_until_idle(
            listener,
            "s".to_owned(),
            root,
            IDLE,
            Duration::from_millis(20),
        )
    });
    (address, served)
}

#[test]
fn probes_do_not_postpone_the_idle_stop_and_a_request_does() {
    let root = std::env::temp_dir().join(format!("layerfs-idle-probes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    // probed every 100 ms throughout, the worker still stops at its idle time
    let (address, served) = serve(&root);
    let endpoint = worker::Endpoint {
        address,
        secret: "s".to_owned(),
    };
    let begun = Instant::now();
    let stop = Arc::new(AtomicBool::new(false));
    let prober = {
        let (stop, endpoint) = (stop.clone(), endpoint.clone());
        std::thread::spawn(move || {
            let mut probes = 0;
            // at most 5 s, so a worker that never stops fails the test, not hangs it
            while !stop.load(Ordering::SeqCst) && begun.elapsed() < Duration::from_secs(5) {
                probe(address, &endpoint);
                probes += 1;
                std::thread::sleep(Duration::from_millis(100));
            }
            probes
        })
    };
    served.join().unwrap().unwrap();
    let took = begun.elapsed();
    stop.store(true, Ordering::SeqCst);
    let probes = prober.join().unwrap();
    assert!(probes >= 4, "only {probes} probe rounds ran");
    assert!(
        took >= IDLE && took < IDLE * 3,
        "stopped after {took:?} despite {probes} probe rounds"
    );

    // a request that does work restarts the clock: one sent at 500 ms holds
    // the worker until 500 ms + IDLE at least (any answer counts, here a
    // refusal of its arguments)
    let (address, served) = serve(&root);
    let endpoint = worker::Endpoint {
        address,
        secret: "s".to_owned(),
    };
    let begun = Instant::now();
    std::thread::sleep(Duration::from_millis(500));
    let answer = worker::call(&endpoint, "store.call", SdkValue::Null, &mut |_| {});
    assert_eq!(answer.unwrap_err().code, "InvalidInput");
    served.join().unwrap().unwrap();
    let took = begun.elapsed();
    assert!(
        took >= Duration::from_millis(500) + IDLE,
        "a request did not postpone the stop: {took:?}"
    );
    let _ = std::fs::remove_dir_all(root);
}
