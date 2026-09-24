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

fn id<const N: usize>(tag: u8) -> [u8; N] {
    let mut value = [4; N];
    value[0] = tag;
    value
}
fn request(status: bool) -> Request {
    let workspace = vec![b'a'; WORKSPACE_ID_BYTES];
    let incarnation = [4; 32];
    Request {
        id: 1,
        generation: 0,
        store: 0,
        profile: WORKSPACE_STATUS_PROFILE,
        deadline_ms: if status {
            WORKSPACE_STATUS_MAX_MS
        } else {
            WORKSPACE_COMMIT_MAX_MS
        },
        response_bytes: 0,
        operation: if status {
            Operation::WorkspaceStatus {
                workspace,
                incarnation,
            }
        } else {
            Operation::WorkspaceCommit {
                workspace,
                incarnation,
            }
        },
    }
}
fn stage() -> StageWire {
    StageWire {
        workspace: [4; 32],
        token: 1,
        stack: id(0x31),
        branch: id(0x11),
        expected_head: Some(id(0x12)),
        expected_base: id(0x32),
        expected_root: [1; 32],
        construction_base_root: [1; 32],
        intended_commit_base: id(0x32),
        candidate_root: [2; 32],
        profile: [3; 32],
        scope: [4; 32],
        generation: 1,
    }
}
fn outcome() -> CommitOutcomeWire {
    CommitOutcomeWire::Committed(CommitWire {
        commit: id(0x12),
        stack: id(0x31),
        root: [2; 32],
        parent: Some(id(0x12)),
        base_layer: id(0x32),
    })
}
fn failure() -> WorkspaceCommitFailureWire {
    WorkspaceCommitFailureWire {
        generation: 1,
        phase: WorkspaceCommitPhase::Reconcile,
        disposition: WorkspaceCommitFailureDisposition::KnownCommitLocalFailure,
        cause: Failure {
            code: Code::HeadMoved,
            unknown: false,
            cleanup: Some(Code::Io),
            history: Some(Box::new(HistoryFailure {
                conflict: Some(HistoryConflict::BranchMoved {
                    expected_head: Some(id(0x12)),
                    actual_head: Some(id(0x12)),
                    expected_base: id(0x32),
                    actual_base: id(0x32),
                }),
                stage: StageObservation::Retained(Box::new(stage())),
            })),
        },
        known_stage: Some(stage()),
        observed_stage: Some(stage()),
        known_outcome: Some(outcome()),
        observed_outcome: Some(outcome()),
        installed_revision: Some(5),
    }
}
fn result(outcome: WorkspaceCommitOutcome) -> WorkspaceCommitWire {
    WorkspaceCommitWire {
        workspace: vec![b'a'; WORKSPACE_ID_BYTES],
        incarnation: [4; 32],
        outcome,
    }
}
fn completed() -> WorkspaceCommitWire {
    result(WorkspaceCommitOutcome::Completed(
        WorkspaceCommitReportWire {
            generation: 1,
            stage_token: Some(1),
            outcome: outcome(),
            revision: 5,
        },
    ))
}
fn status() -> WorkspaceWritableStatusWire {
    WorkspaceWritableStatusWire {
        status: WorkspaceStatusWire {
            workspace: vec![b'a'; WORKSPACE_ID_BYTES],
            incarnation: [4; 32],
            mounted: true,
            stopping: false,
            closed: false,
            active_operations: 1,
            nodes: 1,
            handles: 1,
            cookies: 1,
            consumer_accounted_bytes: 4096,
            projection: [3; PROJECTION_CLASSES],
            upstream_calls: 2,
        },
        generation: 2,
        revision: 5,
        dirty_inodes: 2,
        submission: Some(WorkspaceSubmissionWire {
            generation: 1,
            captured_revision: 0,
            dirty_inodes: 3,
            phase: WorkspaceStagePhase::Failed,
            inode: Some(1),
            saved_files: 2,
            saved_metadata: 1,
            stage_token: Some(1),
            candidate_root: Some([2; 32]),
            failure: Some(WorkspaceStageFailureDisposition::KnownStageLocalFailure),
            failure_phase: Some(WorkspaceStagePhase::LocalBookkeeping),
            commit: Some(WorkspaceCommitStatusWire {
                phase: WorkspaceCommitPhase::Reconcile,
                known_root: Some([2; 32]),
                known_head: Some(id(0x12)),
                installed_revision: Some(5),
                failure: Some(WorkspaceCommitFailureDisposition::KnownCommitLocalFailure),
            }),
        }),
    }
}
fn roundtrip(response: Response) -> Vec<u8> {
    let bytes = encode_response(&response).unwrap();
    assert_eq!(decode_response(&bytes).unwrap(), response);
    for end in 0..bytes.len() {
        assert!(decode_response(&bytes[..end]).is_err(), "{end}");
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(decode_response(&trailing).is_err());
    bytes
}

#[test]
fn commit_request_has_its_own_budget_and_never_store_authority() {
    let r = request(false);
    assert_eq!(r.operation.opcode(), 14);
    assert_eq!(r.operation.label(), "WorkspaceCommit");
    assert!(r.operation.mutation());
    assert!(!r.operation.read_only());
    assert!(!r.operation.content_mutation());
    assert!(!r.operation.metadata_mutation());
    assert_eq!(r.operation.input_length().unwrap(), 0);
    assert_eq!(permission_bit(14), None);
    assert_eq!(WORKSPACE_COMMIT_MAX_MS, MAX_OPERATION_MS);
    let bytes = encode_request(&r).unwrap();
    assert_eq!(bytes.len(), WORKSPACE_COMMIT_REQUEST_BYTES);
    assert_eq!(bytes[26], 14);
    assert_eq!(decode_request(r.id, &bytes).unwrap(), r);
    for end in 0..bytes.len() {
        assert!(decode_request(1, &bytes[..end]).is_err());
    }
    let mut oversized = bytes;
    oversized.push(0);
    assert_eq!(
        decode_request(1, &oversized).unwrap_err().code,
        Code::Capacity
    );
    for field in 0..7 {
        let mut invalid = r.clone();
        match field {
            0 => invalid.id = 0,
            1 => invalid.generation = 1,
            2 => invalid.store = 1,
            3 => invalid.response_bytes = 1,
            4 => invalid.deadline_ms = 0,
            5 => invalid.deadline_ms = WORKSPACE_COMMIT_MAX_MS + 1,
            _ => invalid.profile = HISTORY_PROFILE,
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
        (b"../bad".to_vec(), [4; 32]),
        (b"valid".to_vec(), [0; 32]),
    ] {
        let mut invalid = r.clone();
        invalid.operation = Operation::WorkspaceCommit {
            workspace,
            incarnation,
        };
        assert_eq!(invalid.validate().unwrap_err().code, Code::InvalidInput);
    }
    let mut status = request(true);
    status.deadline_ms = MAX_OPERATION_MS;
    assert_eq!(status.validate().unwrap_err().code, Code::InvalidInput);
    assert_eq!(
        decode_request(1, &encode_request_with_budget(&r, 10_000).unwrap())
            .unwrap()
            .deadline_ms,
        10_000
    );
    assert!(encode_request_failure(&r, &failure().cause).is_err());
}

#[test]
fn commit_terminal_preserves_full_known_and_observed_records_with_bounded_bytes() {
    assert_eq!(
        roundtrip(Response::WorkspaceCommit(Box::new(completed()))).len(),
        274
    );
    let response = Response::WorkspaceCommit(Box::new(result(WorkspaceCommitOutcome::Failed(
        Box::new(failure()),
    ))));
    let bytes = roundtrip(response);
    assert_eq!(bytes[0], 17);
    assert_eq!(bytes.len(), WORKSPACE_COMMIT_RESULT_BYTES);
    for (offset, byte) in [
        (98, 2),
        (107, 0),
        (107, 6),
        (108, 0),
        (108, 4),
        (111, 18),
        (112, 2),
        (593, 2),
        (936, 2),
        (1279, 2),
        (1430, 2),
        (1581, 2),
        (594 + 40, 0),
        (937 + 40, 0),
        (1280 + 1, 0),
        (1431 + 1, 0),
    ] {
        let mut invalid = bytes.clone();
        invalid[offset] = byte;
        assert!(decode_response(&invalid).is_err(), "offset {offset}");
    }
    let mut invalid = bytes.clone();
    invalid[99..107].fill(0);
    assert!(decode_response(&invalid).is_err());
    let mut oversized = bytes;
    oversized.push(0);
    assert_eq!(
        decode_response(&oversized).unwrap_err().code,
        Code::Capacity
    );
    let mut unknown = failure();
    unknown.known_stage = None;
    unknown.known_outcome = None;
    unknown.installed_revision = None;
    unknown.disposition = WorkspaceCommitFailureDisposition::Unknown;
    unknown.cause = Code::InvalidInput.into();
    // The service reply can be syntactically sound but not match the capture.
    unknown.observed_stage.as_mut().unwrap().workspace = [9; 32];
    roundtrip(Response::WorkspaceCommit(Box::new(result(
        WorkspaceCommitOutcome::Failed(Box::new(unknown.clone())),
    ))));
    unknown.disposition = WorkspaceCommitFailureDisposition::KnownBeforeCommit;
    assert!(
        result(WorkspaceCommitOutcome::Failed(Box::new(unknown.clone())))
            .validate()
            .is_err()
    );
    unknown.observed_outcome = None;
    roundtrip(Response::WorkspaceCommit(Box::new(result(
        WorkspaceCommitOutcome::Failed(Box::new(unknown)),
    ))));
    let mut mismatch = failure();
    mismatch.observed_outcome = Some(CommitOutcomeWire::UpToDate {
        head: None,
        root: [2; 32],
    });
    assert!(result(WorkspaceCommitOutcome::Failed(Box::new(mismatch)))
        .validate()
        .is_err());
    let mut missing = failure();
    missing.known_outcome = None;
    assert!(result(WorkspaceCommitOutcome::Failed(Box::new(missing)))
        .validate()
        .is_err());
    let mut wrong_stage = failure();
    wrong_stage.known_stage.as_mut().unwrap().workspace = [9; 32];
    assert!(
        result(WorkspaceCommitOutcome::Failed(Box::new(wrong_stage)))
            .validate()
            .is_err()
    );
    for outcome in [
        CommitOutcomeWire::UpToDate {
            head: None,
            root: [2; 32],
        },
        CommitOutcomeWire::UpToDate {
            head: Some(id(0x12)),
            root: [2; 32],
        },
    ] {
        let reply = result(WorkspaceCommitOutcome::Completed(
            WorkspaceCommitReportWire {
                generation: 1,
                stage_token: None,
                outcome: outcome.clone(),
                revision: 2,
            },
        ));
        roundtrip(Response::WorkspaceCommit(Box::new(reply)));
        roundtrip(Response::History(Box::new(HistoryResult::Committed(
            outcome,
        ))));
    }
}

#[test]
fn writable_status_preserves_submission_and_reachable_partial_commit_state() {
    let value = status();
    let bytes = roundtrip(Response::WorkspaceWritableStatus(Box::new(value.clone())));
    assert_eq!(bytes[0], 18);
    assert_eq!(bytes.len(), WORKSPACE_WRITABLE_STATUS_RESULT_BYTES);
    // Offsets inside the status payload are the same; everything after the
    // status shifts by the 80 bytes the bounded projection counts added.
    let status_delta = (PROJECTION_CLASSES + 1) * 8;
    let shifted = |offset: usize| {
        if offset < 139 {
            offset
        } else {
            offset + status_delta
        }
    };
    for (offset, byte) in [
        (98, 8),
        (163, 2),
        (188, 0),
        (188, 8),
        (189, 2),
        (202, 2),
        (211, 2),
        (244, 4),
        (245, 8),
        (246, 2),
        (247, 0),
        (247, 6),
        (248, 2),
        (281, 2),
        (282, 0),
        (315, 2),
        (324, 4),
    ] {
        let offset = shifted(offset);
        let mut invalid = bytes.clone();
        invalid[offset] = byte;
        assert!(decode_response(&invalid).is_err(), "offset {offset}");
    }
    for start in [139, 164, 190, 203] {
        let start = shifted(start);
        let mut invalid = bytes.clone();
        invalid[start..start + 8].fill(0);
        assert!(decode_response(&invalid).is_err(), "offset {start}");
    }
    let mut oversized = bytes;
    oversized.push(0);
    assert_eq!(
        decode_response(&oversized).unwrap_err().code,
        Code::Capacity
    );
    for phase in [
        WorkspaceCommitPhase::Preparing,
        WorkspaceCommitPhase::CommitStaged,
        WorkspaceCommitPhase::CompositeCommit,
        WorkspaceCommitPhase::Reconcile,
        WorkspaceCommitPhase::Complete,
    ] {
        for disposition in [
            None,
            Some(WorkspaceCommitFailureDisposition::KnownBeforeCommit),
            Some(WorkspaceCommitFailureDisposition::Unknown),
            Some(WorkspaceCommitFailureDisposition::KnownCommitLocalFailure),
        ] {
            let mut partial = value.clone();
            // Poison fallback may preserve failure classification without known roots.
            partial.submission.as_mut().unwrap().commit = Some(WorkspaceCommitStatusWire {
                phase,
                known_root: None,
                known_head: None,
                installed_revision: None,
                failure: disposition,
            });
            roundtrip(Response::WorkspaceWritableStatus(Box::new(partial)));
        }
    }
    for phase in [
        WorkspaceStagePhase::Captured,
        WorkspaceStagePhase::FileSave,
        WorkspaceStagePhase::MetadataSave,
        WorkspaceStagePhase::StageChanges,
        WorkspaceStagePhase::Staged,
        WorkspaceStagePhase::Failed,
        WorkspaceStagePhase::LocalBookkeeping,
    ] {
        for disposition in [
            None,
            Some(WorkspaceStageFailureDisposition::KnownBeforeStage),
            Some(WorkspaceStageFailureDisposition::Unknown),
            Some(WorkspaceStageFailureDisposition::KnownStageLocalFailure),
        ] {
            let mut partial = value.clone();
            let submission = partial.submission.as_mut().unwrap();
            submission.phase = phase;
            submission.failure = disposition;
            submission.failure_phase = disposition.map(|_| phase);
            submission.inode = None;
            submission.stage_token = None;
            submission.candidate_root = None;
            submission.commit = None;
            roundtrip(Response::WorkspaceWritableStatus(Box::new(partial)));
        }
    }
    let mut idle = value.clone();
    idle.submission = None;
    roundtrip(Response::WorkspaceWritableStatus(Box::new(idle.clone())));
    idle.status.closed = true;
    idle.status.mounted = false;
    idle.status.active_operations = 0;
    idle.status.nodes = 0;
    idle.status.handles = 0;
    idle.status.cookies = 0;
    assert!(idle.validate().is_err());
    idle.dirty_inodes = 0;
    roundtrip(Response::WorkspaceWritableStatus(Box::new(idle)));
    let mut wrong_generation = value.clone();
    wrong_generation.submission.as_mut().unwrap().generation = 2;
    assert!(wrong_generation.validate().is_err());
    let mut wrong_revision = value.clone();
    wrong_revision
        .submission
        .as_mut()
        .unwrap()
        .captured_revision = value.revision;
    assert!(wrong_revision.validate().is_err());
    let mut broken_pair = value;
    broken_pair.submission.as_mut().unwrap().failure_phase = None;
    assert!(broken_pair.validate().is_err());
}

#[derive(Clone, Copy, Debug)]
enum Scenario {
    Completed,
    Failed,
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
fn authenticated_commit_and_writable_status_correlate_and_never_replay() {
    for status_request in [false, true] {
        for scenario in [
            Scenario::Completed,
            Scenario::Failed,
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
            terminal(status_request, scenario);
        }
    }
}
fn terminal(status_request: bool, scenario: Scenario) {
    let mut r = request(status_request);
    r.deadline_ms = 5000;
    let reply = if status_request {
        Response::WorkspaceWritableStatus(Box::new(status()))
    } else if matches!(scenario, Scenario::Failed) {
        Response::WorkspaceCommit(Box::new(result(WorkspaceCommitOutcome::Failed(Box::new(
            failure(),
        )))))
    } else {
        Response::WorkspaceCommit(Box::new(completed()))
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
            let mut bytes = encode_response(&reply).unwrap();
            match scenario {
                Scenario::WrongWorkspace => bytes[3] = b'x',
                Scenario::WrongIncarnation => bytes[66] = 9,
                Scenario::WrongReply => {
                    bytes = if status_request {
                        encode_response(&Response::WorkspaceCommit(Box::new(completed()))).unwrap()
                    } else {
                        encode_response(&Response::WorkspaceWritableStatus(Box::new(status())))
                            .unwrap()
                    }
                }
                Scenario::WrongLifecycle(tag) => {
                    bytes = encode_response(&Response::WorkspaceMount(Box::new(
                        WorkspaceLifecycleWire {
                            workspace: vec![b'a'; 63],
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
            Scenario::Completed | Scenario::Failed => assert_eq!(result.unwrap(), reply),
            Scenario::Refused => assert_eq!(result.unwrap_err(), Failure::from(Code::Denied)),
            _ => {
                assert_eq!(
                    result.unwrap_err(),
                    Failure::from(if status_request {
                        Code::Io
                    } else {
                        Code::Unknown
                    }),
                    "{status_request}/{scenario:?}"
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
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "{status_request}/{scenario:?}"
    );
}
