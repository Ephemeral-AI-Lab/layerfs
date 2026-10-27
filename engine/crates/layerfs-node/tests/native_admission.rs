//! First-connection admission through the built Node binding's exported call.
#![cfg(target_os = "linux")]
mod support;
use layerfs_bridge::{
    control::{Answer, DaemonPhase, DaemonStatus, Reply, Request, SdkValue},
    duplex::{accept, generate_keypair},
    initial_record::InitialRecord,
};
use std::{
    net::TcpListener,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
use support::{engine::*, *};

fn run(refuse: Option<usize>) {
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
    let engine = engines(&home.0, vec![(200, running.clone()), (200, running)]);
    let ended = Arc::new(AtomicUsize::new(0));
    let count = ended.clone();
    let stopped = Arc::new(AtomicBool::new(false));
    let stop = stopped.clone();
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(80);
        while !stop.load(Ordering::SeqCst) && Instant::now() < deadline {
            let stream = match listener.accept() {
                Ok((stream, _)) => stream,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(2));
                    continue;
                }
                Err(e) => panic!("daemon admission: {e}"),
            };
            if refuse.is_none_or(|refuse| count.load(Ordering::SeqCst) < refuse) {
                drop(stream);
                count.fetch_add(1, Ordering::SeqCst);
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
            connection
                .send
                .send(
                    &Answer {
                        id: call.id,
                        reply: Reply::Sdk(SdkValue::map([
                            ("kind", SdkValue::text("ok")),
                            ("workspaces", SdkValue::List(vec![])),
                        ])),
                    }
                    .encode()
                    .unwrap(),
                )
                .unwrap();
            return;
        }
    });
    let result = std::panic::catch_unwind(|| {
        home.node(if refuse.is_some() {
        "assert.deepEqual(await native.call('sandbox.list_workspaces',args), {value:{kind:'ok',workspaces:[]}});"
    } else {
        "const start=Date.now(); const answer=await native.call('sandbox.list_workspaces',args); assert.equal(answer.refused.code,'Busy'); assert.ok(Date.now()-start>=60000);"
    }, None, Duration::from_secs(75))
    });
    stopped.store(true, Ordering::SeqCst);
    server.join().unwrap();
    assert_eq!(engine.join().unwrap().len(), 2);
    if let Err(error) = result {
        std::panic::resume_unwind(error);
    }
    if let Some(refuse) = refuse {
        assert_eq!(ended.load(Ordering::SeqCst), refuse);
    } else {
        assert!(ended.load(Ordering::SeqCst) > 1, "opened only once");
    }
}

#[test]
fn a_first_connection_waits_for_the_daemon_to_admit_it() {
    run(Some(3));
}

#[test]
fn a_process_the_daemon_never_admits_is_refused_busy() {
    run(None);
}
