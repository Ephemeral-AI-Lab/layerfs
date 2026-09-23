#![cfg(feature = "native")]
//! Codec-level tests for the history wire contract.
//!
//! These exercise the production codecs directly: every closed suboperation and
//! every reply shape round-trips, and every declared bound is checked before a
//! mutation can be reached. The end-to-end route (service + daemon + frames) is
//! driven separately by `layerfs-daemon/tests/history_route.py`.

use layerfs_bridge::{adapters::native::protocol::*, contract::*};

/// One Commit identity with a distinct body and the frozen `0x12` tag.
fn commit_id(body: u8) -> [u8; COMMIT_BYTES] {
    let mut value = [0x12; COMMIT_BYTES];
    value[1] = body;
    value
}

/// One Layer identity with a distinct body and the frozen `0x32` tag.
fn layer_id(body: u8) -> [u8; LAYER_BYTES] {
    let mut value = [0x32; LAYER_BYTES];
    value[1] = body;
    value
}

fn request(operation: Operation) -> Request {
    let profile = match operation {
        Operation::HistoryQuery(_) | Operation::HistoryCommand(_) => HISTORY_PROFILE,
        _ => 1,
    };
    Request {
        id: 1,
        generation: 1,
        store: 1,
        profile,
        deadline_ms: 5_000,
        response_bytes: 4096,
        operation,
    }
}

fn round_trip(operation: Operation) -> Request {
    let original = request(operation);
    let bytes = encode_request_with_budget(&original, 5_000).expect("encode");
    let decoded = decode_request(1, &bytes).expect("decode");
    assert_eq!(decoded, original);
    decoded
}

fn prepared(workspace: [u8; 32], branch: [u8; 17]) -> PreparedChanges {
    PreparedChanges {
        directory_metadata: Vec::new(),
        new_directories: Vec::new(),
        new_file_serials: Vec::new(),
        new_symlink_serials: Vec::new(),
        workspace,
        branch,
        expected_head: Some([0x12; 33]),
        expected_base: layer_id(0x03),
        generation: 9,
        base: [0x44; 32],
        scope: [0x55; 32],
        root_serial: 1,
        directories: vec![DirectoryChange {
            parent: 1,
            changes: vec![(b"a".to_vec(), Some(2)), (b"b".to_vec(), None)],
        }],
        inodes: vec![InodeChange {
            serial: 2,
            kind: 1,
            content: [0x66; 32],
            metadata: [0x77; 32],
        }],
    }
}

fn manifest() -> Vec<ManifestEntry> {
    vec![
        ManifestEntry {
            parent: 0,
            name: Vec::new(),
            kind: 2,
            mode: 0o755,
            mtime_seconds: 1,
            mtime_nanoseconds: 0,
            content: None,
            target: Vec::new(),
        },
        ManifestEntry {
            parent: 0,
            name: b"dir".to_vec(),
            kind: 2,
            mode: 0o700,
            mtime_seconds: 2,
            mtime_nanoseconds: 1,
            content: None,
            target: Vec::new(),
        },
        ManifestEntry {
            parent: 0,
            name: b"file".to_vec(),
            kind: 1,
            mode: 0o644,
            mtime_seconds: 3,
            mtime_nanoseconds: 2,
            content: Some([0x88; 32]),
            target: Vec::new(),
        },
        ManifestEntry {
            parent: 1,
            name: b"nested".to_vec(),
            kind: 1,
            mode: 0o600,
            mtime_seconds: 4,
            mtime_nanoseconds: 3,
            content: Some([0x99; 32]),
            target: Vec::new(),
        },
        ManifestEntry {
            parent: 0,
            name: b"link".to_vec(),
            kind: 3,
            mode: 0o777,
            mtime_seconds: 5,
            mtime_nanoseconds: 4,
            content: None,
            target: b"file".to_vec(),
        },
    ]
}

