#![cfg(feature = "native")]
use layerfs_bridge::{
    adapters::native::{
        client::Client,
        connection::{accept, connect, Peer, VerifiedPeer},
        listen,
        protocol::*,
    },
    contract::*,
};
use std::sync::atomic::{AtomicUsize, Ordering};

fn request(close: bool) -> Request {
    let workspace = b"mounted".to_vec();
    let incarnation = [4; 32];
    Request {
        id: 1,
        generation: 0,
        store: 0,
        profile: WORKSPACE_STATUS_PROFILE,
        deadline_ms: WORKSPACE_CLOSE_CLEAN_MAX_MS,
        response_bytes: 0,
        operation: if close {
            Operation::WorkspaceCloseClean {
                workspace,
                incarnation,
            }
        } else {
            Operation::WorkspaceUnmount {
                workspace,
                incarnation,
            }
        },
    }
}
fn value(outcome: WorkspaceLifecycleOutcome) -> WorkspaceLifecycleWire {
    WorkspaceLifecycleWire {
        workspace: b"mounted".to_vec(),
        incarnation: [4; 32],
        outcome,
    }
}
fn response(close: bool, value: WorkspaceLifecycleWire) -> Response {
    if close {
        Response::WorkspaceCloseClean(Box::new(value))
    } else {
        Response::WorkspaceUnmount(Box::new(value))
    }
}

#[test]
fn close_clean_bounds_and_shared_outcomes_preserve_unmount_wire_bytes() {
    let r = request(true);
    assert_eq!(r.operation.opcode(), 11);
    assert_eq!(r.operation.label(), "WorkspaceCloseClean");
    assert!(!r.operation.read_only());
    assert!(r.operation.mutation());
    assert!(!r.operation.content_mutation());
    assert!(!r.operation.metadata_mutation());
    assert_eq!(r.operation.input_length().unwrap(), 0);
    assert_eq!(permission_bit(11), None);
    assert_eq!(permission_bit(10), None);
    assert_eq!(permission_bit(9), Some(0x80));
    let encoded = encode_request(&r).unwrap();
    assert_eq!(decode_request(1, &encoded).unwrap(), r);
    let mut unmount = encode_request(&request(false)).unwrap();
    assert_eq!(unmount[26], 10);
    unmount[26] = 11;
    assert_eq!(unmount, encoded);
    for field in 0..7 {
        let mut invalid = r.clone();
        match field {
            0 => invalid.id = 0,
            1 => invalid.store = 1,
            2 => invalid.generation = 1,
            3 => invalid.response_bytes = 1,
            4 => invalid.deadline_ms = 0,
            5 => invalid.deadline_ms = WORKSPACE_CLOSE_CLEAN_MAX_MS + 1,
            _ => invalid.profile = 1,
        }
        let code = if field == 6 {
            Code::Unsupported
        } else {
            Code::InvalidInput
        };
        assert_eq!(encode_request(&invalid).unwrap_err().code, code);
    }
    for (workspace, incarnation) in [
        (vec![], [4; 32]),
        (vec![b'a'; 64], [4; 32]),
        (b"bad/name".to_vec(), [4; 32]),
        (b"mounted".to_vec(), [0; 32]),
    ] {
        let mut invalid = r.clone();
        invalid.operation = Operation::WorkspaceCloseClean {
            workspace,
            incarnation,
        };
        assert_eq!(invalid.validate().unwrap_err().code, Code::InvalidInput);
    }
    let mut widest = r.clone();
    widest.operation = Operation::WorkspaceCloseClean {
        workspace: vec![b'a'; WORKSPACE_ID_BYTES],
        incarnation: [4; 32],
    };
    let encoded = encode_request(&widest).unwrap();
    assert_eq!(encoded.len(), WORKSPACE_CLOSE_CLEAN_REQUEST_BYTES);
    for end in 0..encoded.len() {
        assert!(decode_request(1, &encoded[..end]).is_err());
    }
    let mut oversized = encoded;
    oversized.push(0);
    assert_eq!(
        decode_request(1, &oversized).unwrap_err().code,
        Code::Capacity
    );
    let mut trailing = encode_request(&r).unwrap();
    trailing.push(0);
    assert_eq!(
        decode_request(1, &trailing).unwrap_err().code,
        Code::InvalidInput
    );
    assert_eq!(
        decode_request(1, &encode_request_with_budget(&r, 1).unwrap())
            .unwrap()
            .deadline_ms,
        1
    );

    for outcome in [
        WorkspaceLifecycleOutcome::Completed,
        WorkspaceLifecycleOutcome::Retained(Code::Deadline),
    ] {
        let mut old_bytes = vec![12, 0, 7];
        old_bytes.extend_from_slice(b"mounted");
        old_bytes.extend_from_slice(&[4; 32]);
        old_bytes.push(u8::from(outcome != WorkspaceLifecycleOutcome::Completed));
        if let WorkspaceLifecycleOutcome::Retained(code) = outcome {
            old_bytes.push(code as u8);
        }
        assert_eq!(
            encode_response(&response(false, value(outcome))).unwrap(),
            old_bytes
        );
        let closed = response(true, value(outcome));
        let bytes = encode_response(&closed).unwrap();
        old_bytes[0] = 13;
        assert_eq!(bytes, old_bytes);
        assert_eq!(decode_response(&bytes).unwrap(), closed);
        for end in 0..bytes.len() {
            assert!(decode_response(&bytes[..end]).is_err());
        }
        let mut widest = value(outcome);
        widest.workspace = vec![b'a'; WORKSPACE_ID_BYTES];
        let mut bytes = encode_response(&response(true, widest)).unwrap();
        assert_eq!(
            bytes.len(),
            WORKSPACE_CLOSE_CLEAN_RESULT_BYTES
                - usize::from(outcome == WorkspaceLifecycleOutcome::Completed)
        );
        bytes.push(0);
        assert!(decode_response(&bytes).is_err());
    }
    let mut trailing =
        encode_response(&response(true, value(WorkspaceLifecycleOutcome::Completed))).unwrap();
    trailing.push(0);
    assert_eq!(
        decode_response(&trailing).unwrap_err().code,
        Code::InvalidInput
    );
}

