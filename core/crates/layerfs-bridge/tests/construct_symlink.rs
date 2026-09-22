#![cfg(feature = "native")]
//! One body-free content constructor with exact native reply correlation.
use layerfs_bridge::{adapters::native::protocol::*, contract::*};

fn request(target: &[u8]) -> Request {
    Request {
        id: 1,
        generation: 3,
        store: 7,
        profile: 1,
        deadline_ms: 1000,
        response_bytes: 0,
        operation: Operation::ConstructSymlink {
            target: target.to_vec(),
        },
    }
}
fn saved(length: u64) -> Response {
    Response::Saved {
        root: [2; 32],
        length,
        inserted: 7,
        reused: 8,
    }
}

#[test]
fn constructor_has_exact_bounds_opaque_target_and_shared_content_grant() {
    assert_eq!(CONSTRUCT_SYMLINK_OPCODE, 16);
    assert_eq!(SYMLINK_TARGET_BYTES, 4096);
    assert_eq!(CONSTRUCT_SYMLINK_REQUEST_BYTES, 4125);
    let targets = [
        b"".to_vec(),
        b"../relative/target".to_vec(),
        b"/absolute/target".to_vec(),
        vec![0xff, b'/', 0x80],
        vec![b'x'; 4096],
    ];
    for target in targets {
        let r = request(&target);
        assert_eq!(r.operation.opcode(), 16);
        assert_eq!(permission_bit(16), Some(4));
        assert_eq!(permission_bit(3), Some(4));
        assert!(r.operation.content_mutation() && r.operation.mutation());
        assert!(!r.operation.metadata_mutation() && !r.operation.read_only());
        assert_eq!(r.operation.input_length().unwrap(), 0);
        let bytes = encode_request(&r).unwrap();
        assert_eq!(bytes.len(), 29 + target.len());
        assert!(bytes.len() <= 4125);
        assert_eq!(bytes[26], 16);
        assert_eq!(&bytes[27..29], &(target.len() as u16).to_be_bytes());
        assert_eq!(&bytes[29..], target);
        assert_eq!(decode_request(1, &bytes).unwrap(), r);
        let response = saved(target.len() as u64);
        let bytes = encode_response(&response).unwrap();
        assert_eq!(bytes.len(), 57);
        assert_eq!(bytes[0], 2);
        assert_eq!(&bytes[1..33], &[2; 32]);
        assert_eq!(&bytes[33..41], &(target.len() as u64).to_be_bytes());
        assert_eq!(&bytes[41..49], &7u64.to_be_bytes());
        assert_eq!(&bytes[49..57], &8u64.to_be_bytes());
        assert_eq!(decode_response(&bytes).unwrap(), response);
    }
}

#[test]
fn constructor_rejects_malformed_metadata_and_nonzero_result_body_budget() {
    let good = encode_request(&request(b"../target")).unwrap();
    for length in 0..good.len() {
        assert!(decode_request(1, &good[..length]).is_err());
    }
    let mut extra = good.clone();
    extra.push(1);
    assert_eq!(
        decode_request(1, &extra).unwrap_err().code,
        Code::InvalidInput
    );
    let mut nul = good;
    nul[29] = 0;
    assert_eq!(
        decode_request(1, &nul).unwrap_err().code,
        Code::InvalidInput
    );
    assert_eq!(
        request(b"a\0b").validate().unwrap_err().code,
        Code::InvalidInput
    );
    assert_eq!(
        request(&vec![b'x'; 4097]).validate().unwrap_err().code,
        Code::Capacity
    );
    let mut oversize = encode_request(&request(b"")).unwrap();
    oversize[27..29].copy_from_slice(&4097u16.to_be_bytes());
    oversize.extend([b'x'; 4097]);
    assert_eq!(
        decode_request(1, &oversize).unwrap_err().code,
        Code::Capacity
    );
    let mut wrong = request(b"target");
    wrong.response_bytes = 1;
    assert_eq!(wrong.validate().unwrap_err().code, Code::InvalidInput);
    for profile in [2, 3] {
        wrong = request(b"target");
        wrong.profile = profile;
        assert_eq!(wrong.validate().unwrap_err().code, Code::Unsupported);
    }
    for budget in [0, 600_001] {
        wrong = request(b"target");
        wrong.deadline_ms = budget;
        assert!(wrong.validate().is_err());
    }
    wrong = request(b"target");
    wrong.deadline_ms = 600_000;
    assert!(wrong.validate().is_ok());
    let bytes = encode_response(&saved(6)).unwrap();
    for length in 0..bytes.len() {
        assert!(decode_response(&bytes[..length]).is_err());
    }
    let mut extra = bytes;
    extra.push(1);
    assert_eq!(
        decode_response(&extra).unwrap_err().code,
        Code::InvalidInput
    );
}

