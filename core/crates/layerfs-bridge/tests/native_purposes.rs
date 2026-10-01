#![cfg(feature = "native")]
//! Independent HELLO framing and real authenticated purpose restrictions.
use layerfs_bridge::{
    adapters::native::{
        client::Client,
        connection::{accept, connect, Peer, VerifiedPeer},
        listen,
        protocol::{Frame, Kind},
        purpose::{Hello, Purpose},
        server::serve_admitted,
    },
    contract::*,
};
use std::{
    io,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

#[test]
fn literal_closed_hello_vectors_preserve_v1_and_bind_three_v2_purposes() {
    assert_eq!(Hello::legacy().encode(), [0, 1]);
    for (purpose, wire) in [
        (Purpose::General, [0, 2, 1, 0]),
        (Purpose::Catalog, [0, 2, 2, 0]),
        (Purpose::Control, [0, 2, 3, 0]),
    ] {
        let selected = Hello::decode(&wire).unwrap();
        assert_eq!(selected.version(), 2);
        assert_eq!(selected.purpose(), purpose);
        assert_eq!(Hello::selected(purpose).encode(), wire);
    }
    for wire in [
        vec![],
        vec![0],
        vec![0, 0],
        vec![0, 3],
        vec![0, 2, 0, 0],
        vec![0, 2, 4, 0],
        vec![0, 2, 1, 1],
        vec![0, 2, 1],
        vec![0, 2, 1, 0, 0],
    ] {
        assert_eq!(Hello::decode(&wire).unwrap_err().code, Code::Unsupported);
    }
    assert_eq!(Hello::decode(&[0, 1]).unwrap().purpose(), Purpose::General);
}
fn request(operation: Operation) -> Request {
    Request {
        id: 1,
        generation: 1,
        store: 1,
        profile: if matches!(
            operation,
            Operation::HistoryQuery(_) | Operation::HistoryCommand(_)
        ) {
            HISTORY_PROFILE
        } else {
            1
        },
        deadline_ms: 5000,
        response_bytes: 0,
        operation,
    }
}
struct Unread;
impl Source for Unread {
    fn read(
        &mut self,
        _: &mut [u8],
        _: std::time::Instant,
        _: &std::sync::atomic::AtomicBool,
    ) -> io::Result<usize> {
        panic!("purpose refusal must precede source")
    }
}

#[test]
fn real_peer_selection_precedes_requests_and_client_refuses_wrong_surface_before_upload() {
    for purpose in [Purpose::General, Purpose::Catalog, Purpose::Control] {
        let listener = listen("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = listener.local_addr().unwrap();
        let server_key = *VerifiedPeer::from_private(&[9; 32]).unwrap().public_key();
        let peer_key = *VerifiedPeer::from_private(&[7; 32]).unwrap().public_key();
        let effects = Arc::new(AtomicUsize::new(0));
        std::thread::scope(|threads| {
            let effect = Arc::clone(&effects);
            let worker = threads.spawn(move || {
                let (socket, _) = listener.accept().unwrap();
                let connection = accept(
                    socket,
                    &[9; 32],
                    &[Peer {
                        selector: 1,
                        public: peer_key,
                        expires_unix: u64::MAX,
                    }],
                )
                .unwrap();
                serve_admitted(
                    connection,
                    |hello, peer| {
                        assert_eq!(hello, Hello::selected(purpose));
                        assert_eq!(peer.public_key(), &peer_key);
                        Ok(())
                    },
                    |_, r, input, _, _| {
                        assert_eq!(input.read(&mut [0; 1])?, 0);
                        effect.fetch_add(1, Ordering::SeqCst);
                        if let Operation::HistoryCommand(HistoryCommand::ReserveInodes {
                            scope,
                            count,
                        }) = r.operation
                        {
                            Ok(Response::History(Box::new(HistoryResult::Reservation {
                                scope,
                                start: 1,
                                count,
                            })))
                        } else {
                            Ok(Response::FileSaveCapabilities {
                                version: SAVE_FILE_V2_VERSION,
                            })
                        }
                    },
                )
            });
            let mut client =
                Client::for_purpose(connect(address, 1, &[7; 32], &server_key).unwrap(), purpose)
                    .unwrap();
            if purpose != Purpose::General {
                let r = request(Operation::SaveFile {
                    base: None,
                    base_length: 0,
                    length: 1,
                    extents: 1,
                    replacement: 1,
                });
                assert_eq!(
                    client
                        .call(&r, &mut Unread, &mut io::sink())
                        .unwrap_err()
                        .code,
                    Code::Denied
                );
                assert_eq!(effects.load(Ordering::SeqCst), 0);
            }
            let r = request(if purpose == Purpose::Catalog {
                Operation::HistoryCommand(HistoryCommand::ReserveInodes {
                    scope: [3; 32],
                    count: 1,
                })
            } else {
                Operation::FileSaveCapabilities
            });
            client.call(&r, &mut &[][..], &mut io::sink()).unwrap();
            assert_eq!(effects.load(Ordering::SeqCst), 1);
            drop(client);
            assert!(worker.join().unwrap().is_err());
        });
    }
}
#[test]
fn malformed_and_oversized_first_hello_never_reach_class_admission() {
    for wire in [vec![0, 2, 3, 1], vec![0; 32768]] {
        let listener = listen("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = listener.local_addr().unwrap();
        let server = *VerifiedPeer::from_private(&[9; 32]).unwrap().public_key();
        let peer = *VerifiedPeer::from_private(&[7; 32]).unwrap().public_key();
        std::thread::scope(|threads| {
            let worker = threads.spawn(move || {
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
                serve_admitted(
                    connection,
                    |_, _| panic!("malformed HELLO admission"),
                    |_, _, _, _, _| panic!("malformed HELLO effect"),
                )
            });
            let mut connection = connect(address, 1, &[7; 32], &server).unwrap();
            connection
                .send
                .write(&Frame {
                    kind: if wire.len() > 4 {
                        Kind::Begin
                    } else {
                        Kind::Hello
                    },
                    id: 0,
                    bytes: wire,
                })
                .unwrap();
            assert!(connection.receive.read().is_err());
            drop(connection);
            let failure = worker.join().unwrap().unwrap_err();
            assert!(matches!(failure.code, Code::Unsupported | Code::Capacity));
        });
    }
}