fn every_query() -> Vec<HistoryQuery> {
    vec![
        HistoryQuery::GetStack { stack: [0x31; 17] },
        HistoryQuery::ListStacks {
            cursor: vec![0xAB; CURSOR_BYTES],
            limit: PAGE_RECORDS,
        },
        HistoryQuery::GetBranch { branch: [0x11; 17] },
        HistoryQuery::ListBranches {
            stack: [0x31; 17],
            cursor: Vec::new(),
            limit: 1,
        },
        HistoryQuery::GetCommit {
            commit: commit_id(0x01),
        },
        HistoryQuery::CommitHistory {
            branch: [0x11; 17],
            start: Some(commit_id(0x02)),
            cursor: vec![0xCD; CURSOR_BYTES],
            limit: 7,
        },
        HistoryQuery::GetLayer {
            layer: layer_id(0x05),
        },
        HistoryQuery::LayerHistory {
            stack: [0x31; 17],
            start: None,
            cursor: Vec::new(),
            limit: 2,
        },
        HistoryQuery::GetStage {
            workspace: [0x01; 32],
        },
        HistoryQuery::ListStages {
            branch: [0x11; 17],
            cursor: vec![0xEF; 8],
            limit: 3,
        },
    ]
}

fn every_command() -> Vec<HistoryCommand> {
    vec![
        HistoryCommand::InitLayerStack {
            stack: [0x51; 16],
            name: b"main".to_vec(),
            scope_seed: [0x02; 32],
            manifest: manifest(),
        },
        HistoryCommand::Fork {
            stack: [0x31; 17],
            branch: [0x61; 16],
            name: b"work".to_vec(),
            source: HistoryForkSource::Layer(layer_id(0x01)),
        },
        HistoryCommand::Fork {
            stack: [0x31; 17],
            branch: [0x62; 16],
            name: b"historical".to_vec(),
            source: HistoryForkSource::Commit {
                branch: [0x11; 17],
                commit: commit_id(0x03),
            },
        },
        HistoryCommand::StageChanges(prepared([0x71; 32], [0x11; 17])),
        HistoryCommand::CommitStaged {
            workspace: [0x72; 32],
            token: 1,
        },
        HistoryCommand::Commit(prepared([0x73; 32], [0x11; 17])),
        HistoryCommand::AddLayer {
            stack: [0x31; 17],
            branch: [0x11; 17],
            commit: [0x12; 33],
            expected_stack_head: layer_id(0x01),
            expected_branch_base: layer_id(0x02),
        },
        HistoryCommand::DiscardStage {
            workspace: [0x74; 32],
            token: 2,
        },
        HistoryCommand::ReserveInodes {
            scope: [0x03; 32],
            count: 65_536,
        },
    ]
}

#[test]
fn every_query_and_command_round_trips() {
    for query in every_query() {
        let label = format!("{query:?}");
        let operation = Operation::HistoryQuery(query);
        let original = request(operation);
        let bytes = encode_request_with_budget(&original, 5_000)
            .unwrap_or_else(|error| panic!("encode query {label}: {error:?}"));
        assert_eq!(decode_request(1, &bytes).expect("decode query"), original);
    }
    for command in every_command() {
        let label = format!("{command:?}");
        let operation = Operation::HistoryCommand(command);
        let original = request(operation);
        let bytes = encode_request_with_budget(&original, 5_000)
            .unwrap_or_else(|error| panic!("encode command {label}: {error:?}"));
        assert_eq!(decode_request(1, &bytes).expect("decode command"), original);
    }
}

#[test]
fn profile_and_opcode_must_agree() {
    // A history operation on the legacy profile, and a legacy operation on the
    // history profile, are both refused before any mutation.
    let mut legacy = request(Operation::HistoryQuery(HistoryQuery::GetStack {
        stack: [0x31; 17],
    }));
    legacy.profile = 1;
    assert_eq!(
        encode_request_with_budget(&legacy, 5_000).unwrap_err().code,
        Code::Unsupported
    );
    let mut history = request(Operation::Inspect {
        root: [0; 32],
        query: Inspect::File,
    });
    history.profile = HISTORY_PROFILE;
    assert_eq!(
        encode_request_with_budget(&history, 5_000)
            .unwrap_err()
            .code,
        Code::Unsupported
    );
}

