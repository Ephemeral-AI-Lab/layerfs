#![cfg(feature = "native")]
use layerfs_bridge::{adapters::native::protocol::*, contract::*};

fn request() -> Request {
    Request {
        id: 1,
        generation: 3,
        store: 7,
        profile: 1,
        deadline_ms: 1000,
        response_bytes: 0,
        operation: Operation::UpdatePortableMetadata {
            base: [1; 32],
            kind: 1,
            mode: 0o640,
            mtime_seconds: -2,
            mtime_nanoseconds: 750_000_000,
        },
    }
}
fn saved() -> Response {
    Response::MetadataSaved {
        base: [1; 32],
        kind: 1,
        mode: 0o640,
        mtime_seconds: -2,
        mtime_nanoseconds: 750_000_000,
        metadata: [2; 32],
        inserted: 7,
        reused: 8,
    }
}

#[test]
fn metadata_operation_has_exact_shapes_and_an_independent_store_grant() {
    let request = request();
    assert_eq!(request.operation.opcode(), 9);
    assert_eq!(permission_bit(9), Some(0x80));
    assert_eq!(permission_bit(8), None);
    assert!(request.operation.content_mutation());
    assert!(request.operation.mutation());
    assert!(!request.operation.metadata_mutation());
    assert!(!request.operation.read_only());
    assert_eq!(request.operation.input_length().unwrap(), 0);
    let bytes = encode_request(&request).unwrap();
    assert_eq!(bytes.len(), PORTABLE_METADATA_REQUEST_BYTES);
    assert_eq!(decode_request(1, &bytes).unwrap(), request);
    for end in 0..bytes.len() {
        assert!(decode_request(1, &bytes[..end]).is_err());
    }
    let mut oversized = bytes;
    oversized.push(0);
    assert_eq!(
        decode_request(1, &oversized).unwrap_err().code,
        Code::Capacity
    );
    let bytes = encode_response(&saved()).unwrap();
    assert_eq!(bytes.len(), PORTABLE_METADATA_RESULT_BYTES);
    assert_eq!(bytes[0], 11);
    assert_eq!(decode_response(&bytes).unwrap(), saved());
    for end in 0..bytes.len() {
        assert!(decode_response(&bytes[..end]).is_err());
    }
    let mut oversized = bytes;
    oversized.push(0);
    assert_eq!(
        decode_response(&oversized).unwrap_err().code,
        Code::Capacity
    );
}

#[test]
fn invalid_portable_fields_and_profiles_are_refused_before_delivery() {
    for (kind, mode, ns) in [
        (0, 0o644, 0),
        (4, 0o644, 0),
        (1, 0o4755, 0),
        (2, 0o2777, 0),
        (3, 0o755, 0),
        (1, 0o644, 1_000_000_000),
    ] {
        let mut request = request();
        request.operation = Operation::UpdatePortableMetadata {
            base: [1; 32],
            kind,
            mode,
            mtime_seconds: i64::MIN,
            mtime_nanoseconds: ns,
        };
        assert_eq!(
            encode_request(&request).unwrap_err().code,
            Code::InvalidInput
        );
        let response = Response::MetadataSaved {
            base: [1; 32],
            kind,
            mode,
            mtime_seconds: i64::MIN,
            mtime_nanoseconds: ns,
            metadata: [2; 32],
            inserted: 0,
            reused: 0,
        };
        assert_eq!(
            encode_response(&response).unwrap_err().code,
            Code::InvalidInput
        );
    }
    for (kind, mode) in [(1, 0o777), (2, 0o1777), (3, 0o777)] {
        let mut request = request();
        request.operation = Operation::UpdatePortableMetadata {
            base: [1; 32],
            kind,
            mode,
            mtime_seconds: i64::MIN,
            mtime_nanoseconds: 999_999_999,
        };
        assert!(request.validate().is_ok());
    }
    let mut wrong = request();
    wrong.response_bytes = 1;
    assert_eq!(wrong.validate().unwrap_err().code, Code::InvalidInput);
    for profile in [2, 3] {
        wrong = request();
        wrong.profile = profile;
        assert_eq!(wrong.validate().unwrap_err().code, Code::Unsupported);
    }
    let mut malformed = encode_response(&saved()).unwrap();
    malformed[33] = 0;
    assert_eq!(
        decode_response(&malformed).unwrap_err().code,
        Code::InvalidInput
    );
}

#[test]
fn native_metadata_reply_must_echo_the_exact_input_and_preserve_unknown_delivery() {
    use layerfs_bridge::adapters::native::{
        client::Client,
        connection::{accept, connect, Peer, VerifiedPeer},
        listen,
    };
    for scenario in 0..7 {
        let listener = listen("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = listener.local_addr().unwrap();
        let public = *VerifiedPeer::from_private(&[9; 32]).unwrap().public_key();
        std::thread::scope(|threads| {
            let server = threads.spawn(|| {
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
                let hello = connection.receive.read().unwrap();
                connection.send.write(&hello).unwrap();
                assert_eq!(connection.receive.read().unwrap().kind, Kind::Begin);
                assert_eq!(connection.receive.read().unwrap().kind, Kind::EndInput);
                let mut response = saved();
                if let Response::MetadataSaved {
                    base,
                    kind,
                    mode,
                    mtime_seconds,
                    mtime_nanoseconds,
                    ..
                } = &mut response
                {
                    match scenario {
                        1 => *base = [3; 32],
                        2 => *mode = 0o600,
                        3 => *mtime_seconds = -3,
                        4 => *mtime_nanoseconds = 1,
                        6 => *kind = 2,
                        _ => {}
                    }
                }
                let frame = if scenario == 5 {
                    Frame {
                        kind: Kind::ResultData,
                        id: 1,
                        bytes: vec![1],
                    }
                } else {
                    Frame {
                        kind: Kind::Success,
                        id: 1,
                        bytes: encode_response(&response).unwrap(),
                    }
                };
                connection.send.write(&frame).unwrap();
            });
            let mut client = Client::new(connect(address, 1, &[7; 32], &public).unwrap()).unwrap();
            let mut output = Vec::new();
            let result = client.call(&request(), &mut &[][..], &mut output);
            if scenario == 0 {
                assert_eq!(result.unwrap(), saved());
            } else {
                let failure = result.unwrap_err();
                assert_eq!(failure.code, Code::Unknown);
                assert!(failure.unknown);
            }
            assert!(output.is_empty());
            drop(client);
            server.join().unwrap();
        });
    }
}
