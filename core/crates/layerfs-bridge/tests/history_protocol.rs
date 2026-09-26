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
        HistoryCommand::ImportNativeDirectory {
            stack: [0x51; 16],
            name: b"main".to_vec(),
            scope_seed: [0x02; 32],
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
fn native_import_declares_progress_response_budget() {
    let mut value = request(Operation::HistoryCommand(
        HistoryCommand::ImportNativeDirectory {
            stack: [0x52; 16],
            name: b"native".to_vec(),
            scope_seed: [0x04; 32],
        },
    ));
    value.response_bytes = 0;
    assert_eq!(encode_request(&value).unwrap_err().code, Code::Capacity);
    value.response_bytes = u64::from(value.deadline_ms.div_ceil(1_000));
    let encoded = encode_request(&value).unwrap();
    assert_eq!(decode_request(value.id, &encoded).unwrap(), value);
    assert!(value.operation.content_mutation());
    assert!(!value.operation.metadata_mutation());
}

#[test]
fn prepared_frame_bytes_matches_the_encoder_for_every_trailer() {
    // The prepared-update admission compares one figure against the metadata
    // frame budget, so the figure has to be the encoder's own output rather
    // than an estimate over the caller's counts: an over-estimate refuses a
    // request the frame carries, an under-estimate fails inside the encoder
    // after the caller already accepted the request.
    let measure = |label: &str, changes: &PreparedChanges| {
        let encoded = request(Operation::HistoryCommand(HistoryCommand::Commit(
            changes.clone(),
        )));
        if let Err(failure) = encoded.validate() {
            panic!("validate {label}: {failure:?}");
        }
        let bytes = encode_request_with_budget(&encoded, 5_000)
            .unwrap_or_else(|failure| panic!("encode {label}: {failure:?}"));
        assert_eq!(
            changes.frame_bytes().expect("frame bytes"),
            bytes.len(),
            "declared frame bytes must be the encoded length"
        );
    };
    let binding = |parent: u64, names: &[(&[u8], Option<u64>)]| DirectoryChange {
        parent,
        changes: names
            .iter()
            .map(|(name, serial)| (name.to_vec(), *serial))
            .collect(),
    };
    let inode = |serial: u64, kind: u8| InodeChange {
        serial,
        kind,
        content: [0x66; 32],
        metadata: [0x77; 32],
    };
    // One complete update: two directory rows, one declared fresh directory,
    // one patched directory, a fresh file and a fresh symlink.
    let full = PreparedChanges {
        workspace: [0x71; 32],
        branch: [0x11; 17],
        expected_head: Some(commit_id(0x05)),
        expected_base: layer_id(0x03),
        generation: 9,
        base: [0x44; 32],
        scope: [0x55; 32],
        root_serial: 1,
        directories: vec![
            binding(1, &[(b"a".as_slice(), Some(2)), (b"z".as_slice(), None)]),
            binding(5, &[(b"c".as_slice(), Some(2))]),
        ],
        inodes: vec![inode(2, 1), inode(3, 3)],
        new_directories: vec![DirectoryMetadata {
            serial: 5,
            mode: 0o755,
            mtime_seconds: -1,
            mtime_nanoseconds: 12,
        }],
        directory_metadata: vec![DirectoryMetadata {
            serial: 6,
            mode: 0o700,
            mtime_seconds: 1,
            mtime_nanoseconds: 0,
        }],
        new_file_serials: vec![2],
        new_symlink_serials: vec![3],
    };
    measure("full", &full);
    // Every trailer version: files only, directory records only, and none.
    let files_only = PreparedChanges {
        new_symlink_serials: Vec::new(),
        ..full.clone()
    };
    measure("files_only", &files_only);
    let records_only = PreparedChanges {
        new_file_serials: Vec::new(),
        ..files_only.clone()
    };
    measure("records_only", &records_only);
    let no_trailer = PreparedChanges {
        new_directories: Vec::new(),
        directory_metadata: Vec::new(),
        ..records_only.clone()
    };
    measure("no_trailer", &no_trailer);
    // An absent expected head shortens the fixed part by exactly its width.
    let headless = PreparedChanges {
        expected_head: None,
        ..full.clone()
    };
    measure("headless", &headless);
    assert_eq!(
        full.frame_bytes().unwrap() - headless.frame_bytes().unwrap(),
        COMMIT_BYTES
    );
    // A name's own width is measured rather than assumed: the same update with
    // a 255-byte name and with a one-byte name differs by exactly 254 bytes.
    let named = PreparedChanges {
        directories: vec![binding(1, &[(b"a".as_slice(), Some(2))])],
        new_directories: Vec::new(),
        directory_metadata: Vec::new(),
        new_file_serials: Vec::new(),
        new_symlink_serials: Vec::new(),
        ..full.clone()
    };
    let long_name = PreparedChanges {
        directories: vec![binding(1, &[(vec![b'n'; 255].as_slice(), Some(2))])],
        ..named.clone()
    };
    let short_name = PreparedChanges {
        directories: vec![binding(1, &[(b"n".as_slice(), Some(2))])],
        ..named.clone()
    };
    measure("long_name", &long_name);
    measure("short_name", &short_name);
    assert_eq!(
        long_name.frame_bytes().unwrap() - short_name.frame_bytes().unwrap(),
        254
    );
    // The empty prepared update is still a framed request.
    let empty = PreparedChanges {
        directories: Vec::new(),
        inodes: Vec::new(),
        expected_head: None,
        ..no_trailer
    };
    measure("empty", &empty);
}

#[test]
fn a_prepared_update_wider_than_128_rows_round_trips_and_the_frame_is_the_bound() {
    // A prepared update used to be refused above 128 directories, 128 inodes
    // and 128 names however much the frame could carry. The bound is now the
    // metadata frame the update travels in: a 200-name directory is carried,
    // and the same shape past the frame is refused as `Capacity` by the
    // contract before any encoder sees it.
    let wide = |names: usize| PreparedChanges {
        workspace: [0x71; 32],
        branch: [0x11; 17],
        expected_head: Some(commit_id(0x05)),
        expected_base: layer_id(0x03),
        generation: 9,
        base: [0x44; 32],
        scope: [0x55; 32],
        root_serial: 1,
        directories: vec![DirectoryChange {
            parent: 1,
            changes: (0..names)
                .map(|index| (format!("f{index:04}").into_bytes(), Some(2 + index as u64)))
                .collect(),
        }],
        inodes: (0..names)
            .map(|index| InodeChange {
                serial: 2 + index as u64,
                kind: 1,
                content: [0x66; 32],
                metadata: [0x77; 32],
            })
            .collect(),
        new_directories: Vec::new(),
        directory_metadata: vec![DirectoryMetadata {
            serial: 1_000_000,
            mode: 0o755,
            mtime_seconds: -1,
            mtime_nanoseconds: 7,
        }],
        new_file_serials: (0..names).map(|index| 2 + index as u64).collect(),
        new_symlink_serials: Vec::new(),
    };
    let carried = wide(200);
    let fitting = request(Operation::HistoryCommand(HistoryCommand::Commit(
        carried.clone(),
    )));
    fitting.validate().expect("200 names fit one metadata frame");
    let bytes = encode_request_with_budget(&fitting, 5_000).expect("encode");
    assert_eq!(carried.frame_bytes().unwrap(), bytes.len());
    assert!(bytes.len() <= METADATA_BYTES);
    assert_eq!(decode_request(1, &bytes).expect("decode"), fitting);
    // Past the frame the same shape is refused, and the refusal is a resource
    // refusal rather than a malformed request.
    let past = wide(400);
    let oversized = request(Operation::HistoryCommand(HistoryCommand::Commit(
        past.clone(),
    )));
    assert!(past.frame_bytes().unwrap() > METADATA_BYTES);
    let refusal = oversized.validate().expect_err("past the frame");
    assert_eq!(refusal.code, Code::Capacity);
    assert!(encode_request_with_budget(&oversized, 5_000).is_err());
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
    // A prepared update whose rows are legal but whose encoded metadata
    // exceeds the 32 KiB envelope is refused as a resource refusal by the
    // contract itself, before any encoder: the admission is the frame the
    // update travels in rather than a row count. The same update used to be
    // admitted here and refused only inside the encoder.
    let widest = vec![b'x'; 255];
    let mut changes = prepared([0x71; 32], [0x11; 17]);
    changes.directories = [1_u64, 2]
        .into_iter()
        .map(|parent| DirectoryChange {
            parent,
            changes: (0..64)
                .map(|_| (widest.clone(), Some(2)))
                .collect::<Vec<_>>(),
        })
        .collect();
    assert_eq!(
        changes
            .directories
            .iter()
            .map(|directory| directory.changes.len())
            .sum::<usize>(),
        128
    );
    assert!(changes.frame_bytes().unwrap() > METADATA_BYTES);
    let request = request(Operation::HistoryCommand(HistoryCommand::StageChanges(
        changes,
    )));
    assert_eq!(request.validate().unwrap_err().code, Code::Capacity);
    assert_eq!(
        encode_request_with_budget(&request, 5_000)
            .unwrap_err()
            .code,
        Code::Capacity
    );
}

#[test]
fn the_retired_pathless_init_tag_is_refused_and_unassigned() {
    // The retired route declared a stack body, a bounded name, a scope seed, a
    // 16-bit entry count and bounded entries. This fixture keeps that shape.
    let name = b"main";
    let mut body = vec![0x51; 16];
    body.extend_from_slice(&(name.len() as u16).to_be_bytes());
    body.extend_from_slice(name);
    body.extend_from_slice(&[0x02; 32]);
    body.extend_from_slice(&2_u16.to_be_bytes());
    body.extend_from_slice(&[0_u8; 48]);
    let encoded = encode_request(&request(Operation::HistoryCommand(
        HistoryCommand::ImportNativeDirectory {
            stack: [0x51; 16],
            name: name.to_vec(),
            scope_seed: [0x02; 32],
        },
    )))
    .expect("encode");
    // Patch the suboperation tag to the retired value, keeping the production
    // envelope so the refusal comes from the tag and not from a malformed body.
    // The current route's payload opens with its stack body.
    let payload = &body[..52];
    let tag = encoded
        .windows(payload.len())
        .position(|window| window == payload)
        .expect("the declared payload must appear in the encoded request")
        - 1;
    assert_eq!(encoded[tag], 9, "the tag precedes the payload");
    let mut retired = encoded[..tag + 1].to_vec();
    retired[tag] = 1;
    retired.extend_from_slice(body.get(52..).unwrap_or_default());
    let failure = decode_request(1, &retired).unwrap_err();
    assert_eq!(failure.code, Code::Unsupported);
    // The current route still round-trips at the widest legal name, so the only
    // thing refused above is the retired tag itself.
    let widest = HistoryCommand::ImportNativeDirectory {
        stack: [0x51; 16],
        name: vec![b'n'; NAME_MAX_BYTES],
        scope_seed: [0x02; 32],
    };
    round_trip(Operation::HistoryCommand(widest));
    // Every assigned history-command tag is above the retired one. The tag is
    // the byte between the envelope and the payload, and the native-import
    // command's payload opens with its stack body.
    for command in every_command() {
        let body = match &command {
            HistoryCommand::ImportNativeDirectory { stack, .. } => stack.to_vec(),
            _ => continue,
        };
        let encoded =
            encode_request_with_budget(&request(Operation::HistoryCommand(command)), 5_000)
                .unwrap();
        let tag = encoded
            .windows(body.len())
            .position(|window| window == body)
            .expect("declared stack body")
            - 1;
        assert!(
            encoded[tag] > 1,
            "tag {} is not above the retired one",
            encoded[tag]
        );
    }
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
    assert_eq!(permission_bit(SAVE_FILE_OPCODE), Some(1 << 3));
    assert_eq!(permission_bit(5), None);
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
            HistoryCommand::ImportNativeDirectory { .. }
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
    let operation = Operation::SaveFile {
        base: Some([1; 32]),
        base_length: 1,
        length: 1,
        extents: 1,
        replacement: 0,
    };
    assert!(operation.content_mutation());
    assert!(!operation.metadata_mutation());
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
    legacy.operation = Operation::SaveFile {
        base: None,
        base_length: 0,
        length: 0,
        extents: 0,
        replacement: 0,
    };
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

/// The retired pathless initialization owns tag 1, so its frozen encoding is the
/// only recorded case with no live counterpart. Its own bytes prove which case
/// that is: a `01` tag directly after the command envelope.
const HISTORY_ENVELOPE_HEX: &str = "000000000000000100000001000200001388000000000000100007";

#[test]
fn the_retired_pathless_init_encoding_is_the_only_recorded_case_without_a_live_match() {
    use std::fmt::Write;
    let recorded: Vec<&str> = include_str!("fixtures/history-wire-a4a144af.hex")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect();
    let live: Vec<String> = history_wire_fixtures()
        .iter()
        .map(|bytes| {
            let mut hex = String::new();
            for byte in bytes {
                write!(&mut hex, "{byte:02x}").unwrap();
            }
            hex
        })
        .collect();
    // The frozen bytes are evidence and stay exactly as recorded. Every one of
    // them must still be reproduced by the production encoder except the case
    // whose operation this change deleted.
    let mut missing = Vec::new();
    for (index, line) in recorded.iter().enumerate() {
        if !live.iter().any(|hex| hex == line) {
            missing.push(index);
        }
    }
    assert_eq!(
        missing.len(),
        1,
        "unexpected drifted encodings: {missing:?}"
    );
    let retired = recorded[missing[0]];
    let tag = retired
        .find(HISTORY_ENVELOPE_HEX)
        .expect("history-command envelope")
        + HISTORY_ENVELOPE_HEX.len();
    assert_eq!(
        &retired[tag..tag + 2],
        "01",
        "the only drifted encoding must be the retired pathless initialization"
    );
}
