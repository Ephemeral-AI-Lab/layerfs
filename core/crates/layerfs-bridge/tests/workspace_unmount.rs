#![cfg(feature = "native")]
use layerfs_bridge::{
    adapters::native::{
        client::Client,
        connection::{accept, connect, Peer, VerifiedPeer},
        listen,
        protocol::*,
        server::serve,
    },
    contract::*,
};
use std::sync::atomic::{AtomicUsize, Ordering};

fn request() -> Request {
    Request {
        id: 1,
        generation: 0,
        store: 0,
        profile: WORKSPACE_STATUS_PROFILE,
        deadline_ms: WORKSPACE_UNMOUNT_MAX_MS,
        response_bytes: 0,
        operation: Operation::WorkspaceUnmount {
            workspace: b"mounted".to_vec(),
            incarnation: [4; 32],
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
fn response(outcome: WorkspaceLifecycleOutcome) -> Response {
    Response::WorkspaceUnmount(Box::new(value(outcome)))
}
fn peer() -> Peer {
    Peer {
        selector: 1,
        public: *VerifiedPeer::from_private(&[7; 32]).unwrap().public_key(),
        expires_unix: u64::MAX,
    }
}

#[test]
fn unmount_is_an_exact_bounded_lifecycle_mutation_without_store_grants() {
    let r = request();
    assert_eq!(r.operation.opcode(), 10);
    assert_eq!(r.operation.label(), "WorkspaceUnmount");
    assert!(!r.operation.read_only());
    assert!(r.operation.mutation());
    assert!(!r.operation.content_mutation());
    assert!(!r.operation.metadata_mutation());
    assert_eq!(r.operation.input_length().unwrap(), 0);
    assert_eq!(permission_bit(10), None);
    assert_eq!(permission_bit(9), Some(0x80));
    assert_eq!(decode_request(1, &encode_request(&r).unwrap()).unwrap(), r);
    for profile in [0, 1, 2, 4] {
        let mut invalid = r.clone();
        invalid.profile = profile;
        assert_eq!(invalid.validate().unwrap_err().code, Code::Unsupported);
    }
    for field in 0..6 {
        let mut invalid = r.clone();
        match field {
            0 => invalid.id = 0,
            1 => invalid.store = 1,
            2 => invalid.generation = 1,
            3 => invalid.response_bytes = 1,
            4 => invalid.deadline_ms = 0,
            _ => invalid.deadline_ms = WORKSPACE_UNMOUNT_MAX_MS + 1,
        }
        assert_eq!(invalid.validate().unwrap_err().code, Code::InvalidInput);
        assert!(encode_request(&invalid).is_err());
    }
    for workspace in [
        vec![],
        vec![b'a'; 64],
        b"../x".to_vec(),
        b"x/y".to_vec(),
        b"a\0b".to_vec(),
        vec![255],
    ] {
        let mut invalid = r.clone();
        invalid.operation = Operation::WorkspaceUnmount {
            workspace,
            incarnation: [4; 32],
        };
        assert_eq!(invalid.validate().unwrap_err().code, Code::InvalidInput);
    }
    let mut invalid = r.clone();
    invalid.operation = Operation::WorkspaceUnmount {
        workspace: b"mounted".to_vec(),
        incarnation: [0; 32],
    };
    assert_eq!(invalid.validate().unwrap_err().code, Code::InvalidInput);

    let mut widest = r.clone();
    widest.operation = Operation::WorkspaceUnmount {
        workspace: vec![b'a'; WORKSPACE_ID_BYTES],
        incarnation: [4; 32],
    };
    let bytes = encode_request(&widest).unwrap();
    assert_eq!(bytes.len(), WORKSPACE_UNMOUNT_REQUEST_BYTES);
    assert_eq!(decode_request(1, &bytes).unwrap(), widest);
    for end in 0..bytes.len() {
        assert!(decode_request(1, &bytes[..end]).is_err());
    }
    assert!(decode_request(0, &bytes).is_err());
    let mut oversized = bytes;
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
    let shortened = encode_request_with_budget(&r, 1).unwrap();
    assert_eq!(decode_request(1, &shortened).unwrap().deadline_ms, 1);
    assert!(encode_request_with_budget(&r, 0).is_err());
    assert!(encode_request_with_budget(&r, WORKSPACE_UNMOUNT_MAX_MS + 1).is_err());
    let failure = Failure::from(Code::Deadline);
    assert_eq!(
        decode_request_failure(&r, &encode_request_failure(&r, &failure).unwrap()).unwrap(),
        failure
    );
}

#[test]
fn entered_outcomes_have_exact_identity_bounds_and_a_closed_retained_code_set() {
    for outcome in [
        WorkspaceLifecycleOutcome::Completed,
        WorkspaceLifecycleOutcome::Retained(Code::Deadline),
        WorkspaceLifecycleOutcome::Retained(Code::Io),
        WorkspaceLifecycleOutcome::Retained(Code::Busy),
        WorkspaceLifecycleOutcome::Retained(Code::Unsupported),
    ] {
        let mut result = value(outcome);
        result.workspace = vec![b'a'; WORKSPACE_ID_BYTES];
        let encoded =
            encode_response(&Response::WorkspaceUnmount(Box::new(result.clone()))).unwrap();
        assert_eq!(encoded[0], 12);
        assert_eq!(
            encoded.len(),
            WORKSPACE_UNMOUNT_RESULT_BYTES
                - usize::from(outcome == WorkspaceLifecycleOutcome::Completed)
        );
        assert_eq!(
            decode_response(&encoded).unwrap(),
            Response::WorkspaceUnmount(Box::new(result))
        );
        for end in 0..encoded.len() {
            assert!(decode_response(&encoded[..end]).is_err());
        }
    }
    let base = encode_response(&response(WorkspaceLifecycleOutcome::Retained(Code::Io))).unwrap();
    for code in 0..=u8::MAX {
        let mut bytes = base.clone();
        *bytes.last_mut().unwrap() = code;
        let valid = [Code::Deadline, Code::Io, Code::Busy, Code::Unsupported]
            .iter()
            .any(|allowed| *allowed as u8 == code);
        assert_eq!(
            decode_response(&bytes).is_ok(),
            valid,
            "retained code {code}"
        );
    }
    for code in [
        Code::Denied,
        Code::Unknown,
        Code::InvalidInput,
        Code::Capacity,
        Code::NotFound,
    ] {
        let invalid = value(WorkspaceLifecycleOutcome::Retained(code));
        assert!(invalid.validate().is_err());
        assert!(encode_response(&Response::WorkspaceUnmount(Box::new(invalid))).is_err());
    }
    let mut invalid = value(WorkspaceLifecycleOutcome::Completed);
    invalid.incarnation = [0; 32];
    assert!(invalid.validate().is_err());
    invalid.incarnation = [4; 32];
    invalid.workspace = b"bad/name".to_vec();
    assert!(invalid.validate().is_err());
    let mut bytes = encode_response(&response(WorkspaceLifecycleOutcome::Completed)).unwrap();
    *bytes.last_mut().unwrap() = 2;
    assert!(decode_response(&bytes).is_err());
    bytes = encode_response(&response(WorkspaceLifecycleOutcome::Completed)).unwrap();
    bytes[3 + b"mounted".len()..3 + b"mounted".len() + 32].fill(0);
    assert!(decode_response(&bytes).is_err());
    let mut trailing = encode_response(&response(WorkspaceLifecycleOutcome::Completed)).unwrap();
    trailing.push(0);
    assert_eq!(
        decode_response(&trailing).unwrap_err().code,
        Code::InvalidInput
    );
    let mut widest = value(WorkspaceLifecycleOutcome::Retained(Code::Deadline));
    widest.workspace = vec![b'a'; WORKSPACE_ID_BYTES];
    let mut oversized = encode_response(&Response::WorkspaceUnmount(Box::new(widest))).unwrap();
    oversized.push(0);
    assert_eq!(
        decode_response(&oversized).unwrap_err().code,
        Code::Capacity
    );
}

#[test]
fn authenticated_entered_attempts_and_pre_admission_failures_remain_distinct() {
    for expected in [
        Ok(response(WorkspaceLifecycleOutcome::Completed)),
        Ok(response(WorkspaceLifecycleOutcome::Retained(
            Code::Deadline,
        ))),
        Ok(response(WorkspaceLifecycleOutcome::Retained(Code::Io))),
        Ok(response(WorkspaceLifecycleOutcome::Retained(Code::Busy))),
        Ok(response(WorkspaceLifecycleOutcome::Retained(
            Code::Unsupported,
        ))),
        Err(Failure::from(Code::Deadline)),
        Err(Failure::from(Code::Denied)),
        Err(Failure::from(Code::Busy)),
    ] {
        let listener = listen("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = listener.local_addr().unwrap();
        let server_key = *VerifiedPeer::from_private(&[9; 32]).unwrap().public_key();
        let calls = AtomicUsize::new(0);
        std::thread::scope(|threads| {
            let server = threads.spawn(|| {
                let (socket, _) = listener.accept().unwrap();
                let connection = accept(socket, &[9; 32], &[peer()]).unwrap();
                let _closed = serve(connection, |peer, received, input, _, _| {
                    assert_eq!(peer.public_key(), &self::peer().public);
                    assert_eq!(received.operation, request().operation);
                    assert_eq!(
                        (received.store, received.generation, received.profile),
                        (0, 0, 3)
                    );
                    assert!((1..=WORKSPACE_UNMOUNT_MAX_MS).contains(&received.deadline_ms));
                    let mut bytes = Vec::new();
                    input.read_to_end(&mut bytes)?;
                    assert!(bytes.is_empty());
                    calls.fetch_add(1, Ordering::SeqCst);
                    expected.clone()
                });
            });
            let mut client =
                Client::new(connect(address, 1, &[7; 32], &server_key).unwrap()).unwrap();
            let mut output = Vec::new();
            assert_eq!(client.call(&request(), &mut &[][..], &mut output), expected);
            assert!(output.is_empty());
            drop(client);
            server.join().unwrap();
        });
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}

#[derive(Clone, Copy, Debug)]
enum Fault {
    Workspace,
    Incarnation,
    RequestId,
    WrongResponse,
    Malformed,
    MalformedFailure,
    RetainedCode,
    Trailing,
    Lost,
    ResultData,
}
#[test]
fn delivered_bad_or_lost_unmount_results_are_unknown_and_never_replayed() {
    for fault in [
        Fault::Workspace,
        Fault::Incarnation,
        Fault::RequestId,
        Fault::WrongResponse,
        Fault::Malformed,
        Fault::MalformedFailure,
        Fault::RetainedCode,
        Fault::Trailing,
        Fault::Lost,
        Fault::ResultData,
    ] {
        let listener = listen("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = listener.local_addr().unwrap();
        let server_key = *VerifiedPeer::from_private(&[9; 32]).unwrap().public_key();
        let calls = AtomicUsize::new(0);
        std::thread::scope(|threads| {
            let server = threads.spawn(|| {
                let (socket, _) = listener.accept().unwrap();
                let mut connection = accept(socket, &[9; 32], &[peer()]).unwrap();
                let hello = connection.receive.read().unwrap();
                assert_eq!(hello.kind, Kind::Hello);
                connection.send.write(&hello).unwrap();
                let begin = connection.receive.read().unwrap();
                assert_eq!(begin.kind, Kind::Begin);
                assert_eq!(
                    decode_request(begin.id, &begin.bytes).unwrap().operation,
                    request().operation
                );
                let end = connection.receive.read().unwrap();
                let mut input = InputState::new(begin.id, 0);
                input.accept(&end).unwrap();
                assert!(input.ended());
                calls.fetch_add(1, Ordering::SeqCst);
                if matches!(fault, Fault::Lost) {
                    return;
                }
                let mut result = value(WorkspaceLifecycleOutcome::Completed);
                if matches!(fault, Fault::Workspace) {
                    result.workspace = b"other".to_vec();
                }
                if matches!(fault, Fault::Incarnation) {
                    result.incarnation = [5; 32];
                }
                let mut bytes =
                    encode_response(&Response::WorkspaceUnmount(Box::new(result))).unwrap();
                match fault {
                    Fault::WrongResponse => {
                        bytes = encode_response(&Response::Read { length: 0 }).unwrap()
                    }
                    Fault::Malformed => bytes = vec![12],
                    Fault::MalformedFailure => bytes = vec![Code::Io as u8, 2, 0],
                    Fault::RetainedCode => {
                        bytes = encode_response(&response(WorkspaceLifecycleOutcome::Retained(
                            Code::Io,
                        )))
                        .unwrap();
                        *bytes.last_mut().unwrap() = Code::Denied as u8;
                    }
                    Fault::Trailing => bytes.push(0),
                    Fault::ResultData => bytes = vec![1],
                    _ => (),
                }
                connection
                    .send
                    .write(&Frame {
                        kind: match fault {
                            Fault::ResultData => Kind::ResultData,
                            Fault::MalformedFailure => Kind::Failure,
                            _ => Kind::Success,
                        },
                        id: if matches!(fault, Fault::RequestId) {
                            2
                        } else {
                            1
                        },
                        bytes,
                    })
                    .unwrap();
            });
            let mut client =
                Client::new(connect(address, 1, &[7; 32], &server_key).unwrap()).unwrap();
            let mut output = Vec::new();
            let failure = client
                .call(&request(), &mut &[][..], &mut output)
                .unwrap_err();
            assert_eq!(failure, Failure::from(Code::Unknown), "{fault:?}");
            assert!(output.is_empty());
            let mut replay = request();
            replay.id = 2;
            assert_eq!(
                client
                    .call(&replay, &mut &[][..], &mut output)
                    .unwrap_err()
                    .code,
                Code::InvalidInput
            );
            drop(client);
            server.join().unwrap();
        });
        assert_eq!(calls.load(Ordering::SeqCst), 1, "{fault:?}");
    }
}