#[test]
fn unknown_suboperations_are_refused_before_mutation() {
    for opcode in [QUERY_OPCODE, COMMAND_OPCODE] {
        let mut bytes = vec![0u8; 28];
        bytes[0..8].copy_from_slice(&1u64.to_be_bytes());
        bytes[8..12].copy_from_slice(&1u32.to_be_bytes());
        bytes[12..14].copy_from_slice(&HISTORY_PROFILE.to_be_bytes());
        bytes[14..18].copy_from_slice(&5_000u32.to_be_bytes());
        bytes[18..26].copy_from_slice(&4096u64.to_be_bytes());
        bytes[26] = opcode;
        bytes[27] = 0x7F;
        assert_eq!(
            decode_request(1, &bytes).unwrap_err().code,
            Code::Unsupported
        );
    }
    // An unknown Fork source is refused for the same reason.
    let fork = every_command()
        .into_iter()
        .find(|command| matches!(command, HistoryCommand::Fork { .. }))
        .expect("a fork command");
    let mut bytes =
        encode_request_with_budget(&request(Operation::HistoryCommand(fork)), 5_000).unwrap();
    // The Fork source tag is the byte after the name blob; a value outside the
    // closed set is refused rather than reinterpreted.
    let length = bytes.len();
    bytes[length - 34] = 9;
    assert_eq!(
        decode_request(1, &bytes).unwrap_err().code,
        Code::Unsupported
    );
}

#[test]
fn identity_widths_and_tags_are_checked() {
    let wrong_width = HistoryQuery::GetStack { stack: [0x31; 17] };
    let mut bytes =
        encode_request_with_budget(&request(Operation::HistoryQuery(wrong_width)), 5_000).unwrap();
    bytes.truncate(bytes.len() - 1);
    assert!(decode_request(1, &bytes).is_err());
    // A tag byte from another identity class is an invalid input, not a guess.
    let cases = [
        Operation::HistoryQuery(HistoryQuery::GetStack { stack: [0x11; 17] }),
        Operation::HistoryQuery(HistoryQuery::GetBranch { branch: [0x31; 17] }),
        Operation::HistoryQuery(HistoryQuery::GetCommit {
            commit: layer_id(0x06),
        }),
        Operation::HistoryQuery(HistoryQuery::GetLayer {
            layer: commit_id(0x04),
        }),
        Operation::HistoryQuery(HistoryQuery::GetStage { workspace: [0; 32] }),
        Operation::HistoryCommand(HistoryCommand::CommitStaged {
            workspace: [0x72; 32],
            token: 0,
        }),
        Operation::HistoryCommand(HistoryCommand::ReserveInodes {
            scope: [0x03; 32],
            count: 0,
        }),
        Operation::HistoryCommand(HistoryCommand::ReserveInodes {
            scope: [0x03; 32],
            count: i64::MAX as u64 + 1,
        }),
    ];
    for operation in cases {
        let code = encode_request_with_budget(&request(operation), 5_000)
            .unwrap_err()
            .code;
        assert!(
            matches!(code, Code::InvalidInput | Code::Capacity),
            "unexpected {code:?}"
        );
    }
}

#[test]
fn page_and_count_bounds_are_checked() {
    assert_eq!(
        encode_request_with_budget(
            &request(Operation::HistoryQuery(HistoryQuery::ListStacks {
                cursor: Vec::new(),
                limit: 0,
            })),
            5_000
        )
        .unwrap_err()
        .code,
        Code::InvalidInput
    );
    assert_eq!(
        encode_request_with_budget(
            &request(Operation::HistoryQuery(HistoryQuery::ListStacks {
                cursor: Vec::new(),
                limit: PAGE_RECORDS + 1,
            })),
            5_000
        )
        .unwrap_err()
        .code,
        Code::Capacity
    );
    assert_eq!(
        encode_request_with_budget(
            &request(Operation::HistoryQuery(HistoryQuery::ListStacks {
                cursor: vec![0; CURSOR_BYTES + 1],
                limit: 1,
            })),
            5_000
        )
        .unwrap_err()
        .code,
        Code::Capacity
    );
    let mut too_many = manifest();
    let root = too_many[0].clone();
    too_many = vec![root];
    for index in 0..1_500 {
        too_many.push(ManifestEntry {
            parent: 0,
            name: format!("n{index}").into_bytes(),
            kind: 2,
            mode: 0o755,
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
            content: None,
            target: Vec::new(),
        });
    }
    assert_eq!(too_many.len(), 1_501);
    round_trip(Operation::HistoryCommand(HistoryCommand::InitLayerStack {
        stack: [0x51; 16],
        name: b"main".to_vec(),
        scope_seed: [0x02; 32],
        manifest: too_many[..130].to_vec(),
    }));
    assert_eq!(
        encode_request_with_budget(
            &request(Operation::HistoryCommand(HistoryCommand::InitLayerStack {
                stack: [0x51; 16],
                name: b"main".to_vec(),
                scope_seed: [0x02; 32],
                manifest: too_many,
            })),
            5_000
        )
        .unwrap_err()
        .code,
        Code::Capacity
    );
}

