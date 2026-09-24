#![cfg(feature = "native")]
use layerfs_bridge::{adapters::native::protocol::*, contract::*};

fn request() -> Request {
    Request {
        id: 1,
        generation: 0,
        store: 0,
        profile: WORKSPACE_STATUS_PROFILE,
        deadline_ms: WORKSPACE_STATUS_MAX_MS,
        response_bytes: 0,
        operation: Operation::WorkspaceStatus {
            workspace: b"mounted".to_vec(),
            incarnation: [4; 32],
        },
    }
}
fn status() -> WorkspaceStatusWire {
    WorkspaceStatusWire {
        workspace: b"mounted".to_vec(),
        incarnation: [4; 32],
        mounted: true,
        stopping: false,
        closed: false,
        active_operations: 1,
        nodes: 2,
        handles: 3,
        cookies: 4,
        consumer_accounted_bytes: 5,
        projection: [7; PROJECTION_CLASSES],
        upstream_calls: 11,
        range_accepted_payload_bytes: 13,
        range_shifted_suffix_bytes: 17,
    }
}

#[test]
fn status_profile_is_bounded_and_has_no_store_permission() {
    let mut r = request();
    assert_eq!(WORKSPACE_STATUS_PROFILE, 4);
    assert_eq!(&PROJECTION_CLASS_LABELS[9..], ["range_state", "range_edit"]);
    assert_eq!(r.operation.opcode(), 8);
    assert!(r.operation.read_only());
    assert!(!r.operation.mutation());
    assert!(!r.operation.content_mutation());
    assert!(!r.operation.metadata_mutation());
    assert_eq!(r.operation.input_length().unwrap(), 0);
    assert_eq!(permission_bit(r.operation.opcode()), None);
    assert_eq!(decode_request(1, &encode_request(&r).unwrap()).unwrap(), r);
    for profile in [1, 2, 3] {
        r.profile = profile;
        assert_eq!(r.validate().unwrap_err().code, Code::Unsupported);
    }
    for field in 0..5 {
        let mut invalid = request();
        match field {
            0 => invalid.store = 1,
            1 => invalid.generation = 1,
            2 => invalid.response_bytes = 1,
            3 => invalid.deadline_ms = 5001,
            _ => invalid.id = 0,
        }
        assert_eq!(invalid.validate().unwrap_err().code, Code::InvalidInput);
    }
    for id in [
        vec![],
        vec![b'a'; 64],
        b"../x".to_vec(),
        b"a\0b".to_vec(),
        vec![255],
    ] {
        let mut invalid = request();
        invalid.operation = Operation::WorkspaceStatus {
            workspace: id,
            incarnation: [4; 32],
        };
        assert_eq!(invalid.validate().unwrap_err().code, Code::InvalidInput);
    }
    r = request();
    r.operation = Operation::WorkspaceStatus {
        workspace: vec![b'a'; 63],
        incarnation: [4; 32],
    };
    let bytes = encode_request(&r).unwrap();
    assert_eq!(bytes.len(), WORKSPACE_STATUS_REQUEST_BYTES);
    for end in 0..bytes.len() {
        assert!(decode_request(1, &bytes[..end]).is_err());
    }
    let mut oversized = bytes;
    oversized.push(0);
    assert_eq!(
        decode_request(1, &oversized).unwrap_err().code,
        Code::Capacity
    );
    let mut zero_incarnation = request();
    zero_incarnation.operation = Operation::WorkspaceStatus {
        workspace: b"mounted".to_vec(),
        incarnation: [0; 32],
    };
    assert!(zero_incarnation.validate().is_err());
    let failure = Failure::from(Code::Denied);
    assert_eq!(
        encode_request_failure(&request(), &failure).unwrap(),
        encode_failure(failure.clone())
    );
    assert_eq!(
        decode_request_failure(&request(), &encode_failure(failure.clone())).unwrap(),
        failure
    );
}

#[test]
fn status_reply_preserves_exact_identity_and_refuses_bad_flags_or_closed_counts() {
    let mut value = status();
    value.workspace = vec![b'a'; 63];
    let response = Response::WorkspaceStatus(Box::new(value.clone()));
    let bytes = encode_response(&response).unwrap();
    assert_eq!(bytes.len(), WORKSPACE_STATUS_RESULT_BYTES);
    assert_eq!(bytes[0], 10);
    assert_eq!(decode_response(&bytes).unwrap(), response);
    for end in 0..bytes.len() {
        assert!(decode_response(&bytes[..end]).is_err());
    }
    let mut invalid = bytes.clone();
    invalid[1 + 2 + 63 + 32] = 8;
    assert_eq!(
        decode_response(&invalid).unwrap_err().code,
        Code::InvalidInput
    );
    invalid = bytes.clone();
    invalid[1 + 2 + 63..1 + 2 + 63 + 32].fill(0);
    assert_eq!(
        decode_response(&invalid).unwrap_err().code,
        Code::InvalidInput
    );
    let mut oversized = bytes;
    oversized.push(0);
    assert_eq!(
        decode_response(&oversized).unwrap_err().code,
        Code::Capacity
    );
    value.closed = true;
    assert!(value.validate().is_err());
    value.mounted = false;
    value.active_operations = 0;
    value.nodes = 0;
    value.handles = 0;
    value.cookies = 0;
    assert!(value.validate().is_ok());
    let response = Response::WorkspaceStatus(Box::new(value));
    assert_eq!(
        decode_response(&encode_response(&response).unwrap()).unwrap(),
        response
    );
}

#[test]
fn authenticated_status_rejects_mismatched_identity_and_result_data_without_unknown_mutation() {
    use layerfs_bridge::adapters::native::{
        client::Client,
        connection::{accept, connect, Peer, VerifiedPeer},
        listen,
    };
    for scenario in 0..4 {
        let listener = listen("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = listener.local_addr().unwrap();
        let server_key = *VerifiedPeer::from_private(&[9; 32]).unwrap().public_key();
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
                let mut value = status();
                if scenario == 1 {
                    value.incarnation = [5; 32];
                }
                if scenario == 2 {
                    value.workspace = b"other".to_vec();
                }
                let frame = if scenario == 3 {
                    Frame {
                        kind: Kind::ResultData,
                        id: 1,
                        bytes: vec![1],
                    }
                } else {
                    Frame {
                        kind: Kind::Success,
                        id: 1,
                        bytes: encode_response(&Response::WorkspaceStatus(Box::new(value)))
                            .unwrap(),
                    }
                };
                connection.send.write(&frame).unwrap();
            });
            let mut client =
                Client::new(connect(address, 1, &[7; 32], &server_key).unwrap()).unwrap();
            let mut output = Vec::new();
            let result = client.call(&request(), &mut &[][..], &mut output);
            if scenario == 0 {
                assert_eq!(
                    result.unwrap(),
                    Response::WorkspaceStatus(Box::new(status()))
                );
            } else {
                let failure = result.unwrap_err();
                assert_eq!(failure.code, Code::Io);
                assert!(!failure.unknown);
            }
            assert!(output.is_empty());
            drop(client);
            server.join().unwrap();
        });
    }
}
