//! A call waits a bounded time for one of its process's Sandbox connections
//! (#508), through the built Node binding's exported call. The mock daemon
//! admits one connection and holds its call without a reply; it admits no
//! other. A second call is refused `Busy` once the product's 60-second wait
//! ends, instead of waiting for ever, and nothing is sent for it.
#![cfg(target_os = "linux")]
mod support;
use layerfs_bridge::{
    control::{Answer, DaemonPhase, DaemonStatus, Reply, Request},
    duplex::{accept, generate_keypair},
    initial_record::InitialRecord,
};
use std::{
    net::TcpListener,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
use support::{engine::*, *};

/// The held call ends this long after it is held: after the second call's
/// 60-second wait has ended, and also when a failed assertion ends Node early
/// (Node's exit waits for the binding thread that holds the call).
const RELEASE: Duration = Duration::from_secs(66);

#[test]
fn a_call_behind_a_held_connection_is_refused_busy() {
    let home = Home::new();
    let daemon = generate_keypair().unwrap();
    let client = generate_keypair().unwrap();
    home.record(CID, client.private, daemon.public);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    let running = exited()
        .replace("\"Running\":false", "\"Running\":true")
        .replace(
            "\"Ports\":{}",
            &format!(r#""Ports":{{"30421/tcp":[{{"HostIp":"127.0.0.1","HostPort":"{port}"}}]}}"#),
        );
    // Discovery for the first call inspects the container twice; the second
    // call checks it once.
    let engine = engines(&home.0, vec![(200, running); 3]);
    let stopped = Arc::new(AtomicBool::new(false));
    let stop = stopped.clone();
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(90);
        let (mut held, mut refused, mut since) = (None, 0usize, None::<Instant>);
        while !stop.load(Ordering::SeqCst) && Instant::now() < deadline {
            if since.is_some_and(|at| at.elapsed() >= RELEASE) {
                break;
            }
            let stream = match listener.accept() {
                Ok((stream, _)) => stream,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(2));
                    continue;
                }
                Err(e) => panic!("daemon admission: {e}"),
            };
            if held.is_some() {
                // Not admitted: closed before the Hello reply.
                drop(stream);
                refused += 1;
                continue;
            }
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut connection = accept(stream, &daemon.private, client.public).unwrap();
            let hello = match InitialRecord::decode(connection.receive.receive().unwrap()).unwrap()
            {
                InitialRecord::Control(call) => call,
                InitialRecord::Install(_) => panic!("install record"),
            };
            assert!(matches!(hello.request, Request::Hello(_)));
            connection
                .send
                .send(
                    &Answer {
                        id: hello.id,
                        reply: Reply::Hello(DaemonStatus {
                            instance: [7; 32],
                            phase: DaemonPhase::ControlReady,
                            overlay_sqlite: Some("3.50.4".into()),
                            store_sqlite: Some("3.50.4".into()),
                        }),
                    }
                    .encode()
                    .unwrap(),
                )
                .unwrap();
            let call = match InitialRecord::decode(connection.receive.receive().unwrap()).unwrap() {
                InitialRecord::Control(call) => call,
                InitialRecord::Install(_) => panic!("install record"),
            };
            assert!(matches!(call.request, Request::Sdk(_)));
            // Held without a reply until the release; dropping it ends the call.
            held = Some(connection);
            since = Some(Instant::now());
        }
        drop(held);
        refused
    });
    let result = std::panic::catch_unwind(|| {
        home.node(
            "const first = native.call('sandbox.list_workspaces', args);
             await new Promise((resolve) => setTimeout(resolve, 200));
             const start = Date.now();
             const second = await native.call('sandbox.list_workspaces', args);
             const waited = Date.now() - start;
             console.log('SECOND', waited, JSON.stringify(second));
             try {
               assert.equal(second.refused?.code, 'Busy', JSON.stringify(second));
               assert.match(second.refused.message, /nothing was attempted/);
               assert.ok(waited >= 60000, String(waited));
             } catch (error) {
               // The held first call would keep Node alive: end now.
               console.error(error);
               process.exit(1);
             }
             const held = await first;
             assert.ok(held.refused, JSON.stringify(held));",
            None,
            Duration::from_secs(85),
        )
    });
    stopped.store(true, Ordering::SeqCst);
    let refused = server.join().unwrap();
    assert_eq!(engine.join().unwrap().len(), 3);
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
    // The second call opened one connection, refused before Hello; then it
    // waited for the held one rather than dialing again.
    assert_eq!(refused, 1);
}