#[test]
fn a_manifest_is_a_tree_by_construction() {
    let build = |entries: Vec<ManifestEntry>| {
        encode_request_with_budget(
            &request(Operation::HistoryCommand(HistoryCommand::InitLayerStack {
                stack: [0x51; 16],
                name: b"main".to_vec(),
                scope_seed: [0x02; 32],
                manifest: entries,
            })),
            5_000,
        )
        .unwrap_err()
        .code
    };
    // A root that is not a directory, or that carries content, is refused.
    let mut wrong_root = manifest();
    wrong_root[0].kind = 1;
    assert_eq!(build(wrong_root), Code::InvalidInput);
    let mut content_root = manifest();
    content_root[0].content = Some([0x01; 32]);
    assert_eq!(build(content_root), Code::InvalidInput);
    // A forward or self parent would make the manifest cyclic.
    let mut forward = manifest();
    forward[1].parent = 3;
    assert_eq!(build(forward), Code::InvalidInput);
    // Two entries may not claim the same name under one parent.
    let mut duplicate = manifest();
    duplicate[1].name = b"file".to_vec();
    assert_eq!(build(duplicate), Code::InvalidInput);
    // A regular file needs a published root and a symlink needs a target.
    let mut file_without_root = manifest();
    file_without_root[2].content = None;
    assert_eq!(build(file_without_root), Code::InvalidInput);
    let mut link_without_target = manifest();
    link_without_target[4].target = Vec::new();
    assert_eq!(build(link_without_target), Code::InvalidInput);
    let mut link_with_content = manifest();
    link_with_content[4].content = Some([0x01; 32]);
    assert_eq!(build(link_with_content), Code::InvalidInput);
    // Portable modes are checked against the kind they belong to.
    let mut bad_mode = manifest();
    bad_mode[1].mode = 0o10000;
    assert_eq!(build(bad_mode), Code::InvalidInput);
    let mut bad_link_mode = manifest();
    bad_link_mode[4].mode = 0o755;
    assert_eq!(build(bad_link_mode), Code::InvalidInput);
    let mut bad_nanos = manifest();
    bad_nanos[1].mtime_nanoseconds = 1_000_000_000;
    assert_eq!(build(bad_nanos), Code::InvalidInput);
    // A manifest that is legal still round-trips through the decoder.
    round_trip(Operation::HistoryCommand(HistoryCommand::InitLayerStack {
        stack: [0x51; 16],
        name: b"main".to_vec(),
        scope_seed: [0x02; 32],
        manifest: manifest(),
    }));
}

fn stack_wire() -> StackWire {
    StackWire {
        stack: [0x31; 17],
        name: vec![b'a'; NAME_MAX_BYTES],
        scope: [0x01; 32],
        profile: [0x02; 32],
        head_layer: [0x32; 33],
    }
}

fn branch_wire() -> BranchWire {
    BranchWire {
        branch: [0x11; 17],
        stack: [0x31; 17],
        name: vec![b'b'; NAME_MAX_BYTES],
        base_layer: [0x32; 33],
        head_commit: Some([0x12; 33]),
    }
}

fn commit_wire() -> CommitWire {
    CommitWire {
        commit: commit_id(0x06),
        stack: [0x31; 17],
        root: [0x03; 32],
        parent: Some(commit_id(0x07)),
        base_layer: layer_id(0x09),
    }
}

fn layer_wire() -> LayerWire {
    LayerWire {
        layer: layer_id(0x0A),
        stack: [0x31; 17],
        parent: Some(layer_id(0x0B)),
        root: [0x04; 32],
        source_branch: Some([0x11; 17]),
        source_commit: Some(commit_id(0x08)),
    }
}