#[derive(Clone, Copy, Debug)]
enum Scenario {
    Completed,
    Retained,
    Refused,
    WrongWorkspace,
    WrongIncarnation,
    UnmountReply,
    CloseReplyToUnmount,
    Malformed,
    Lost,
    ResultData,
}
#[test]
fn authenticated_close_correlates_its_variant_and_identity_without_replay() {
    for scenario in [
        Scenario::Completed,
        Scenario::Retained,
        Scenario::Refused,
        Scenario::WrongWorkspace,
        Scenario::WrongIncarnation,
        Scenario::UnmountReply,
        Scenario::CloseReplyToUnmount,
        Scenario::Malformed,
        Scenario::Lost,
        Scenario::ResultData,
    ] {
        let r = request(!matches!(scenario, Scenario::CloseReplyToUnmount));
        let listener = listen("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = listener.local_addr().unwrap();
        let server_key = *VerifiedPeer::from_private(&[9; 32]).unwrap().public_key();
        let calls = AtomicUsize::new(0);
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
                assert_eq!(hello.kind, Kind::Hello);
                connection.send.write(&hello).unwrap();
                let begin = connection.receive.read().unwrap();
                assert_eq!(begin.kind, Kind::Begin);
                assert_eq!(
                    decode_request(begin.id, &begin.bytes).unwrap().operation,
                    r.operation
                );
                let mut input = InputState::new(r.id, 0);
                input.accept(&connection.receive.read().unwrap()).unwrap();
                assert!(input.ended());
                calls.fetch_add(1, Ordering::SeqCst);
                if matches!(scenario, Scenario::Lost) {
                    return;
                }
                let outcome = if matches!(scenario, Scenario::Retained) {
                    WorkspaceLifecycleOutcome::Retained(Code::Deadline)
                } else {
                    WorkspaceLifecycleOutcome::Completed
                };
                let mut result = value(outcome);
                if matches!(scenario, Scenario::WrongWorkspace) {
                    result.workspace = b"other".to_vec();
                }
                if matches!(scenario, Scenario::WrongIncarnation) {
                    result.incarnation = [5; 32];
                }
                let mut bytes = encode_response(&response(
                    !matches!(scenario, Scenario::UnmountReply),
                    result,
                ))
                .unwrap();
                let kind = match scenario {
                    Scenario::Refused => {
                        bytes = encode_failure(Code::Deadline.into()).to_vec();
                        Kind::Failure
                    }
                    Scenario::ResultData => {
                        bytes = vec![1];
                        Kind::ResultData
                    }
                    Scenario::Malformed => {
                        bytes = vec![13];
                        Kind::Success
                    }
                    _ => Kind::Success,
                };
                connection
                    .send
                    .write(&Frame {
                        kind,
                        id: r.id,
                        bytes,
                    })
                    .unwrap();
            });
            let mut client =
                Client::new(connect(address, 1, &[7; 32], &server_key).unwrap()).unwrap();
            let mut output = Vec::new();
            let result = client.call(&r, &mut &[][..], &mut output);
            match scenario {
                Scenario::Completed => assert_eq!(
                    result.unwrap(),
                    response(true, value(WorkspaceLifecycleOutcome::Completed))
                ),
                Scenario::Retained => assert_eq!(
                    result.unwrap(),
                    response(
                        true,
                        value(WorkspaceLifecycleOutcome::Retained(Code::Deadline))
                    )
                ),
                Scenario::Refused => assert_eq!(result.unwrap_err(), Failure::from(Code::Deadline)),
                _ => {
                    assert_eq!(
                        result.unwrap_err(),
                        Failure::from(Code::Unknown),
                        "{scenario:?}"
                    );
                    let mut replay = r.clone();
                    replay.id += 1;
                    assert_eq!(
                        client
                            .call(&replay, &mut &[][..], &mut output)
                            .unwrap_err()
                            .code,
                        Code::InvalidInput
                    );
                }
            }
            assert!(output.is_empty());
            drop(client);
            server.join().unwrap();
        });
        assert_eq!(calls.load(Ordering::SeqCst), 1, "{scenario:?}");
    }
}
