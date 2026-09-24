#![cfg(feature = "native")]
//! Logical Source/Write fragments do not define authenticated wire-frame counts.
use layerfs_bridge::{
    adapters::native::{
        client::Client,
        connection::{accept, connect, Peer, VerifiedPeer},
        listen,
        server::serve,
    },
    contract::*,
};
use std::{
    io,
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::Instant,
};

struct Tiny<'a>(&'a [u8]);
impl Source for Tiny<'_> {
    fn read(
        &mut self,
        out: &mut [u8],
        deadline: Instant,
        cancel: &AtomicBool,
    ) -> io::Result<usize> {
        let length = out.len().min(1);
        Source::read(&mut self.0, &mut out[..length], deadline, cancel)
    }
}
fn exercise(upload: bool) {
    let listener = listen("127.0.0.1:0".parse().unwrap()).unwrap();
    let address = listener.local_addr().unwrap();
    let public = *VerifiedPeer::from_private(&[9; 32]).unwrap().public_key();
    let peer = *VerifiedPeer::from_private(&[7; 32]).unwrap().public_key();
    let expected: Vec<u8> = (0..FRAME_BYTES + 517).map(|i| (i % 251) as u8).collect();
    let input = Arc::new(Mutex::new(Vec::new()));
    let request = Request {
        id: 1,
        generation: 1,
        store: 1,
        profile: 1,
        deadline_ms: 5000,
        response_bytes: if upload { 0 } else { expected.len() as u64 },
        operation: if upload {
            Operation::ConstructFile {
                length: expected.len() as u64,
            }
        } else {
            Operation::ReadFile {
                root: [1; 32],
                start: 0,
                end: expected.len() as u64,
            }
        },
    };
    std::thread::scope(|threads| {
        let recorded = input.clone();
        let bytes = &expected;
        let server = threads.spawn(move || {
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
            let _closed = serve(connection, |_, _, source, out, _| {
                source.read_to_end(&mut recorded.lock().unwrap())?;
                if upload {
                    Ok(Response::Saved {
                        root: [2; 32],
                        length: bytes.len() as u64,
                        inserted: 1,
                        reused: 0,
                        metadata: None,
                    })
                } else {
                    for byte in bytes {
                        out.write_all(&[*byte])?;
                    }
                    Ok(Response::Read {
                        length: bytes.len() as u64,
                    })
                }
            });
        });
        let mut client = Client::new(connect(address, 1, &[7; 32], &public).unwrap()).unwrap();
        let mut source = Tiny(if upload { &expected[..] } else { &[][..] });
        let mut result = Vec::new();
        let response = client.call(&request, &mut source, &mut result);
        drop(client);
        server.join().unwrap();
        assert!(
            response.is_ok(),
            "fragmented logical stream failed: {response:?}"
        );
        if upload {
            assert_eq!(*input.lock().unwrap(), expected);
            assert!(result.is_empty());
        } else {
            assert_eq!(result, expected);
            assert!(input.lock().unwrap().is_empty());
        }
    });
}
#[test]
fn short_source_reads_form_bounded_body_frames() {
    exercise(true);
}
#[test]
fn short_logical_writes_form_bounded_result_frames() {
    exercise(false);
}