fn stage_wire() -> StageWire {
    StageWire {
        workspace: [0x05; 32],
        token: 7,
        stack: [0x31; 17],
        branch: [0x11; 17],
        expected_head: Some(commit_id(0x09)),
        expected_base: layer_id(0x0D),
        expected_root: [0x06; 32],
        construction_base_root: [0x06; 32],
        intended_commit_base: layer_id(0x0D),
        candidate_root: [0x08; 32],
        profile: [0x02; 32],
        scope: [0x01; 32],
        generation: 3,
    }
}

fn every_result() -> Vec<HistoryResult> {
    vec![
        HistoryResult::Stack(stack_wire()),
        HistoryResult::Stacks {
            continuation: vec![0xAA; CURSOR_BYTES],
            records: vec![stack_wire(), stack_wire()],
        },
        HistoryResult::BranchSnapshot(BranchSnapshotWire {
            branch: branch_wire(),
            head_root: Some([0x09; 32]),
            base_root: [0x0A; 32],
            effective_root: [0x09; 32],
            root_serial: Some(1),
            scope: [0x01; 32],
            profile: [0x02; 32],
        }),
        HistoryResult::Branches {
            continuation: Vec::new(),
            records: vec![branch_wire()],
        },
        HistoryResult::Commit(commit_wire()),
        HistoryResult::Commits {
            continuation: Vec::new(),
            records: vec![commit_wire()],
        },
        HistoryResult::Layer(layer_wire()),
        HistoryResult::Layers {
            continuation: Vec::new(),
            records: vec![layer_wire()],
        },
        HistoryResult::Stage(stage_wire()),
        HistoryResult::Stages {
            continuation: Vec::new(),
            records: vec![stage_wire()],
        },
        HistoryResult::StackCreated(StackCreatedWire {
            stack: stack_wire(),
            root: [1; 32],
            root_serial: 1,
        }),
        HistoryResult::Committed(CommitOutcomeWire::Committed(commit_wire())),
        HistoryResult::Committed(CommitOutcomeWire::UpToDate {
            head: Some(commit_id(0x0A)),
            root: [0x0C; 32],
        }),
        HistoryResult::Committed(CommitOutcomeWire::UpToDate {
            head: None,
            root: [0x0D; 32],
        }),
        HistoryResult::Published(LayerOutcomeWire::Added(layer_wire())),
        HistoryResult::Published(LayerOutcomeWire::UpToDate {
            layer: layer_id(0x07),
        }),
        HistoryResult::Published(LayerOutcomeWire::NoChanges {
            head: layer_id(0x08),
        }),
        HistoryResult::Discarded { removed: true },
        HistoryResult::Discarded { removed: false },
        HistoryResult::Reservation {
            scope: [0x01; 32],
            start: 1,
            count: 65_536,
        },
    ]
}

#[test]
fn every_reply_round_trips() {
    for result in every_result() {
        let response = Response::History(Box::new(result));
        let bytes = encode_response(&response).expect("encode");
        assert_eq!(decode_response(&bytes).expect("decode"), response);
    }
}

#[test]
fn a_reply_tag_outside_the_closed_union_is_refused() {
    assert_eq!(
        decode_response(&[0xFF]).unwrap_err().code,
        Code::Unsupported
    );
    assert_eq!(
        decode_response(&[0x09]).unwrap_err().code,
        Code::InvalidInput
    );
    assert_eq!(
        decode_response(&[0x08, 0x7F]).unwrap_err().code,
        Code::Unsupported
    );
    // A truncated history record is not silently accepted.
    let bytes = encode_response(&Response::History(Box::new(HistoryResult::Stage(
        stage_wire(),
    ))))
    .unwrap();
    assert!(decode_response(&bytes[..bytes.len() - 1]).is_err());
}

#[test]
fn failure_codes_round_trip_and_stay_typed() {
    for code in [
        Code::Busy,
        Code::NotFound,
        Code::HeadMoved,
        Code::StageChanged,
        Code::ContinuityUnavailable,
    ] {
        let failure = Failure::from(code);
        assert_eq!(
            decode_failure(&encode_failure(failure.clone())).unwrap(),
            failure
        );
    }
    assert_eq!(Code::Busy as u8, 13);
    assert_eq!(Code::NotFound as u8, 14);
    assert_eq!(Code::HeadMoved as u8, 15);
    assert_eq!(Code::StageChanged as u8, 16);
    assert_eq!(Code::ContinuityUnavailable as u8, 17);
}

