#![cfg(feature = "native")]
use layerfs_bridge::{
    adapters::native::{
        connection::{accept, connect, Peer, VerifiedPeer},
        listen,
        protocol::{decode_failure, encode_request, Frame, Kind},
        server::serve,
    },
    contract::{Code, Operation, Request},
};
use std::{sync::mpsc, time::Duration};

#[test]
fn early_failure_is_delivered_while_the_bounded_upload_closes() {
    let listener = listen("127.0.0.1:0".parse().unwrap()).unwrap();
    let address = listener.local_addr().unwrap();
    let public = *VerifiedPeer::from_private(&[9; 32]).unwrap().public_key();
    let peer = *VerifiedPeer::from_private(&[7; 32]).unwrap().public_key();
    let (sent, queued) = mpsc::channel();
    let (finished, result) = mpsc::channel();
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
            let result = serve(connection, |_, _, _, _, _| {
                // The handler rejects before reading: an upload byte is already
                // queued on the real socket, and END_INPUT has not been sent.
                queued.recv_timeout(Duration::from_secs(2)).unwrap();
                Err(Code::Denied.into())
            });
            finished.send(result.unwrap_err()).unwrap();
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
            response_bytes: 0,
            operation: Operation::SaveFile {
                base: None,
                base_length: 0,
                length: 1,
                extents: 1,
                replacement: 1,
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
                kind: Kind::Body,
                id: 1,
                bytes: vec![1],
            })
            .unwrap();
        sent.send(()).unwrap();
        let terminal = connection.receive.read().unwrap();
        assert_eq!((terminal.kind, terminal.id), (Kind::Failure, 1));
        let failure = decode_failure(&terminal.bytes).unwrap();
        assert_eq!(failure.code, Code::Denied);
        assert!(!failure.unknown);
        // Delivery has completed; final server teardown waits for the remaining
        // upload within the original deadline, rather than resetting queued data.
        assert!(matches!(
            result.recv_timeout(Duration::from_millis(50)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        connection.send.end_upload();
        let failure = result.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(failure.code, Code::Denied);
        assert!(!failure.unknown);
        server.join().unwrap();
    });
}
