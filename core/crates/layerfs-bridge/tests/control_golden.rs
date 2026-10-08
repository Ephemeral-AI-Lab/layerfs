//! Pre-R6 control encodings pinned as literal bytes: request tags 1 to 10 and reply tags 1 to 13.
//! Every expected vector is written by hand from the encoder's field order; the
//! encoder never produces its own expectation. A later additive tag or value
//! must leave each of these vectors unchanged.
use layerfs_bridge::control::*;
use layerfs_content::ObjectId;
use layerfs_history::{
    error::MovedState, BranchId, BranchRecord, BranchSnapshot, CommitHistoryRequest, CommitId,
    CommitRecord, CommitStagedOutcome, ForkRequest, ForkSource, HistoryName, LayerId, LayerStackId,
    PageResult, WorkspaceId,
};
const CALL: &[u8] = b"LFSC\x01";
const ANSWER: &[u8] = b"LFSA\x01";
/// One nonzero correlation and its big-endian bytes.
const ID: u64 = 0x0102_0304_0506_0708;
const ID_BYTES: &[u8] = &[1, 2, 3, 4, 5, 6, 7, 8];
const WORKSPACE: [u8; 32] = [0x03; 32];
/// Namespace 0x0A0B as a big-endian signed 64-bit value.
const NAMESPACE: &[u8] = &[0, 0, 0, 0, 0, 0, 0x0A, 0x0B];
/// History identities: one tag byte (Branch 0x11, Commit 0x12, LayerStack 0x31,
/// Layer 0x32), then the body.
const BRANCH: [u8; 17] = tagged(0x11, 0xB1);
const OTHER_BRANCH: [u8; 17] = tagged(0x11, 0xB2);
const STACK: [u8; 17] = tagged(0x31, 0x5C);
const LAYER: [u8; 33] = tagged(0x32, 0x1A);
const OTHER_LAYER: [u8; 33] = tagged(0x32, 0x1B);
const COMMIT: [u8; 33] = tagged(0x12, 0xC0);
const PARENT: [u8; 33] = tagged(0x12, 0xC1);
const HEAD_ROOT: [u8; 32] = [0xA1; 32];
const BASE_ROOT: [u8; 32] = [0xA2; 32];
const EFFECTIVE_ROOT: [u8; 32] = [0xA3; 32];
const SCOPE: [u8; 32] = [0xA4; 32];
const PROFILE: [u8; 32] = [0xA5; 32];
const LOCAL_ROOT: [u8; 32] = [0xA6; 32];
const INSTANCE: [u8; 32] = [0x1D; 32];
const CURSOR: [u8; 160] = [0x05; 160];
const fn tagged<const N: usize>(tag: u8, body: u8) -> [u8; N] {
    let mut bytes = [body; N];
    bytes[0] = tag;
    bytes
}
fn join(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}
/// Magic, correlation, then `body`, whose first byte is the request tag.
fn call(request: Request, body: &[&[u8]]) {
    let expected = join(&[CALL, ID_BYTES, &join(body)]);
    let call = Call { id: ID, request };
    assert_eq!(call.encode().unwrap(), expected, "{call:?}");
    assert_eq!(Call::decode(&expected).unwrap(), call);
}
/// Magic, correlation, then `body`, whose first byte is the reply tag.
fn answer(reply: Reply, body: &[&[u8]]) {
    let expected = join(&[ANSWER, ID_BYTES, &join(body)]);
    let answer = Answer { id: ID, reply };
    assert_eq!(answer.encode().unwrap(), expected, "{answer:?}");
    assert_eq!(Answer::decode(&expected).unwrap(), answer);
}
fn workspace() -> WorkspaceId {
    WorkspaceId::from_authority(WORKSPACE).unwrap()
}
fn branch(bytes: [u8; 17]) -> BranchId {
    BranchId::from_bytes(bytes).unwrap()
}
fn stack() -> LayerStackId {
    LayerStackId::from_bytes(STACK).unwrap()
}
fn layer(bytes: [u8; 33]) -> LayerId {
    LayerId::from_bytes(bytes).unwrap()
}
fn commit(bytes: [u8; 33]) -> CommitId {
    CommitId::from_bytes(bytes).unwrap()
}
fn object(bytes: [u8; 32]) -> ObjectId {
    ObjectId::from_bytes(&bytes).unwrap()
}
fn token() -> WorkspaceToken {
    WorkspaceToken {
        workspace: workspace(),
        namespace: 0x0A0B,
    }
}
/// Workspace incarnation, then the namespace.
fn token_bytes() -> Vec<u8> {
    join(&[&WORKSPACE, NAMESPACE])
}
fn binding(head: bool) -> BranchSnapshot {
    BranchSnapshot {
        branch: BranchRecord {
            id: branch(BRANCH),
            stack: stack(),
            name: HistoryName::new("main").unwrap(),
            base_layer: layer(LAYER),
            head_commit: head.then(|| commit(COMMIT)),
        },
        head_root: head.then(|| object(HEAD_ROOT)),
        base_root: object(BASE_ROOT),
        effective_root: object(EFFECTIVE_ROOT),
        scope: object(SCOPE),
        profile: object(PROFILE),
    }
}
/// Branch, stack, length-prefixed name, base Layer, optional head Commit,
/// optional head root, then base root, effective root, scope and profile.
fn binding_bytes(head: bool) -> Vec<u8> {
    let head = if head {
        join(&[&[1], &COMMIT, &[1], &HEAD_ROOT])
    } else {
        vec![0, 0]
    };
    join(&[
        &BRANCH,
        &STACK,
        &[0, 4],
        b"main",
        &LAYER,
        &head,
        &BASE_ROOT,
        &EFFECTIVE_ROOT,
        &SCOPE,
        &PROFILE,
    ])
}
fn record() -> CommitRecord {
    CommitRecord {
        id: commit(COMMIT),
        stack: stack(),
        root: object(EFFECTIVE_ROOT),
        parent: Some(commit(PARENT)),
        base_layer: layer(LAYER),
    }
}
/// Commit, stack, root, optional parent, base Layer.
fn record_bytes() -> Vec<u8> {
    join(&[&COMMIT, &STACK, &EFFECTIVE_ROOT, &[1], &PARENT, &LAYER])
}
fn first_record() -> CommitRecord {
    CommitRecord {
        id: commit(PARENT),
        stack: stack(),
        root: object(BASE_ROOT),
        parent: None,
        base_layer: layer(LAYER),
    }
}
fn first_record_bytes() -> Vec<u8> {
    join(&[&PARENT, &STACK, &BASE_ROOT, &[0], &LAYER])
}
fn refusal(code: ControlCode, phase: &str, detail: &str) -> ControlRefusal {
    ControlRefusal {
        code,
        phase: phase.into(),
        moved: None,
        published: None,
        detail: detail.into(),
    }
}
fn status(activity: Activity, native: Option<NativeStatus>) -> WorkspaceStatus {
    WorkspaceStatus {
        token: token(),
        binding: binding(false),
        activity,
        epoch: 0x0113,
        epoch_saturated: true,
        published: None,
        local: Some(LocalObservation {
            revision: 8,
            active: 3,
            captured: Some(2),
            captured_revision: None,
            base_root: LOCAL_ROOT,
            dirty_inodes: 11,
            dirty_directory_entries: 5,
            closed: false,
            base_readers: 1,
        }),
        local_failure: None,
        native,
    }
}
/// The tag 3 body of `status`: token, binding, activity, epoch, saturation,
/// publication flag, engine-observation flag, then the local fields in
/// declaration order with each optional generation behind its own flag.
fn status_bytes(activity: u8) -> Vec<u8> {
    join(&[
        &token_bytes(),
        &binding_bytes(false),
        &[activity],
        &[0, 0, 0, 0, 0, 0, 0x01, 0x13],
        &[1],
        &[0],
        &[1],
        &[0, 0, 0, 0, 0, 0, 0, 8],
        &[0, 0, 0, 0, 0, 0, 0, 3],
        &[1, 0, 0, 0, 0, 0, 0, 0, 2],
        &[0],
        &LOCAL_ROOT,
        &[0, 0, 0, 0, 0, 0, 0, 11],
        &[0, 0, 0, 0, 0, 0, 0, 5],
        &[0],
        &[0, 0, 0, 0, 0, 0, 0, 1],
    ])
}
const RECEIPT: NativeReceipt = NativeReceipt {
    mount: 0x0102,
    root: 7,
    mount_id: 911,
    device_major: 0,
    device_minor: 64,
    abi_major: 7,
    abi_minor: 41,
    offered: u64::MAX - 1,
    selected: (1 << 22) | 1,
    max_write: 131_072,
    max_readahead: 65_536,
    max_background: 12,
    congestion_threshold: 9,
    page_size: 4096,
    loops: 2,
    abort_bound: true,
};
fn ready() -> ReadyMount {
    ReadyMount {
        token: token(),
        directory: "/m/w".into(),
        receipt: RECEIPT,
    }
}
/// Token, length-prefixed directory, then the receipt: three u64 identities,
/// four u32 device and protocol numbers, offered and selected u64, write and
/// readahead u32, background and congestion u16, page size u32, loops u16 and
/// the abort flag.
fn ready_bytes() -> Vec<u8> {
    join(&[
        &token_bytes(),
        &[0, 4],
        b"/m/w",
        &[0, 0, 0, 0, 0, 0, 1, 2],
        &[0, 0, 0, 0, 0, 0, 0, 7],
        &[0, 0, 0, 0, 0, 0, 0x03, 0x8F],
        &[0, 0, 0, 0],
        &[0, 0, 0, 64],
        &[0, 0, 0, 7],
        &[0, 0, 0, 41],
        &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFE],
        &[0, 0, 0, 0, 0, 0x40, 0, 1],
        &[0, 2, 0, 0],
        &[0, 1, 0, 0],
        &[0, 12],
        &[0, 9],
        &[0, 0, 0x10, 0],
        &[0, 2],
        &[1],
    ])
}
const WORK: NativeWork = NativeWork {
    loops_configured: 4,
    loops_entered: 3,
    loops_exited: 2,
    loops_joined: 1,
    received: 5,
    admitted: 16,
    queued: 6,
    running: 7,
    parked: 8,
    retained: 9,
    completed: 10,
    handoffs: 11,
    inline: 12,
    refused: 13,
    terminal: 14,
    unadmitted: 15,
    forget_units: 0x0102_0304_0506_0708,
};
/// Four u16 loop counts, six u32 request gauges, seven u64 counters: 88 bytes.
fn work_bytes() -> Vec<u8> {
    join(&[
        &[0, 4, 0, 3, 0, 2, 0, 1],
        &[0, 0, 0, 5, 0, 0, 0, 16, 0, 0, 0, 6],
        &[0, 0, 0, 7, 0, 0, 0, 8, 0, 0, 0, 9],
        &[0, 0, 0, 0, 0, 0, 0, 10],
        &[0, 0, 0, 0, 0, 0, 0, 11],
        &[0, 0, 0, 0, 0, 0, 0, 12],
        &[0, 0, 0, 0, 0, 0, 0, 13],
        &[0, 0, 0, 0, 0, 0, 0, 14],
        &[0, 0, 0, 0, 0, 0, 0, 15],
        &[1, 2, 3, 4, 5, 6, 7, 8],
    ])
}
fn draining() -> NativeStatus {
    NativeStatus {
        phase: NativePhase::Draining,
        ready: Some(ready()),
        detached: true,
        work: Some(WORK),
    }
}
/// Phase, Ready flag and record, detach flag, counter flag and counters.
fn draining_bytes() -> Vec<u8> {
    join(&[&[5, 1], &ready_bytes(), &[1, 1], &work_bytes()])
}
#[test]
fn every_request_tag_keeps_its_bytes() {
    call(
        Request::Mount {
            workspace: workspace(),
            branch: branch(BRANCH),
        },
        &[&[1], &WORKSPACE, &BRANCH],
    );
    call(Request::Commit(token()), &[&[2], &token_bytes()]);
    call(Request::Status(token()), &[&[3], &token_bytes()]);
    call(Request::Unmount(token()), &[&[4], &token_bytes()]);
    let fork = |source| {
        Request::Fork(ForkRequest {
            stack: stack(),
            branch: branch(OTHER_BRANCH),
            name: HistoryName::new("fork").unwrap(),
            source,
        })
    };
    call(
        fork(ForkSource::Layer(layer(OTHER_LAYER))),
        &[
            &[5],
            &STACK,
            &OTHER_BRANCH,
            &[0, 4],
            b"fork",
            &[1],
            &OTHER_LAYER,
        ],
    );
    call(
        fork(ForkSource::Commit {
            branch: branch(BRANCH),
            commit: commit(COMMIT),
        }),
        &[
            &[5],
            &STACK,
            &OTHER_BRANCH,
            &[0, 4],
            b"fork",
            &[2],
            &BRANCH,
            &COMMIT,
        ],
    );
    // Branch, optional start, optional 160-byte cursor behind its length, limit.
    call(
        Request::History(CommitHistoryRequest {
            branch: branch(BRANCH),
            start: Some(commit(COMMIT)),
            cursor: Some(CURSOR.to_vec()),
            limit: 32,
        }),
        &[
            &[6],
            &BRANCH,
            &[1],
            &COMMIT,
            &[1, 0, 160],
            &CURSOR,
            &[0, 32],
        ],
    );
    call(
        Request::History(CommitHistoryRequest {
            branch: branch(BRANCH),
            start: None,
            cursor: None,
            limit: 1,
        }),
        &[&[6], &BRANCH, &[0], &[0], &[0, 1]],
    );
    call(
        Request::Hello(HelloRequest {
            expected_instance: Some(INSTANCE),
            wait_for_store: true,
        }),
        &[&[7], &[1], &INSTANCE, &[1]],
    );
    call(
        Request::Hello(HelloRequest {
            expected_instance: None,
            wait_for_store: false,
        }),
        &[&[7], &[0], &[0]],
    );
    call(Request::EndSession, &[&[8]]);
    call(Request::Attach(token()), &[&[9], &token_bytes()]);
    call(Request::Locate(workspace()), &[&[10], &WORKSPACE]);
}
#[test]
fn binding_commit_history_and_session_replies_keep_their_bytes() {
    answer(
        Reply::Bound {
            token: token(),
            binding: binding(true),
        },
        &[&[1], &token_bytes(), &binding_bytes(true)],
    );
    answer(
        Reply::Committed(CommitStagedOutcome::Committed(record())),
        &[&[2], &[1], &record_bytes()],
    );
    answer(
        Reply::Committed(CommitStagedOutcome::UpToDate {
            head: None,
            root: object(EFFECTIVE_ROOT),
        }),
        &[&[2], &[2], &[0], &EFFECTIVE_ROOT],
    );
    answer(Reply::Unmounted(token()), &[&[4], &token_bytes()]);
    answer(
        Reply::Forked(binding(false)),
        &[&[5], &binding_bytes(false)],
    );
    // Row count, rows, then the optional continuation behind its length.
    answer(
        Reply::History(PageResult {
            records: vec![record(), first_record()],
            continuation: Some(CURSOR.to_vec()),
        }),
        &[
            &[6],
            &[0, 2],
            &record_bytes(),
            &first_record_bytes(),
            &[1, 0, 160],
            &CURSOR,
        ],
    );
    answer(
        Reply::History(PageResult {
            records: Vec::new(),
            continuation: None,
        }),
        &[&[6], &[0, 0], &[0]],
    );
    // Instance, phase, then both versions as length-prefixed text; absent is empty.
    for (phase, byte) in [
        (DaemonPhase::InstallPending, 1),
        (DaemonPhase::Installing, 2),
        (DaemonPhase::Retained, 3),
    ] {
        answer(
            Reply::Hello(DaemonStatus {
                instance: INSTANCE,
                phase,
                overlay_sqlite: None,
                store_sqlite: None,
            }),
            &[&[8], &INSTANCE, &[byte], &[0, 0], &[0, 0]],
        );
    }
    answer(
        Reply::Hello(DaemonStatus {
            instance: INSTANCE,
            phase: DaemonPhase::ControlReady,
            overlay_sqlite: Some("3.45.1".into()),
            store_sqlite: Some("3.46.0".into()),
        }),
        &[
            &[8],
            &INSTANCE,
            &[4],
            &[0, 6],
            b"3.45.1",
            &[0, 6],
            b"3.46.0",
        ],
    );
    answer(Reply::SessionEnded, &[&[9]]);
}
#[test]
fn every_refusal_code_keeps_its_bytes() {
    // Code, length-prefixed phase, conflict flag, publication flag, detail.
    for (code, byte) in [
        (ControlCode::Busy, 1),
        (ControlCode::Missing, 3),
        (ControlCode::Capacity, 4),
        (ControlCode::Invalid, 5),
        (ControlCode::Failed, 6),
        (ControlCode::Unknown, 7),
    ] {
        answer(
            Reply::Refused(refusal(code, "p", "d")),
            &[&[7], &[byte], &[0, 1], b"p", &[0], &[0], &[0, 1], b"d"],
        );
    }
    // The conflict: optional expected head, optional actual head, both bases.
    answer(
        Reply::Refused(ControlRefusal {
            moved: Some(MovedState {
                expected_head: None,
                actual_head: Some(commit(COMMIT)),
                expected_base: layer(LAYER),
                actual_base: layer(OTHER_LAYER),
            }),
            ..refusal(ControlCode::HeadMoved, "Commit Publish", "moved")
        }),
        &[
            &[7],
            &[2],
            &[0, 14],
            b"Commit Publish",
            &[1],
            &[0],
            &[1],
            &COMMIT,
            &LAYER,
            &OTHER_LAYER,
            &[0],
            &[0, 5],
            b"moved",
        ],
    );
    // A known publication travels behind its flag as the Commit outcome.
    answer(
        Reply::Refused(ControlRefusal {
            published: Some(CommitStagedOutcome::Committed(record())),
            ..refusal(ControlCode::Unknown, "Commit Install", "kept")
        }),
        &[
            &[7],
            &[7],
            &[0, 14],
            b"Commit Install",
            &[0],
            &[1, 1],
            &record_bytes(),
            &[0, 4],
            b"kept",
        ],
    );
}
#[test]
fn status_keeps_its_bytes_at_every_activity_and_native_phase() {
    for (activity, byte) in [
        (Activity::Idle, 1),
        (Activity::Committing, 2),
        (Activity::Closing, 3),
        (Activity::Uncertain, 4),
        (Activity::LocalFailure, 5),
        (Activity::Attaching, 6),
    ] {
        answer(
            Reply::Status(Box::new(status(activity, None))),
            &[&[3], &status_bytes(byte)],
        );
    }
    // An unavailable engine observation: flag 0, then the original refusal.
    answer(
        Reply::Status(Box::new(WorkspaceStatus {
            token: token(),
            binding: binding(true),
            activity: Activity::LocalFailure,
            epoch: 2,
            epoch_saturated: false,
            published: Some(CommitStagedOutcome::UpToDate {
                head: Some(commit(COMMIT)),
                root: object(EFFECTIVE_ROOT),
            }),
            local: None,
            local_failure: Some(refusal(ControlCode::Failed, "status", "gone")),
            native: None,
        })),
        &[
            &[3],
            &token_bytes(),
            &binding_bytes(true),
            &[5],
            &[0, 0, 0, 0, 0, 0, 0, 2],
            &[0],
            &[1, 2, 1],
            &COMMIT,
            &EFFECTIVE_ROOT,
            &[0],
            &[6],
            &[0, 6],
            b"status",
            &[0],
            &[0],
            &[0, 4],
            b"gone",
        ],
    );
    // Tag 13 is the tag 3 body followed by the native block with no flag:
    // phase, Ready flag, detach flag, counter flag.
    for (phase, byte) in [
        (NativePhase::Unattached, 1),
        (NativePhase::Attaching, 2),
        (NativePhase::Ready, 3),
        (NativePhase::Probing, 4),
        (NativePhase::Draining, 5),
        (NativePhase::Retained, 6),
    ] {
        let native = NativeStatus {
            phase,
            ready: None,
            detached: false,
            work: None,
        };
        answer(
            Reply::Status(Box::new(status(Activity::Idle, Some(native)))),
            &[&[13], &status_bytes(1), &[byte, 0, 0, 0]],
        );
    }
    answer(
        Reply::Status(Box::new(status(Activity::Closing, Some(draining())))),
        &[&[13], &status_bytes(3), &draining_bytes()],
    );
}
#[test]
fn ready_located_and_retained_keep_their_bytes() {
    answer(Reply::Ready(Box::new(ready())), &[&[10], &ready_bytes()]);
    // Tag 11 is the tag 3 body, one flag, then the native block when present.
    answer(
        Reply::Located(Box::new(status(Activity::Idle, None))),
        &[&[11], &status_bytes(1), &[0]],
    );
    answer(
        Reply::Located(Box::new(status(Activity::Closing, Some(draining())))),
        &[&[11], &status_bytes(3), &[1], &draining_bytes()],
    );
    // Token, stage, detach flag, counter flag, length-prefixed detail.
    for (stage, byte) in [
        (TeardownStage::Detach, 1),
        (TeardownStage::Join, 2),
        (TeardownStage::Owner, 3),
        (TeardownStage::Requests, 4),
        (TeardownStage::Lane, 5),
        (TeardownStage::Revoke, 6),
        (TeardownStage::Close, 7),
        (TeardownStage::Registry, 8),
    ] {
        answer(
            Reply::Retained(Box::new(TeardownCustody {
                token: token(),
                stage,
                detached: false,
                work: None,
                detail: String::new(),
                forced: None,
            })),
            &[&[12], &token_bytes(), &[byte, 0, 0], &[0, 0]],
        );
    }
    answer(
        Reply::Retained(Box::new(TeardownCustody {
            token: token(),
            stage: TeardownStage::Requests,
            detached: true,
            work: Some(WORK),
            detail: "left".into(),
            forced: None,
        })),
        &[
            &[12],
            &token_bytes(),
            &[4, 1, 1],
            &work_bytes(),
            &[0, 4],
            b"left",
        ],
    );
    // Tag 12 carries stages 1 to 8 only.
    for stage in [0, 9] {
        let record = join(&[
            ANSWER,
            ID_BYTES,
            &[12],
            &token_bytes(),
            &[stage, 0, 0],
            &[0, 0],
        ]);
        assert!(Answer::decode(&record).is_err(), "stage {stage}");
    }
}