#[test]
fn permission_bits_are_total_and_legacy_mask_grants_nothing() {
    assert_eq!(permission_bit(1), Some(1 << 0));
    assert_eq!(permission_bit(5), Some(1 << 4));
    assert_eq!(permission_bit(QUERY_OPCODE), Some(1 << 5));
    assert_eq!(permission_bit(COMMAND_OPCODE), Some(1 << 6));
    assert_eq!(permission_bit(0), None);
    assert_eq!(permission_bit(8), None);
    assert_eq!(permission_bit(255), None);
    const LEGACY_MASK: u8 = 31;
    assert_eq!(LEGACY_MASK & (1 << 5), 0);
    assert_eq!(LEGACY_MASK & (1 << 6), 0);
}

#[test]
fn classification_is_exhaustive_and_semantic() {
    for query in every_query() {
        let operation = Operation::HistoryQuery(query);
        assert!(operation.read_only());
        assert!(!operation.content_mutation());
        assert!(!operation.metadata_mutation());
        assert!(!operation.mutation());
        assert_eq!(operation.opcode(), QUERY_OPCODE);
    }
    for command in every_command() {
        let content = matches!(
            command,
            HistoryCommand::InitLayerStack { .. }
                | HistoryCommand::StageChanges(_)
                | HistoryCommand::Commit(_)
        );
        let operation = Operation::HistoryCommand(command);
        assert!(!operation.read_only());
        assert_eq!(operation.content_mutation(), content);
        assert_eq!(operation.metadata_mutation(), !content);
        assert!(operation.mutation());
        assert_eq!(operation.opcode(), COMMAND_OPCODE);
    }
    for operation in [
        Operation::ConstructFile { length: 1 },
        Operation::EditFile {
            root: [0; 32],
            base_length: 1,
            edits: Vec::new(),
        },
        Operation::UpdatePreparedFilesystem {
            directory_metadata: Vec::new(),
            new_directories: Vec::new(),
            new_file_serials: Vec::new(),
            new_symlink_serials: Vec::new(),
            base: [0; 32],
            scope: [0; 32],
            root_serial: 1,
            directories: Vec::new(),
            inodes: Vec::new(),
        },
    ] {
        assert!(operation.content_mutation());
        assert!(!operation.metadata_mutation());
    }
    for operation in [
        Operation::ReadFile {
            root: [0; 32],
            start: 0,
            end: 0,
        },
        Operation::Inspect {
            root: [0; 32],
            query: Inspect::File,
        },
    ] {
        assert!(operation.read_only());
        assert!(!operation.mutation());
    }
}

#[test]
fn minimum_and_maximum_record_encodings_match_the_frozen_widths() {
    let mut small_stack = stack_wire();
    small_stack.name = b"a".to_vec();
    let mut small_branch = branch_wire();
    small_branch.name = b"a".to_vec();
    small_branch.head_commit = None;
    let mut small_commit = commit_wire();
    small_commit.parent = None;
    let mut genesis = layer_wire();
    genesis.parent = None;
    genesis.source_branch = None;
    genesis.source_commit = None;
    let mut small_stage = stage_wire();
    small_stage.expected_head = None;
    for (result, width) in [
        (
            HistoryResult::Stacks {
                continuation: vec![],
                records: vec![small_stack],
            },
            117,
        ),
        (
            HistoryResult::Stacks {
                continuation: vec![],
                records: vec![stack_wire()],
            },
            179,
        ),
        (
            HistoryResult::Branches {
                continuation: vec![],
                records: vec![small_branch],
            },
            71,
        ),
        (
            HistoryResult::Branches {
                continuation: vec![],
                records: vec![branch_wire()],
            },
            166,
        ),
        (
            HistoryResult::Commits {
                continuation: vec![],
                records: vec![small_commit],
            },
            116,
        ),
        (
            HistoryResult::Commits {
                continuation: vec![],
                records: vec![commit_wire()],
            },
            149,
        ),
        (
            HistoryResult::Layers {
                continuation: vec![],
                records: vec![genesis],
            },
            85,
        ),
        (
            HistoryResult::Layers {
                continuation: vec![],
                records: vec![layer_wire()],
            },
            168,
        ),
        (
            HistoryResult::Stages {
                continuation: vec![],
                records: vec![small_stage],
            },
            309,
        ),
        (
            HistoryResult::Stages {
                continuation: vec![],
                records: vec![stage_wire()],
            },
            342,
        ),
    ] {
        let response = Response::History(Box::new(result));
        let bytes = encode_response(&response).unwrap();
        assert_eq!(bytes.len(), width + 6);
        assert_eq!(decode_response(&bytes).unwrap(), response);
        assert!(decode_response(&bytes[..bytes.len() - 1]).is_err());
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(decode_response(&trailing).is_err());
        let mut count = bytes;
        count[4..6].copy_from_slice(&129u16.to_be_bytes());
        assert!(decode_response(&count).is_err());
    }
}

