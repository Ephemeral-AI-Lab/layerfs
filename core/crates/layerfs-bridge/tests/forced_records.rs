//! Forced-teardown records are additive: exact bytes, bounds and refusals of request tag 11,
//! reply tags 14 and 15, native phase 7 and teardown stage 9.
use layerfs_bridge::control::*;
use layerfs_content::ObjectId;
use layerfs_history::{
    BranchId, BranchRecord, BranchSnapshot, CommitId, CommitRecord, CommitStagedOutcome,
    HistoryName, LayerId, LayerStackId, WorkspaceId,
};
const CALL: &[u8] = b"LFSC\x01";
const ANSWER: &[u8] = b"LFSA\x01";
const ID: u64 = 0x0102_0304_0506_0708;
const ID_BYTES: &[u8] = &[1, 2, 3, 4, 5, 6, 7, 8];
/// Five magic bytes and the eight-byte correlation precede the tag.
const TAG: usize = 13;
const RECORD_LIMIT: usize = 8192;
const WORKSPACE: [u8; 32] = [0x03; 32];
const NAMESPACE: &[u8] = &[0, 0, 0, 0, 0, 0, 0x0A, 0x0B];
const STACK: [u8; 17] = tagged(0x31, 0x5C);
const LAYER: [u8; 33] = tagged(0x32, 0x1A);
const COMMIT: [u8; 33] = tagged(0x12, 0xC0);
const PARENT: [u8; 33] = tagged(0x12, 0xC1);
const ROOT: [u8; 32] = [0xA3; 32];
/// Terminal replies 0x0102 as a big-endian unsigned 64-bit value.
const FENCED: &[u8] = &[0, 0, 0, 0, 0, 0, 1, 2];
const fn tagged<const N: usize>(tag: u8, body: u8) -> [u8; N] {
    let mut bytes = [body; N];
    bytes[0] = tag;
    bytes
}
fn join(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}
fn answer_bytes(body: &[&[u8]]) -> Vec<u8> {
    join(&[ANSWER, ID_BYTES, &join(body)])
}
fn refused(body: &[&[u8]]) -> bool {
    Answer::decode(&answer_bytes(body)).is_err()
}
/// Exact encoding and decoding; every proper prefix and one more byte are refused.
fn exact(reply: Reply, body: &[&[u8]]) {
    let expected = answer_bytes(body);
    let answer = Answer { id: ID, reply };
    assert_eq!(answer.encode().unwrap(), expected, "{answer:?}");
    assert_eq!(Answer::decode(&expected).unwrap(), answer);
    for end in 0..expected.len() {
        assert!(Answer::decode(&expected[..end]).is_err(), "prefix {end}");
    }
    let mut extra = expected;
    extra.push(0);
    assert!(Answer::decode(&extra).is_err());
}
fn token() -> WorkspaceToken {
    WorkspaceToken {
        workspace: WorkspaceId::from_authority(WORKSPACE).unwrap(),
        namespace: 0x0A0B,
    }
}
fn token_bytes() -> Vec<u8> {
    join(&[&WORKSPACE, NAMESPACE])
}
fn record() -> CommitRecord {
    CommitRecord {
        id: CommitId::from_bytes(COMMIT).unwrap(),
        stack: LayerStackId::from_bytes(STACK).unwrap(),
        root: ObjectId::from_bytes(&ROOT).unwrap(),
        parent: Some(CommitId::from_bytes(PARENT).unwrap()),
        base_layer: LayerId::from_bytes(LAYER).unwrap(),
    }
}
/// Commit, stack, root, present parent, base Layer: 149 bytes.
fn record_bytes() -> Vec<u8> {
    join(&[&COMMIT, &STACK, &ROOT, &[1], &PARENT, &LAYER])
}
const WORK: NativeWork = NativeWork {
    loops_configured: 2,
    loops_entered: 0,
    loops_exited: 0,
    loops_joined: 0,
    received: 0,
    admitted: 0,
    queued: 0,
    running: 0,
    parked: 0,
    retained: 0,
    completed: 0,
    handoffs: 0,
    inline: 0,
    refused: 0,
    terminal: 0,
    unadmitted: 0,
    forget_units: 9,
};
/// The first u16 and the last u64 of the 88-byte counter block.
fn work_bytes() -> Vec<u8> {
    join(&[&[0, 2], &[0; 78], &[0, 0, 0, 0, 0, 0, 0, 9]])
}
fn facts(
    abort: AbortDisposition,
    detach: DetachDisposition,
    commit: CommitKnowledge,
) -> ForcedFacts {
    ForcedFacts {
        abort,
        detach,
        commit,
        fenced: 0x0102,
    }
}
fn receipt(facts: ForcedFacts, cleanup: ForcedCleanup) -> Reply {
    Reply::ForceUnmounted(Box::new(ForceUnmounted {
        token: token(),
        outcome: ForcedOutcome {
            facts,
            work: WORK,
            cleanup,
        },
    }))
}
fn custody(stage: TeardownStage, forced: Option<ForcedFacts>) -> TeardownCustody {
    TeardownCustody {
        token: token(),
        stage,
        detached: true,
        work: Some(WORK),
        detail: "left".into(),
        forced,
    }
}
/// The tag 12 body: token, stage, detach flag, counter flag and counters, detail.
fn custody_bytes(stage: u8) -> Vec<u8> {
    join(&[
        &token_bytes(),
        &[stage, 1, 1],
        &work_bytes(),
        &[0, 4],
        b"left",
    ])
}
#[test]
fn the_force_request_is_tag_11_with_one_strict_flag() {
    for (relinquish_unknown, byte) in [(false, 0), (true, 1)] {
        let expected = join(&[CALL, ID_BYTES, &[11], &token_bytes(), &[byte]]);
        let call = Call {
            id: ID,
            request: Request::ForceUnmount {
                token: token(),
                relinquish_unknown,
            },
        };
        assert_eq!(call.encode().unwrap(), expected);
        assert_eq!(Call::decode(&expected).unwrap(), call);
        assert_eq!(expected.len(), 55);
        for end in 0..expected.len() {
            assert!(Call::decode(&expected[..end]).is_err(), "prefix {end}");
        }
        let mut extra = expected;
        extra.push(0);
        assert!(Call::decode(&extra).is_err());
    }
    for flag in [2, 0xFF] {
        let record = join(&[CALL, ID_BYTES, &[11], &token_bytes(), &[flag]]);
        assert!(Call::decode(&record).is_err(), "flag {flag}");
    }
    // The next request tag stays unknown.
    let unknown = join(&[CALL, ID_BYTES, &[12], &token_bytes(), &[0]]);
    assert!(Call::decode(&unknown).is_err());
    assert!(Call {
        id: ID,
        request: Request::ForceUnmount {
            token: WorkspaceToken {
                namespace: 0,
                ..token()
            },
            relinquish_unknown: true,
        },
    }
    .encode()
    .is_err());
}
#[test]
fn the_forced_receipt_is_tag_14_and_pins_every_value() {
    use {AbortDisposition as Abort, CommitKnowledge as Commit, DetachDisposition as Detach};
    // Token, abort, detach, Commit knowledge, terminal replies, counters, cleanup.
    for (abort, byte) in [(Abort::Written, 1), (Abort::Short, 2), (Abort::Failed, 3)] {
        exact(
            receipt(
                facts(abort, Detach::Detached, Commit::Absent),
                ForcedCleanup::Queued,
            ),
            &[
                &[14],
                &token_bytes(),
                &[byte, 2, 1],
                FENCED,
                &work_bytes(),
                &[2],
            ],
        );
    }
    for (detach, byte) in [
        (Detach::NotAttempted, 1),
        (Detach::Detached, 2),
        (Detach::Busy, 3),
        (Detach::Failed, 4),
    ] {
        exact(
            receipt(
                facts(Abort::Written, detach, Commit::Absent),
                ForcedCleanup::Queued,
            ),
            &[
                &[14],
                &token_bytes(),
                &[1, byte, 1],
                FENCED,
                &work_bytes(),
                &[2],
            ],
        );
    }
    for (cleanup, byte) in [
        (ForcedCleanup::Held, 1),
        (ForcedCleanup::Queued, 2),
        (ForcedCleanup::Gone, 3),
        (ForcedCleanup::Unobserved, 4),
    ] {
        exact(
            receipt(
                facts(Abort::Written, Detach::Detached, Commit::Unknown),
                cleanup,
            ),
            &[
                &[14],
                &token_bytes(),
                &[1, 2, 3],
                FENCED,
                &work_bytes(),
                &[byte],
            ],
        );
    }
    // A known publication is value 2 followed by the existing outcome encoding.
    exact(
        receipt(
            facts(
                Abort::Written,
                Detach::Detached,
                Commit::Published(CommitStagedOutcome::Committed(record())),
            ),
            ForcedCleanup::Held,
        ),
        &[
            &[14],
            &token_bytes(),
            &[1, 2, 2, 1],
            &record_bytes(),
            FENCED,
            &work_bytes(),
            &[1],
        ],
    );
    exact(
        receipt(
            facts(
                Abort::Written,
                Detach::Detached,
                Commit::Published(CommitStagedOutcome::UpToDate {
                    head: None,
                    root: ObjectId::from_bytes(&ROOT).unwrap(),
                }),
            ),
            ForcedCleanup::Gone,
        ),
        &[
            &[14],
            &token_bytes(),
            &[1, 2, 2, 2, 0],
            &ROOT,
            FENCED,
            &work_bytes(),
            &[3],
        ],
    );
    // Zero and the first unassigned value of each byte are refused.
    for [abort, detach, commit, cleanup] in [
        [0, 2, 1, 2],
        [4, 2, 1, 2],
        [1, 0, 1, 2],
        [1, 5, 1, 2],
        [1, 2, 0, 2],
        [1, 2, 4, 2],
        [1, 2, 1, 0],
        [1, 2, 1, 5],
    ] {
        assert!(
            refused(&[
                &[14],
                &token_bytes(),
                &[abort, detach, commit],
                FENCED,
                &work_bytes(),
                &[cleanup],
            ]),
            "{abort} {detach} {commit} {cleanup}"
        );
    }
    // An unknown outcome inside a known publication, and the next reply tag.
    assert!(refused(&[
        &[14],
        &token_bytes(),
        &[1, 2, 2, 3],
        FENCED,
        &work_bytes(),
        &[2],
    ]));
    assert!(refused(&[&[16], &token_bytes()]));
    assert!(refused(&[&[0]]));
}
#[test]
fn retained_facts_travel_under_tag_15_and_tag_12_keeps_its_bytes() {
    use {AbortDisposition as Abort, CommitKnowledge as Commit, DetachDisposition as Detach};
    let plain = answer_bytes(&[&[12], &custody_bytes(4)]);
    exact(
        Reply::Retained(Box::new(custody(TeardownStage::Requests, None))),
        &[&[12], &custody_bytes(4)],
    );
    // The same body under tag 15, then abort, detach, knowledge and terminal replies.
    let held = facts(
        Abort::Written,
        Detach::Busy,
        Commit::Published(CommitStagedOutcome::Committed(record())),
    );
    let forced = answer_bytes(&[
        &[15],
        &custody_bytes(4),
        &[1, 3, 2, 1],
        &record_bytes(),
        FENCED,
    ]);
    exact(
        Reply::Retained(Box::new(custody(TeardownStage::Requests, Some(held)))),
        &[
            &[15],
            &custody_bytes(4),
            &[1, 3, 2, 1],
            &record_bytes(),
            FENCED,
        ],
    );
    assert_eq!(plain[TAG + 1..], forced[TAG + 1..plain.len()]);
    // Neither record is valid under the other's tag.
    let mut without_facts = plain;
    without_facts[TAG] = 15;
    assert!(Answer::decode(&without_facts).is_err());
    let mut with_facts = forced;
    with_facts[TAG] = 12;
    assert!(Answer::decode(&with_facts).is_err());
    // Tag 15 carries every stage, the Abort stage included.
    for (stage, byte) in [
        (TeardownStage::Detach, 1),
        (TeardownStage::Join, 2),
        (TeardownStage::Owner, 3),
        (TeardownStage::Requests, 4),
        (TeardownStage::Lane, 5),
        (TeardownStage::Revoke, 6),
        (TeardownStage::Close, 7),
        (TeardownStage::Registry, 8),
        (TeardownStage::Abort, 9),
    ] {
        let failed = facts(Abort::Failed, Detach::NotAttempted, Commit::Unknown);
        exact(
            Reply::Retained(Box::new(custody(stage, Some(failed)))),
            &[&[15], &custody_bytes(byte), &[3, 1, 3], FENCED],
        );
    }
    for stage in [0, 10] {
        assert!(
            refused(&[&[15], &custody_bytes(stage), &[3, 1, 3], FENCED]),
            "stage {stage}"
        );
    }
    // The Abort stage has no tag 12 form, in either direction.
    assert!(Answer {
        id: ID,
        reply: Reply::Retained(Box::new(custody(TeardownStage::Abort, None))),
    }
    .encode()
    .is_err());
    assert!(refused(&[&[12], &custody_bytes(9)]));
}
#[test]
fn stopping_is_phase_7_in_every_native_block() {
    let root = ObjectId::from_bytes(&ROOT).unwrap();
    let status = |phase| {
        Box::new(WorkspaceStatus {
            token: token(),
            binding: BranchSnapshot {
                branch: BranchRecord {
                    id: BranchId::from_bytes(tagged(0x11, 0xB1)).unwrap(),
                    stack: LayerStackId::from_bytes(STACK).unwrap(),
                    name: HistoryName::new("main").unwrap(),
                    base_layer: LayerId::from_bytes(LAYER).unwrap(),
                    head_commit: None,
                },
                head_root: None,
                base_root: root,
                effective_root: root,
                scope: root,
                profile: root,
            },
            activity: Activity::Closing,
            epoch: 4,
            epoch_saturated: false,
            published: None,
            local: Some(LocalObservation {
                revision: 1,
                active: 1,
                captured: None,
                captured_revision: None,
                base_root: ROOT,
                dirty_inodes: 0,
                dirty_directory_entries: 0,
                closed: false,
                base_readers: 0,
            }),
            local_failure: None,
            native: Some(NativeStatus {
                phase,
                ready: None,
                detached: false,
                work: None,
            }),
        })
    };
    // Tag 13 (status) and tag 11 (Locate) both end with this block: phase,
    // Ready flag, detach flag, counter flag.
    type Wrap = fn(Box<WorkspaceStatus>) -> Reply;
    let replies: [(u8, Wrap); 2] = [(13, Reply::Status), (11, Reply::Located)];
    for (tag, reply) in replies {
        let encoded = |phase| {
            Answer {
                id: ID,
                reply: reply(status(phase)),
            }
            .encode()
            .unwrap()
        };
        let retained = encoded(NativePhase::Retained);
        let stopping = encoded(NativePhase::Stopping);
        let phase = retained.len() - 4;
        assert_eq!(retained[TAG], tag);
        assert_eq!(retained[phase..], [6, 0, 0, 0]);
        assert_eq!(stopping[phase..], [7, 0, 0, 0]);
        assert_eq!(retained[..phase], stopping[..phase]);
        assert_eq!(
            Answer::decode(&stopping).unwrap().reply,
            reply(status(NativePhase::Stopping))
        );
        for unknown in [0, 8] {
            let mut record = stopping.clone();
            record[phase] = unknown;
            assert!(Answer::decode(&record).is_err(), "phase {unknown}");
        }
    }
}
#[test]
fn the_largest_forced_records_stay_under_the_record_limit() {
    let token = WorkspaceToken {
        namespace: i64::MAX,
        ..token()
    };
    let work = NativeWork {
        loops_configured: u16::MAX,
        loops_entered: u16::MAX,
        loops_exited: u16::MAX,
        loops_joined: u16::MAX,
        received: u32::MAX,
        admitted: u32::MAX,
        queued: u32::MAX,
        running: u32::MAX,
        parked: u32::MAX,
        retained: u32::MAX,
        completed: u64::MAX,
        handoffs: u64::MAX,
        inline: u64::MAX,
        refused: u64::MAX,
        terminal: u64::MAX,
        unadmitted: u64::MAX,
        forget_units: u64::MAX,
    };
    // The longest Commit knowledge is a Commit record with a parent.
    let facts = ForcedFacts {
        abort: AbortDisposition::Failed,
        detach: DetachDisposition::Failed,
        commit: CommitKnowledge::Published(CommitStagedOutcome::Committed(record())),
        fenced: u64::MAX,
    };
    let receipt = Answer {
        id: u64::MAX,
        reply: Reply::ForceUnmounted(Box::new(ForceUnmounted {
            token,
            outcome: ForcedOutcome {
                facts: facts.clone(),
                work,
                cleanup: ForcedCleanup::Unobserved,
            },
        })),
    };
    let bytes = receipt.encode().unwrap();
    // 14 of magic, correlation and tag; token 40; facts 2 + 151 + 8; counters 88; cleanup 1.
    assert_eq!(bytes.len(), 304);
    assert_eq!(Answer::decode(&bytes).unwrap(), receipt);
    let custody = |detail: usize| Answer {
        id: u64::MAX,
        reply: Reply::Retained(Box::new(TeardownCustody {
            token,
            stage: TeardownStage::Abort,
            detached: true,
            work: Some(work),
            detail: "z".repeat(detail),
            forced: Some(facts.clone()),
        })),
    };
    let retained = custody(2048);
    let bytes = retained.encode().unwrap();
    // 14; token 40; stage and detach 2; counters 1 + 88; detail 2 + 2048; facts 161.
    assert_eq!(bytes.len(), 2356);
    assert!(bytes.len() < RECORD_LIMIT);
    assert_eq!(Answer::decode(&bytes).unwrap(), retained);
    for end in 0..bytes.len() {
        assert!(Answer::decode(&bytes[..end]).is_err(), "prefix {end}");
    }
    // One more detail byte is refused before send, as under tag 12.
    assert!(custody(2049).encode().is_err());
}
