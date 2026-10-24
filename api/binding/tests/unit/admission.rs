//! A process whose first connection the daemon does not admit, because
//! other processes hold every connection it admits, waits to be admitted
//! instead of failing with a bare `UnexpectedEof`, and is refused `Busy`
//! once the wait ends. Included by pool.rs.
use super::Dial;
use layerfs_bridge::control::{Answer, DaemonPhase, DaemonStatus, Reply, Request};
use layerfs_bridge::duplex::{accept, generate_keypair};
use layerfs_bridge::initial_record::InitialRecord;
use std::{
    net::TcpListener,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

const INSTANCE: [u8; 32] = [7; 32];

/// A daemon that ends its first `refuse` sockets unread, as the daemon ends
/// a socket while it has no vacant connection slot, then admits the next one
/// and answers its Hello; `None` admits none. Counts the sockets it ended.
fn daemon(refuse: Option<usize>) -> (Dial, Arc<AtomicUsize>, thread::JoinHandle<()>) {
    let daemon = generate_keypair().unwrap();
    let client = generate_keypair().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let dial = Dial {
        endpoint: listener.local_addr().unwrap(),
        host: None,
        secret: client.private,
        peer: daemon.public,
        instance: INSTANCE,
    };
    let ended = Arc::new(AtomicUsize::new(0));
    let count = ended.clone();
    let server = thread::spawn(move || loop {
        let (stream, _) = listener.accept().unwrap();
        if refuse.is_none_or(|refuse| count.load(Ordering::SeqCst) < refuse) {
            drop(stream);
            count.fetch_add(1, Ordering::SeqCst);
            continue;
        }
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut connection = accept(stream, &daemon.private, client.public).unwrap();
        let call = match InitialRecord::decode(connection.receive.receive().unwrap()).unwrap() {
            InitialRecord::Control(call) => call,
            InitialRecord::Install(_) => panic!("install record"),
        };
        assert!(matches!(call.request, Request::Hello(_)), "{call:?}");
        let answer = Answer {
            id: call.id,
            reply: Reply::Hello(DaemonStatus {
                instance: INSTANCE,
                phase: DaemonPhase::ControlReady,
                overlay_sqlite: Some("3.50.4".into()),
                store_sqlite: Some("3.50.4".into()),
            }),
        };
        connection.send.send(&answer.encode().unwrap()).unwrap();
        return;
    });
    (dial, ended, server)
}

#[test]
fn a_first_connection_waits_for_the_daemon_to_admit_it() {
    let (dial, ended, server) = daemon(Some(3));
    if let Err(refusal) = dial.open_waiting() {
        panic!("not admitted: {}: {}", refusal.code, refusal.message);
    }
    server.join().unwrap();
    assert_eq!(ended.load(Ordering::SeqCst), 3);
}

#[test]
fn a_process_the_daemon_never_admits_is_refused_busy() {
    let (dial, ended, _server) = daemon(None);
    let started = Instant::now();
    let refusal = dial
        .open_admitted(Duration::from_millis(300))
        .map(|_| ())
        .unwrap_err();
    assert_eq!(refusal.code, "Busy", "{}", refusal.message);
    assert!(started.elapsed() >= Duration::from_millis(300));
    assert!(started.elapsed() < Duration::from_secs(10));
    assert!(ended.load(Ordering::SeqCst) > 1, "opened only once");
}
