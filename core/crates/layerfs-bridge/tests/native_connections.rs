#![cfg(feature = "native")]
//! Real authenticated socket lifecycle, independent of OS buffer-size policy.
use layerfs_bridge::adapters::native::{
    connection::{
        accept, authenticate_until, connect, connect_tcp_until, connect_until, Peer, VerifiedPeer,
    },
    listen,
    protocol::{Frame, Kind},
};

#[test]
fn exec_progress_is_authenticated_and_has_no_result_bytes() {
    use layerfs_bridge::{
        adapters::native::{client::Client, server::serve},
        contract::{Operation, Request, Response, WorkspaceExecWire, WORKSPACE_STATUS_PROFILE},
    };
    let listener = listen("127.0.0.1:0".parse().unwrap()).unwrap();
    let address = listener.local_addr().unwrap();
    let public = *VerifiedPeer::from_private(&[9; 32]).unwrap().public_key();
    let peer = *VerifiedPeer::from_private(&[7; 32]).unwrap().public_key();
    std::thread::scope(|threads| {
        let server = threads.spawn(|| {
            let (socket, _) = listener.accept().unwrap();
            let connection = accept(
                socket,
                &[9; 32],
                &[Peer {
                    selector: 1,
                    public: peer,
                    expires_unix: u64::MAX,
                }],
            )
            .unwrap();
            let _ = serve(connection, |_, _, input, output, _| {
                assert_eq!(input.read(&mut [0; 1])?, 0);
                output.progress()?;
                output.progress()?;
                Ok(Response::WorkspaceExec(Box::new(WorkspaceExecWire {
                    workspace: b"work".to_vec(),
                    incarnation: [4; 32],
                    exit_status: Some(0),
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                    stdout_truncated: false,
                    stderr_truncated: false,
                })))
            });
        });
        let mut client = Client::new(connect(address, 1, &[7; 32], &public).unwrap()).unwrap();
        let request = Request {
            id: 1,
            generation: 0,
            store: 0,
            profile: WORKSPACE_STATUS_PROFILE,
            deadline_ms: 5_000,
            response_bytes: 0,
            operation: Operation::WorkspaceExec {
                workspace: b"work".to_vec(),
                incarnation: [4; 32],
                command: b"true".to_vec(),
            },
        };
        let mut bytes = Vec::new();
        let response = client.call(&request, &mut &[][..], &mut bytes).unwrap();
        assert!(
            matches!(response, Response::WorkspaceExec(result) if result.exit_status == Some(0))
        );
        assert!(bytes.is_empty());
        drop(client);
        server.join().unwrap();
    });
}

#[test]
fn caller_deadline_covers_connect_and_hello() {
    use layerfs_bridge::{adapters::native::client::Client, contract::Code};
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };
    let listener = listen("127.0.0.1:0".parse().unwrap()).unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let public = *VerifiedPeer::from_private(&[9; 32]).unwrap().public_key();
    let failure = connect_until(address, 1, &[7; 32], &public, Instant::now())
        .err()
        .unwrap();
    assert_eq!(failure.code, Code::Deadline);
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    listener.set_nonblocking(false).unwrap();
    let (done, released) = mpsc::channel();
    std::thread::scope(|scope| {
        scope.spawn(move || {
            let (socket, _) = listener.accept().unwrap();
            let mut connection = accept(
                socket,
                &[9; 32],
                &[Peer {
                    selector: 1,
                    public: *VerifiedPeer::from_private(&[7; 32]).unwrap().public_key(),
                    expires_unix: u64::MAX,
                }],
            )
            .unwrap();
            assert_eq!(connection.receive.read().unwrap().kind, Kind::Hello);
            // No sleep: the peer withholds HELLO until the caller's deadline fires.
            released.recv_timeout(Duration::from_secs(2)).unwrap();
        });
        let deadline = Instant::now() + Duration::from_millis(200);
        let connection = connect_until(address, 1, &[7; 32], &public, deadline).unwrap();
        assert!(Client::new(connection).is_err());
        assert!(Instant::now() >= deadline);
        done.send(()).unwrap();
    });
}
#[test]
fn repeated_native_connections_authenticate_with_ordinary_tcp() {
    use nix::poll::{poll, PollFd, PollFlags};
    use std::os::fd::AsFd;
    let listener = listen("127.0.0.1:0".parse().unwrap()).unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let private = [7; 32];
    let server = [9; 32];
    let public = *VerifiedPeer::from_private(&server).unwrap().public_key();
    let peers = [Peer {
        selector: 1,
        public: *VerifiedPeer::from_private(&private).unwrap().public_key(),
        expires_unix: u64::MAX,
    }];
    std::thread::scope(|scope| {
        scope.spawn(|| {
            for _ in 0..64 {
                let mut ready = [PollFd::new(listener.as_fd(), PollFlags::POLLIN)];
                assert!(poll(&mut ready, 1000u16).unwrap() > 0, "client stopped");
                let (stream, _) = listener.accept().unwrap();
                let mut connection = accept(stream, &server, &peers).unwrap();
                let hello = connection.receive.read().unwrap();
                assert_eq!(hello.kind, Kind::Hello);
                connection.send.write(&hello).unwrap();
            }
        });
        for index in 0..64 {
            let opened = if index == 0 {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                connect_tcp_until(address, deadline)
                    .and_then(|stream| authenticate_until(stream, 1, &private, &public, deadline))
            } else {
                connect(address, 1, &private, &public)
            };
            let mut connection = opened.unwrap_or_else(|error| panic!("connect {index}: {error}"));
            connection
                .send
                .write(&Frame {
                    kind: Kind::Hello,
                    id: 0,
                    bytes: vec![0, 1],
                })
                .unwrap();
            assert_eq!(connection.receive.read().unwrap().bytes, [0, 1]);
        }
    });
    drop(listener);
    // No fallback or retry after a refused native connection.
    assert!(connect(address, 1, &private, &public).is_err());
}