#[test]
fn handler_failure_discards_pending_tail_without_drop_flush() {
    use layerfs_bridge::adapters::native::protocol::{decode_failure, encode_request, Frame, Kind};
    for flush in [false, true] {
        let listener = listen("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = listener.local_addr().unwrap();
        let public = *VerifiedPeer::from_private(&[9; 32]).unwrap().public_key();
        let peer = *VerifiedPeer::from_private(&[7; 32]).unwrap().public_key();
        std::thread::scope(|threads| {
            let server = threads.spawn(move || {
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
                let failure = serve(connection, |_, _, input, output, _| {
                    std::io::copy(input, &mut std::io::sink())?;
                    output.write_all(b"tail")?;
                    if flush {
                        output.flush()?;
                    }
                    Err(Code::Denied.into())
                })
                .unwrap_err();
                assert_eq!(failure.code, Code::Denied);
            });
            let mut connection = connect(address, 1, &[7; 32], &public).unwrap();
            connection
                .send
                .write(&Frame {
                    kind: Kind::Hello,
                    id: 0,
                    bytes: 1u16.to_be_bytes().to_vec(),
                })
                .unwrap();
            assert_eq!(connection.receive.read().unwrap().kind, Kind::Hello);
            let request = Request {
                id: 1,
                generation: 1,
                store: 1,
                profile: 1,
                deadline_ms: 5000,
                response_bytes: 4,
                operation: Operation::ReadFile {
                    root: [1; 32],
                    start: 0,
                    end: 4,
                },
            };
            connection
                .send
                .write(&Frame {
                    kind: Kind::Begin,
                    id: 1,
                    bytes: encode_request(&request).unwrap(),
                })
                .unwrap();
            connection
                .send
                .write(&Frame {
                    kind: Kind::EndInput,
                    id: 1,
                    bytes: 0u64.to_be_bytes().to_vec(),
                })
                .unwrap();
            if flush {
                let data = connection.receive.read().unwrap();
                assert_eq!(data.kind, Kind::ResultData);
                assert_eq!(data.bytes, b"tail");
            }
            let terminal = connection.receive.read().unwrap();
            assert_eq!(terminal.kind, Kind::Failure);
            assert_eq!(decode_failure(&terminal.bytes).unwrap().code, Code::Denied);
            assert!(
                connection.receive.read().is_err(),
                "pending bytes emitted after Failure"
            );
            server.join().unwrap();
        });
    }
}

#[test]
fn buffered_output_checks_deadline_before_accepting_another_fragment() {
    use layerfs_bridge::adapters::native::protocol::{encode_request, Frame, Kind};
    use std::{sync::mpsc, time::Duration};
    let listener = listen("127.0.0.1:0".parse().unwrap()).unwrap();
    let address = listener.local_addr().unwrap();
    let public = *VerifiedPeer::from_private(&[9; 32]).unwrap().public_key();
    let peer = *VerifiedPeer::from_private(&[7; 32]).unwrap().public_key();
    let (observed, result) = mpsc::channel();
    std::thread::scope(|threads| {
        let server = threads.spawn(move || {
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
            let _closed = serve(connection, |_, _, input, output, deadline| {
                std::io::copy(input, &mut std::io::sink())?;
                output.write_all(b"a")?;
                while Instant::now() <= deadline {
                    std::thread::sleep(Duration::from_millis(1));
                }
                let kind = output.write_all(b"b").unwrap_err().kind();
                assert!(output.flush().is_err());
                observed.send(kind).unwrap();
                Err(Code::Deadline.into())
            });
        });
        let mut connection = connect(address, 1, &[7; 32], &public).unwrap();
        connection
            .send
            .write(&Frame {
                kind: Kind::Hello,
                id: 0,
                bytes: 1u16.to_be_bytes().to_vec(),
            })
            .unwrap();
        assert_eq!(connection.receive.read().unwrap().kind, Kind::Hello);
        let request = Request {
            id: 1,
            generation: 1,
            store: 1,
            profile: 1,
            deadline_ms: 200,
            response_bytes: 2,
            operation: Operation::ReadFile {
                root: [1; 32],
                start: 0,
                end: 2,
            },
        };
        connection
            .send
            .write(&Frame {
                kind: Kind::Begin,
                id: 1,
                bytes: encode_request(&request).unwrap(),
            })
            .unwrap();
        connection
            .send
            .write(&Frame {
                kind: Kind::EndInput,
                id: 1,
                bytes: 0u64.to_be_bytes().to_vec(),
            })
            .unwrap();
        assert_eq!(
            result.recv_timeout(Duration::from_secs(2)).unwrap(),
            io::ErrorKind::TimedOut
        );
        drop(connection);
        server.join().unwrap();
    });
}