#[test]
fn history_page_budget_includes_tags_counts_and_continuation() {
    let mut records = vec![stack_wire(); 91];
    // 91*179=16289; choose 89 continuation bytes: 6+89+16289=16384.
    let response = Response::History(Box::new(HistoryResult::Stacks {
        continuation: vec![1; 89],
        records: records.clone(),
    }));
    let encoded = encode_response(&response).unwrap();
    assert_eq!(encoded.len(), HISTORY_RESULT_BYTES);
    assert_eq!(decode_response(&encoded).unwrap(), response);
    assert!(
        encode_response(&Response::History(Box::new(HistoryResult::Stacks {
            continuation: vec![1; 90],
            records: records.clone()
        })))
        .is_err()
    );
    let mut oversized = encoded;
    oversized.push(0);
    assert_eq!(
        decode_response(&oversized).unwrap_err().code,
        Code::Capacity
    );
    records.resize(128, stack_wire());
    assert_eq!(
        encode_response(&Response::History(Box::new(HistoryResult::Stacks {
            continuation: vec![],
            records
        })))
        .unwrap_err()
        .code,
        Code::Capacity
    );
    assert!(
        encode_response(&Response::History(Box::new(HistoryResult::Branches {
            continuation: vec![1; 161],
            records: vec![]
        })))
        .is_err()
    );
}

#[test]
fn history_failure_context_round_trips_without_changing_legacy_frames() {
    use layerfs_bridge::adapters::native::protocol::{
        decode_request_failure, encode_request_failure,
    };
    let request = request(Operation::HistoryCommand(HistoryCommand::CommitStaged {
        workspace: [5; 32],
        token: 7,
    }));
    for conflict in [
        HistoryConflict::BranchMoved {
            expected_head: None,
            actual_head: Some(commit_id(1)),
            expected_base: layer_id(1),
            actual_base: layer_id(2),
        },
        HistoryConflict::StackMoved {
            expected: layer_id(1),
            actual: layer_id(2),
        },
        HistoryConflict::StageChanged {
            expected: 7,
            actual: Some(8),
        },
        HistoryConflict::BaseMismatch {
            commit_base: layer_id(1),
            branch_base: layer_id(2),
        },
    ] {
        let code = if matches!(conflict, HistoryConflict::StageChanged { .. }) {
            Code::StageChanged
        } else {
            Code::HeadMoved
        };
        let failure = Failure {
            code,
            unknown: false,
            cleanup: None,
            history: Some(Box::new(HistoryFailure {
                conflict: Some(conflict),
                stage: StageObservation::Retained(Box::new(stage_wire())),
            })),
        };
        let bytes = encode_request_failure(&request, &failure).unwrap();
        assert_eq!(decode_request_failure(&request, &bytes).unwrap(), failure);
        assert!(decode_failure(&bytes).is_err());
        assert!(decode_request_failure(&request, &bytes[..3]).is_err());
        let mut trailing = bytes;
        trailing.push(0);
        assert!(decode_request_failure(&request, &trailing).is_err());
    }
    for stage in [
        StageObservation::Absent([1; 32]),
        StageObservation::AcknowledgedUnknown(Box::new(stage_wire())),
        StageObservation::Unobserved,
    ] {
        let mut failure = Failure::from(Code::Busy);
        if stage != StageObservation::Unobserved {
            failure.history = Some(Box::new(HistoryFailure {
                conflict: None,
                stage,
            }));
        }
        assert_eq!(
            decode_request_failure(
                &request,
                &encode_request_failure(&request, &failure).unwrap()
            )
            .unwrap(),
            failure
        );
    }
    let mut legacy = request;
    legacy.profile = 1;
    legacy.operation = Operation::ConstructFile { length: 0 };
    for code in [Code::InvalidInput, Code::Integrity, Code::Unknown] {
        let failure = Failure::from(code);
        let bytes = encode_request_failure(&legacy, &failure).unwrap();
        assert_eq!(bytes, vec![code as u8, u8::from(code == Code::Unknown), 0]);
        assert_eq!(decode_request_failure(&legacy, &bytes).unwrap(), failure);
    }
}