#[test]
fn definite_missing_inspect_keeps_one_authenticated_session() {
    use layerfs_bridge::{
        adapters::native::{client::Client, server::serve},
        contract::{Code, Inspect, Operation, Request},
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    let listener = listen("127.0.0.1:0".parse().unwrap()).unwrap();
    let address = listener.local_addr().unwrap();
    let private = [7; 32];
    let server = [9; 32];
    let public = *VerifiedPeer::from_private(&server).unwrap().public_key();
    let peers = [Peer {
        selector: 1,
        public: *VerifiedPeer::from_private(&private).unwrap().public_key(),
        expires_unix: u64::MAX,
    }];
    let calls = AtomicUsize::new(0);
    std::thread::scope(|threads| {
        let worker = threads.spawn(|| {
            let (socket, _) = listener.accept().unwrap();
            let connection = accept(socket, &server, &peers).unwrap();
            let _ = serve(connection, |_, _, _, _, _| {
                let code = if calls.fetch_add(1, Ordering::Relaxed) == 0 {
                    Code::PathNotFound
                } else {
                    Code::NotFound
                };
                Err(code.into())
            });
        });
        let mut client = Client::new(connect(address, 1, &private, &public).unwrap()).unwrap();
        for (id, expected) in [(1, Code::PathNotFound), (2, Code::NotFound)] {
            let request = Request {
                id,
                generation: 1,
                store: 1,
                profile: 1,
                deadline_ms: 1000,
                response_bytes: 0,
                operation: Operation::Inspect {
                    root: [1; 32],
                    query: Inspect::File,
                },
            };
            assert_eq!(
                client
                    .call(&request, &mut &[][..], &mut Vec::new())
                    .unwrap_err()
                    .code,
                expected
            );
        }
        drop(client);
        worker.join().unwrap();
    });
    assert_eq!(calls.load(Ordering::Relaxed), 2);
}

#[test]
fn productive_upload_keeps_the_response_wait_alive() {
    use std::time::{Duration, Instant};
    let listener = listen("127.0.0.1:0".parse().unwrap()).unwrap();
    let address = listener.local_addr().unwrap();
    let client_key = [11; 32];
    let server_key = [13; 32];
    let public = *VerifiedPeer::from_private(&server_key)
        .unwrap()
        .public_key();
    let peers = [Peer {
        selector: 1,
        public: *VerifiedPeer::from_private(&client_key)
            .unwrap()
            .public_key(),
        expires_unix: u64::MAX,
    }];
    std::thread::scope(|scope| {
        let server = scope.spawn(|| {
            let (socket, _) = listener.accept().unwrap();
            let mut connection = accept(socket, &server_key, &peers).unwrap();
            let deadline = Instant::now() + Duration::from_secs(12);
            connection.receive.deadline(deadline);
            connection.send.deadline(deadline);
            for _ in 0..7 {
                let frame = connection.receive.read().unwrap();
                assert_eq!(frame.bytes, [42]);
            }
            connection
                .send
                .write(&Frame {
                    kind: Kind::Success,
                    id: 1,
                    bytes: vec![42],
                })
                .unwrap();
        });
        let mut connection = connect(address, 1, &client_key, &public).unwrap();
        let deadline = Instant::now() + Duration::from_secs(12);
        connection.receive.deadline(deadline);
        connection.send.deadline(deadline);
        let upload = scope.spawn(move || {
            for index in 0..7 {
                if index != 0 {
                    std::thread::sleep(Duration::from_secs(1));
                }
                connection
                    .send
                    .write(&Frame {
                        kind: Kind::Body,
                        id: 1,
                        bytes: vec![42],
                    })
                    .unwrap();
            }
        });
        let response = connection.receive.read();
        upload.join().unwrap();
        server.join().unwrap();
        assert_eq!(response.unwrap().bytes, [42]);
    });
}
