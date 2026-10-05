#![cfg(feature = "native")]
//! Standalone portable metadata construction uses one exact, body-free mutation.
use layerfs_bridge::{adapters::native::protocol::*, contract::*};

fn request() -> Request {
    Request {
        id: 1,
        generation: 3,
        store: 7,
        profile: 1,
        deadline_ms: 1000,
        response_bytes: 0,
        operation: Operation::ConstructPortableMetadata {
            kind: 1,
            mode: 0o640,
            mtime_seconds: -2,
            mtime_nanoseconds: 750_000_000,
        },
    }
}
fn constructed() -> Response {
    Response::MetadataConstructed {
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
fn constructor_has_exact_wire_shapes_body_limits_and_shared_metadata_grant() {
    let request = request();
    assert_eq!(CONSTRUCT_PORTABLE_METADATA_OPCODE, 15);
    assert_eq!(request.operation.opcode(), 15);
    assert_eq!(permission_bit(15), Some(128));
    assert_eq!(permission_bit(9), Some(128));
    assert!(request.operation.content_mutation() && request.operation.mutation());
    assert!(!request.operation.metadata_mutation() && !request.operation.read_only());
    assert_eq!(request.operation.input_length().unwrap(), 0);
    let bytes = encode_request(&request).unwrap();
    assert_eq!(CONSTRUCT_PORTABLE_METADATA_REQUEST_BYTES, 44);
    assert_eq!(bytes.len(), 44);
    assert_eq!(bytes[26], 15);
    let payload = [
        &[1][..],
        &0o640u32.to_be_bytes(),
        &(-2i64).to_be_bytes(),
        &750_000_000u32.to_be_bytes(),
    ]
    .concat();
    assert_eq!(&bytes[27..], payload);
    assert_eq!(decode_request(1, &bytes).unwrap(), request);
    for length in 0..bytes.len() {
        assert!(decode_request(1, &bytes[..length]).is_err());
    }
    let mut extra = bytes;
    extra.push(0);
    assert_eq!(decode_request(1, &extra).unwrap_err().code, Code::Capacity);
    let bytes = encode_response(&constructed()).unwrap();
    assert_eq!(CONSTRUCT_PORTABLE_METADATA_RESULT_BYTES, 66);
    assert_eq!(bytes.len(), 66);
    assert_eq!(bytes[0], 19);
    assert_eq!(&bytes[1..18], payload);
    assert_eq!(&bytes[18..50], &[2; 32]);
    assert_eq!(&bytes[50..58], &7u64.to_be_bytes());
    assert_eq!(&bytes[58..66], &8u64.to_be_bytes());
    assert_eq!(decode_response(&bytes).unwrap(), constructed());
    for length in 0..bytes.len() {
        assert!(decode_response(&bytes[..length]).is_err());
    }
    let mut extra = bytes;
    extra.push(0);
    assert_eq!(decode_response(&extra).unwrap_err().code, Code::Capacity);
}

#[test]
fn constructor_refuses_invalid_portable_fields_profiles_and_response_bodies() {
    for (kind, mode, ns) in [
        (0, 0o644, 0),
        (4, 0o644, 0),
        (1, 0o4755, 0),
        (2, 0o2777, 0),
        (3, 0o755, 0),
        (1, 0o644, 1_000_000_000),
    ] {
        let mut request = request();
        request.operation = Operation::ConstructPortableMetadata {
            kind,
            mode,
            mtime_seconds: i64::MIN,
            mtime_nanoseconds: ns,
        };
        assert_eq!(
            encode_request(&request).unwrap_err().code,
            Code::InvalidInput
        );
        let result = Response::MetadataConstructed {
            kind,
            mode,
            mtime_seconds: i64::MIN,
            mtime_nanoseconds: ns,
            metadata: [0; 32],
            inserted: 0,
            reused: u64::MAX,
        };
        assert_eq!(
            encode_response(&result).unwrap_err().code,
            Code::InvalidInput
        );
        let mut raw = encode_request(&self::request()).unwrap();
        raw[27] = kind;
        raw[28..32].copy_from_slice(&mode.to_be_bytes());
        raw[40..44].copy_from_slice(&ns.to_be_bytes());
        assert_eq!(
            decode_request(1, &raw).unwrap_err().code,
            Code::InvalidInput
        );
        let mut raw = encode_response(&constructed()).unwrap();
        raw[1] = kind;
        raw[2..6].copy_from_slice(&mode.to_be_bytes());
        raw[14..18].copy_from_slice(&ns.to_be_bytes());
        assert_eq!(decode_response(&raw).unwrap_err().code, Code::InvalidInput);
    }
    for (kind, mode) in [(1, 0o777), (2, 0o1777), (3, 0o777)] {
        for seconds in [i64::MIN, -1, 0, i64::MAX] {
            let mut request = request();
            request.operation = Operation::ConstructPortableMetadata {
                kind,
                mode,
                mtime_seconds: seconds,
                mtime_nanoseconds: 999_999_999,
            };
            assert_eq!(
                decode_request(1, &encode_request(&request).unwrap()).unwrap(),
                request
            );
        }
    }
    let mut wrong = request();
    wrong.response_bytes = 1;
    assert_eq!(wrong.validate().unwrap_err().code, Code::InvalidInput);
    for profile in [2, 3] {
        wrong = request();
        wrong.profile = profile;
        assert_eq!(wrong.validate().unwrap_err().code, Code::Unsupported);
    }
    for budget in [0, 600_001] {
        wrong = request();
        wrong.deadline_ms = budget;
        assert!(wrong.validate().is_err());
    }
    wrong = request();
    wrong.deadline_ms = 600_000;
    assert!(wrong.validate().is_ok());
}

#[test]
fn native_constructor_correlates_every_echo_and_preserves_unknown_without_replay() {
    use layerfs_bridge::adapters::native::{
        client::Client,
        connection::{accept, connect, Peer, VerifiedPeer},
        listen,
    };
    // Exact success, each independent echo mismatch, wrong typed result,
    // forbidden ResultData, and a lost mutation terminal.
    for scenario in 0..8 {
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
                if scenario == 7 {
                    return;
                }
                let mut response = constructed();
                if let Response::MetadataConstructed {
                    kind,
                    mode,
                    mtime_seconds,
                    mtime_nanoseconds,
                    ..
                } = &mut response
                {
                    match scenario {
                        1 => *kind = 2,
                        2 => *mode = 0o600,
                        3 => *mtime_seconds = -3,
                        4 => *mtime_nanoseconds = 1,
                        _ => {}
                    }
                }
                if scenario == 5 {
                    response = Response::MetadataSaved {
                        base: [1; 32],
                        kind: 1,
                        mode: 0o640,
                        mtime_seconds: -2,
                        mtime_nanoseconds: 750_000_000,
                        metadata: [2; 32],
                        inserted: 7,
                        reused: 8,
                    };
                }
                let frame = if scenario == 6 {
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
                if (1..=6).contains(&scenario) {
                    assert!(connection.receive.read().is_err(),
                        "scenario {scenario}: client sent another frame after the malformed terminal");
                }
            });
            let mut client = Client::new(connect(address, 1, &[7; 32], &public).unwrap()).unwrap();
            let mut output = Vec::new();
            let result = client.call(&request(), &mut &[][..], &mut output);
            if scenario == 0 {
                assert_eq!(result.unwrap(), constructed());
            } else {
                let failure = result.unwrap_err();
                assert_eq!(failure.code, Code::Unknown);
                assert!(failure.unknown);
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
