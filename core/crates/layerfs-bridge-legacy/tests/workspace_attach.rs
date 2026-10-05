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

fn request(status: bool) -> Request {
    let workspace = b"mounted".to_vec();
    let incarnation = [4; 32];
    Request {
        id: 1,
        generation: 0,
        store: 0,
        profile: WORKSPACE_STATUS_PROFILE,
        deadline_ms: WORKSPACE_ATTACH_MAX_MS,
        response_bytes: 0,
        operation: if status {
            Operation::WorkspaceStatus {
                workspace,
                incarnation,
            }
        } else {
            Operation::WorkspaceAttach {
                workspace,
                incarnation,
            }
        },
    }
}
fn attach(outcome: WorkspaceAttachOutcome) -> Response {
    Response::WorkspaceAttach(Box::new(WorkspaceAttachWire {
        workspace: b"mounted".to_vec(),
        incarnation: [4; 32],
        outcome,
    }))
}
fn attachment(state: WorkspaceAttachmentState) -> Response {
    Response::WorkspaceAttachment(Box::new(WorkspaceAttachmentWire {
        workspace: b"mounted".to_vec(),
        incarnation: [4; 32],
        state,
    }))
}
fn codes() -> impl Iterator<Item = Code> {
    (1..=17).map(|value| {
        decode_failure(&[value, u8::from(value == 12), 0])
            .unwrap()
            .code
    })
}
fn roundtrip(response: Response, tag: u8) -> Vec<u8> {
    let bytes = encode_response(&response).unwrap();
    assert_eq!(bytes[0], tag);
    assert_eq!(decode_response(&bytes).unwrap(), response);
    for end in 0..bytes.len() {
        assert!(decode_response(&bytes[..end]).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(decode_response(&trailing).is_err());
    bytes
}

#[test]
fn attach_is_identity_only_bounded_profile_three_mutation() {
    let r = request(false);
    assert_eq!(r.operation.opcode(), 13);
    assert_eq!(r.operation.label(), "WorkspaceAttach");
    assert!(r.operation.mutation());
    assert!(!r.operation.read_only());
    assert!(!r.operation.content_mutation());
    assert!(!r.operation.metadata_mutation());
    assert_eq!(r.operation.input_length().unwrap(), 0);
    assert_eq!(permission_bit(WORKSPACE_ATTACH_OPCODE), None);
    let encoded = encode_request(&r).unwrap();
    assert_eq!(encoded[26], 13);
    assert_eq!(decode_request(1, &encoded).unwrap(), r);
    for field in 0..7 {
        let mut invalid = r.clone();
        match field {
            0 => invalid.id = 0,
            1 => invalid.store = 1,
            2 => invalid.generation = 1,
            3 => invalid.response_bytes = 1,
            4 => invalid.deadline_ms = 0,
            5 => invalid.deadline_ms = WORKSPACE_ATTACH_MAX_MS + 1,
            _ => invalid.profile = 1,
        }
        assert_eq!(
            encode_request(&invalid).unwrap_err().code,
            if field == 6 {
                Code::Unsupported
            } else {
                Code::InvalidInput
            }
        );
    }
    for (workspace, incarnation) in [
        (vec![], [4; 32]),
        (vec![b'a'; 64], [4; 32]),
        (b"bad/name".to_vec(), [4; 32]),
        (b"mounted".to_vec(), [0; 32]),
    ] {
        let mut invalid = r.clone();
        invalid.operation = Operation::WorkspaceAttach {
            workspace,
            incarnation,
        };
        assert_eq!(invalid.validate().unwrap_err().code, Code::InvalidInput);
    }
    let mut widest = r.clone();
    widest.operation = Operation::WorkspaceAttach {
        workspace: vec![b'a'; WORKSPACE_ID_BYTES],
        incarnation: [4; 32],
    };
    let mut bytes = encode_request(&widest).unwrap();
    assert_eq!(bytes.len(), WORKSPACE_ATTACH_REQUEST_BYTES);
    for end in 0..bytes.len() {
        assert!(decode_request(1, &bytes[..end]).is_err());
    }
    bytes.push(0);
    assert_eq!(decode_request(1, &bytes).unwrap_err().code, Code::Capacity);
    let mut trailing = encoded;
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
}

#[test]
fn attach_and_failed_status_preserve_codes_without_widening_lifecycle() {
    roundtrip(attach(WorkspaceAttachOutcome::Completed), 15);
    roundtrip(attachment(WorkspaceAttachmentState::Attaching), 16);
    for code in codes() {
        roundtrip(attach(WorkspaceAttachOutcome::Retained(code)), 15);
        for cleanup in [None, Some(code)] {
            for flags in 0..=8 {
                let progress = if flags == 8 {
                    WorkspaceAttachmentProgress::Running
                } else {
                    WorkspaceAttachmentProgress::Retained {
                        mount_directory: flags & 1 != 0,
                        metadata_arena: flags & 2 != 0,
                        backing_directory: flags & 4 != 0,
                    }
                };
                roundtrip(
                    attachment(WorkspaceAttachmentState::Failed {
                        cause: code,
                        cleanup,
                        progress,
                    }),
                    16,
                );
            }
        }
        let old = WorkspaceLifecycleWire {
            workspace: b"mounted".to_vec(),
            incarnation: [4; 32],
            outcome: WorkspaceLifecycleOutcome::Retained(code),
        };
        assert_eq!(
            old.validate().is_ok(),
            matches!(
                code,
                Code::Deadline | Code::Io | Code::Busy | Code::Unsupported
            )
        );
    }
    let responses = [
        attach(WorkspaceAttachOutcome::Retained(Code::Capacity)),
        attachment(WorkspaceAttachmentState::Failed {
            cause: Code::Io,
            cleanup: Some(Code::Denied),
            progress: WorkspaceAttachmentProgress::Retained {
                mount_directory: true,
                metadata_arena: true,
                backing_directory: true,
            },
        }),
    ];
    for mut response in responses {
        let (workspace, incarnation) = match &mut response {
            Response::WorkspaceAttach(value) => (&mut value.workspace, &mut value.incarnation),
            Response::WorkspaceAttachment(value) => (&mut value.workspace, &mut value.incarnation),
            _ => unreachable!(),
        };
        *workspace = vec![b'a'; WORKSPACE_ID_BYTES];
        *incarnation = [0; 32];
        assert_eq!(
            encode_response(&response).unwrap_err().code,
            Code::InvalidInput
        );
        match &mut response {
            Response::WorkspaceAttach(value) => value.incarnation = [4; 32],
            Response::WorkspaceAttachment(value) => value.incarnation = [4; 32],
            _ => unreachable!(),
        }
        let bytes = encode_response(&response).unwrap();
        let maximum = if bytes[0] == 15 {
            WORKSPACE_ATTACH_RESULT_BYTES
        } else {
            WORKSPACE_ATTACHMENT_RESULT_BYTES
        };
        assert_eq!(bytes.len(), maximum);
        // Reject unknown state/outcome discriminants and failure codes.
        for (offset, value) in [(98, 2), (99, 0), (99, 18)] {
            let mut invalid = bytes.clone();
            invalid[offset] = value;
            assert_eq!(
                decode_response(&invalid).unwrap_err().code,
                Code::InvalidInput
            );
        }
        if bytes[0] == 16 {
            for (offset, value) in [(100, 18), (101, 2), (102, 8)] {
                let mut invalid = bytes.clone();
                invalid[offset] = value;
                assert_eq!(
                    decode_response(&invalid).unwrap_err().code,
                    Code::InvalidInput
                );
            }
        }
        let mut oversized = bytes;
        oversized.push(0);
        assert_eq!(
            decode_response(&oversized).unwrap_err().code,
            Code::Capacity
        );
    }
}

#[derive(Clone, Copy, Debug)]
enum Scenario {
    Completed,
    Retained,
    Refused,
    WrongWorkspace,
    WrongIncarnation,
    WrongReply,
    WrongLifecycle(u8),
    WrongId,
    Malformed,
    Lost,
    ResultData,
}
#[test]
fn authenticated_attach_and_status_correlate_without_replay() {
    for status in [false, true] {
        for scenario in [
            Scenario::Completed,
            Scenario::Retained,
            Scenario::Refused,
            Scenario::WrongWorkspace,
            Scenario::WrongIncarnation,
            Scenario::WrongReply,
            Scenario::WrongLifecycle(12),
            Scenario::WrongLifecycle(13),
            Scenario::WrongLifecycle(14),
            Scenario::WrongId,
            Scenario::Malformed,
            Scenario::Lost,
            Scenario::ResultData,
        ] {
            terminal(status, scenario);
        }
    }
}
fn terminal(status: bool, scenario: Scenario) {
    let r = request(status);
    let reply = if status {
        attachment(if matches!(scenario, Scenario::Retained) {
            WorkspaceAttachmentState::Failed {
                cause: Code::Capacity,
                cleanup: Some(Code::Denied),
                progress: WorkspaceAttachmentProgress::Retained {
                    mount_directory: false,
                    metadata_arena: true,
                    backing_directory: true,
                },
            }
        } else {
            WorkspaceAttachmentState::Attaching
        })
    } else {
        attach(if matches!(scenario, Scenario::Retained) {
            WorkspaceAttachOutcome::Retained(Code::Capacity)
        } else {
            WorkspaceAttachOutcome::Completed
        })
    };
    let listener = listen("127.0.0.1:0".parse().unwrap()).unwrap();
    let address = listener.local_addr().unwrap();
    let public = *VerifiedPeer::from_private(&[9; 32]).unwrap().public_key();
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
            let mut response = reply.clone();
            if matches!(scenario, Scenario::WrongReply) {
                response = if status {
                    attach(WorkspaceAttachOutcome::Completed)
                } else {
                    attachment(WorkspaceAttachmentState::Attaching)
                };
            }
            let mut bytes = encode_response(&response).unwrap();
            match scenario {
                Scenario::WrongWorkspace => bytes[3] = b'x',
                Scenario::WrongIncarnation => bytes[10] = 5,
                Scenario::WrongLifecycle(tag) => {
                    bytes = encode_response(&Response::WorkspaceMount(Box::new(
                        WorkspaceLifecycleWire {
                            workspace: b"mounted".to_vec(),
                            incarnation: [4; 32],
                            outcome: WorkspaceLifecycleOutcome::Completed,
                        },
                    )))
                    .unwrap();
                    bytes[0] = tag;
                }
                _ => {}
            }
            let kind = match scenario {
                Scenario::Refused => {
                    bytes = encode_failure(Code::Denied.into()).to_vec();
                    Kind::Failure
                }
                Scenario::ResultData => {
                    bytes = vec![1];
                    Kind::ResultData
                }
                Scenario::Malformed => {
                    bytes.truncate(1);
                    Kind::Success
                }
                _ => Kind::Success,
            };
            connection
                .send
                .write(&Frame {
                    kind,
                    id: r.id + u64::from(matches!(scenario, Scenario::WrongId)),
                    bytes,
                })
                .unwrap();
        });
        let mut client = Client::new(connect(address, 1, &[7; 32], &public).unwrap()).unwrap();
        let mut output = Vec::new();
        let result = client.call(&r, &mut &[][..], &mut output);
        match scenario {
            Scenario::Completed | Scenario::Retained => assert_eq!(result.unwrap(), reply),
            Scenario::Refused => assert_eq!(result.unwrap_err(), Failure::from(Code::Denied)),
            _ => {
                assert_eq!(
                    result.unwrap_err(),
                    Failure::from(if status { Code::Io } else { Code::Unknown }),
                    "{status}/{scenario:?}"
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
    assert_eq!(calls.load(Ordering::SeqCst), 1, "{status}/{scenario:?}");
}