#[test]
fn invalid_terminal_reservation_and_descriptor_serial_are_refused() {
    let reservation = Response::History(Box::new(HistoryResult::Reservation {
        scope: [1; 32],
        start: 1,
        count: 1,
    }));
    let mut bytes = encode_response(&reservation).unwrap();
    bytes[34..42].copy_from_slice(&(i64::MAX as u64).to_be_bytes());
    assert!(decode_response(&bytes).is_err());
    assert!(
        encode_response(&Response::History(Box::new(HistoryResult::Reservation {
            scope: [1; 32],
            start: i64::MAX as u64,
            count: 1
        })))
        .is_err()
    );
    assert!(
        encode_response(&Response::History(Box::new(HistoryResult::StackCreated(
            StackCreatedWire {
                stack: stack_wire(),
                root: [1; 32],
                root_serial: 0
            }
        ))))
        .is_err()
    );
}

fn history_wire_fixtures() -> Vec<Vec<u8>> {
    let mut cases: Vec<_> = every_query()
        .into_iter()
        .map(Operation::HistoryQuery)
        .chain(every_command().into_iter().map(Operation::HistoryCommand))
        .map(|operation| encode_request(&request(operation)).unwrap())
        .collect();
    cases.extend(
        every_result()
            .into_iter()
            .map(|result| encode_response(&Response::History(Box::new(result))).unwrap()),
    );
    let request = request(Operation::HistoryCommand(HistoryCommand::CommitStaged {
        workspace: [5; 32],
        token: 7,
    }));
    for conflict in [
        HistoryConflict::BranchMoved {
            expected_head: None,
            actual_head: Some(commit_id(1)),
            expected_base: layer_id(1),
            actual_base: layer_id(2),
        },
        HistoryConflict::StackMoved {
            expected: layer_id(1),
            actual: layer_id(2),
        },
        HistoryConflict::StageChanged {
            expected: 7,
            actual: Some(8),
        },
        HistoryConflict::BaseMismatch {
            commit_base: layer_id(1),
            branch_base: layer_id(2),
        },
    ] {
        let code = if matches!(conflict, HistoryConflict::StageChanged { .. }) {
            Code::StageChanged
        } else {
            Code::HeadMoved
        };
        let failure = Failure {
            code,
            unknown: false,
            cleanup: None,
            history: Some(Box::new(HistoryFailure {
                conflict: Some(conflict),
                stage: StageObservation::Retained(Box::new(stage_wire())),
            })),
        };
        cases.push(encode_request_failure(&request, &failure).unwrap());
    }
    for stage in [
        StageObservation::Unobserved,
        StageObservation::Absent([5; 32]),
        StageObservation::AcknowledgedUnknown(Box::new(stage_wire())),
    ] {
        let mut failure = Failure::from(Code::Busy);
        failure.history = Some(Box::new(HistoryFailure {
            conflict: None,
            stage,
        }));
        cases.push(encode_request_failure(&request, &failure).unwrap());
    }
    cases
}

#[test]
fn history_wire_bytes_match_pre_simplification_head() {
    use std::fmt::Write;
    let expected: Vec<_> = include_str!("fixtures/history-wire-a4a144af.hex")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect();
    let actual = history_wire_fixtures();
    assert_eq!(actual.len(), expected.len());
    for (index, (bytes, expected)) in actual.iter().zip(expected).enumerate() {
        let mut hex = String::new();
        for byte in bytes {
            write!(&mut hex, "{byte:02x}").unwrap();
        }
        assert_eq!(hex, expected, "wire case {index}");
    }
}