#[test]
fn native_constructor_correlates_saved_length_and_keeps_terminal_loss_unknown_without_replay() {
    use layerfs_bridge::adapters::native::{
        client::Client,
        connection::{accept, connect, Peer, VerifiedPeer},
        listen,
    };
    // Success, wrong length, wrong result type, ResultData, wrong request id,
    // trailing/truncated result bytes, and deliberate lost mutation terminal.
    for scenario in 0..8 {
        let listener = listen("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = listener.local_addr().unwrap();
        let public = *VerifiedPeer::from_private(&[9; 32]).unwrap().public_key();
        std::thread::scope(|threads| {
            let server = threads.spawn(|| {
                let (socket, _) = listener.accept().unwrap();
                let mut c = accept(
                    socket,
                    &[9; 32],
                    &[Peer {
                        selector: 1,
                        public: *VerifiedPeer::from_private(&[7; 32]).unwrap().public_key(),
                        expires_unix: u64::MAX,
                    }],
                )
                .unwrap();
                let hello = c.receive.read().unwrap();
                c.send.write(&hello).unwrap();
                let begin = c.receive.read().unwrap();
                assert_eq!(begin.kind, Kind::Begin);
                let decoded = decode_request(begin.id, &begin.bytes).unwrap();
                assert_eq!(decoded.operation, request(b"target").operation);
                assert_eq!(
                    (
                        decoded.id,
                        decoded.generation,
                        decoded.store,
                        decoded.profile,
                        decoded.response_bytes
                    ),
                    (1, 3, 7, 1, 0)
                );
                assert!((1..=1000).contains(&decoded.deadline_ms));
                let end = c.receive.read().unwrap();
                assert_eq!(end.kind, Kind::EndInput);
                assert_eq!(end.bytes, 0u64.to_be_bytes());
                if scenario == 7 {
                    return;
                }
                let mut body = encode_response(&saved(if scenario == 1 { 7 } else { 6 })).unwrap();
                if scenario == 2 {
                    body = encode_response(&Response::Read { length: 6 }).unwrap();
                }
                if scenario == 5 {
                    body.push(0);
                }
                if scenario == 6 {
                    body.pop();
                }
                c.send
                    .write(&Frame {
                        kind: if scenario == 3 {
                            Kind::ResultData
                        } else {
                            Kind::Success
                        },
                        id: if scenario == 4 { 2 } else { 1 },
                        bytes: body,
                    })
                    .unwrap();
                if scenario != 0 {
                    assert!(
                        c.receive.read().is_err(),
                        "client replayed after malformed terminal"
                    );
                }
            });
            let mut client = Client::new(connect(address, 1, &[7; 32], &public).unwrap()).unwrap();
            let mut output = Vec::new();
            let result = client.call(&request(b"target"), &mut &[][..], &mut output);
            if scenario == 0 {
                assert_eq!(result.unwrap(), saved(6));
            } else {
                let error = result.unwrap_err();
                assert_eq!(error.code, Code::Unknown);
                assert!(error.unknown);
            }
            assert!(output.is_empty());
            drop(client);
            server.join().unwrap();
            listener.set_nonblocking(true).unwrap();
            assert_eq!(
                listener.accept().unwrap_err().kind(),
                std::io::ErrorKind::WouldBlock
            );
        });
    }
}
