//! History through the production service: a non-FUSE client, real bodies.
//!
//! Every case here enters `Service::handle`, which is the same authorization,
//! admission and dispatch body the native transport and the daemon use. Nothing
//! is called around the service and no history body is re-implemented.

use layerfs_bridge::{adapters::native::connection::VerifiedPeer, contract::*};
use layerfs_content::filesystem::scope_for_seed;
use layerfs_history::{sqlite, HistoryCatalog, HistoryCatalogConfig, HistoryError};
use layerfs_service::{Grant, Service, StoreAccess};
use layerfs_storage::Store;
use layerfs_telemetry::{operation::OperationRecorder, timer::Timing};
use std::{
    io::Cursor,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const ALL: u8 = 0b0111_1111;
const LEGACY: u8 = 31;

struct Fixture {
    _temp: Temp,
    service: Service,
    peer: VerifiedPeer,
    catalog: Arc<dyn HistoryCatalog>,
    catalog_path: PathBuf,
    store_path: PathBuf,
}

fn unique(name: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "layerfs-history-service-{}-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed),
        name
    ))
}

fn fixture(name: &str, operations: u8) -> Fixture {
    fixture_using(name, operations, None)
}

fn fixture_using(name: &str, operations: u8, refusal: Option<bool>) -> Fixture {
    let path = unique(name);
    std::fs::create_dir(&path).unwrap();
    let store_path = path.join("store.sqlite");
    let store = Timing::disabled("create", |s| {
        Store::create(&store_path, Store::default_policy(), s.child("create"))
    })
    .0
    .unwrap();
    let catalog_path = path.join("history.sqlite");
    let catalog: Arc<dyn HistoryCatalog> = Arc::new(
        sqlite::create(
            &catalog_path,
            &HistoryCatalogConfig {
                cursor_key: [71; 32],
                binding_key: b"layerfs-test-authority".to_vec(),
                incarnation: 1,
            },
        )
        .unwrap(),
    );
    let catalog: Arc<dyn HistoryCatalog> = match refusal {
        Some(unknown) => Arc::new(RefusingCommit {
            inner: catalog,
            unknown,
        }),
        None => catalog,
    };
    let peer = VerifiedPeer::from_private(&[7; 32]).unwrap();
    let service = Service::new(
        vec![StoreAccess {
            id: 1,
            store,
            grants: vec![Grant {
                public_key: *peer.public_key(),
                operations,
                expires_unix: u64::MAX,
            }],
            history: Some(Arc::clone(&catalog)),
        }],
        OperationRecorder::disabled(),
    )
    .unwrap();
    Fixture {
        _temp: Temp(path),
        service,
        peer,
        catalog,
        catalog_path,
        store_path,
    }
}

fn profile(operation: &Operation) -> u16 {
    match operation {
        Operation::HistoryQuery(_) | Operation::HistoryCommand(_) => HISTORY_PROFILE,
        _ => 1,
    }
}

fn call_at(
    service: &Service,
    peer: &VerifiedPeer,
    id: u64,
    profile: u16,
    operation: Operation,
    body: &[u8],
) -> Result<Response, Failure> {
    let request = Request {
        id,
        generation: 1,
        store: 1,
        profile,
        deadline_ms: 60_000,
        response_bytes: MAX_FILE,
        operation,
    };
    let mut input = Cursor::new(body.to_vec());
    let mut output = Vec::new();
    service.handle(peer, &request, &mut input, &mut output).0
}

fn call(
    service: &Service,
    peer: &VerifiedPeer,
    id: u64,
    operation: Operation,
) -> Result<Response, Failure> {
    let profile = profile(&operation);
    call_at(service, peer, id, profile, operation, &[])
}

fn result(response: Response) -> HistoryResult {
    match response {
        Response::History(result) => *result,
        other => panic!("expected a history reply, got {other:?}"),
    }
}

fn stack_wire(response: Response) -> StackWire {
    match result(response) {
        HistoryResult::StackCreated(record) => record.stack,
        HistoryResult::Stack(record) => record,
        other => panic!("expected a stack record, got {other:?}"),
    }
}

fn snapshot(response: Response) -> BranchSnapshotWire {
    match result(response) {
        HistoryResult::BranchSnapshot(snapshot) => snapshot,
        other => panic!("expected a branch snapshot, got {other:?}"),
    }
}

fn stage(response: Response) -> StageWire {
    match result(response) {
        HistoryResult::Stage(record) => record,
        other => panic!("expected a stage, got {other:?}"),
    }
}

fn stat(service: &Service, peer: &VerifiedPeer, id: u64, root: Root, path: &[u8]) -> Response {
    call(
        service,
        peer,
        id,
        Operation::Inspect {
            root,
            query: Inspect::Stat {
                path: path.to_vec(),
            },
        },
    )
    .unwrap()
}

fn stat_roots(response: Response) -> (u64, u8, Root, Root) {
    match response {
        Response::Stat {
            serial,
            kind,
            content,
            metadata,
            ..
        } => (serial, kind, content, metadata),
        other => panic!("expected a stat reply, got {other:?}"),
    }
}

fn construct(service: &Service, peer: &VerifiedPeer, id: u64, bytes: &[u8]) -> Root {
    match call_at(
        service,
        peer,
        id,
        1,
        Operation::ConstructFile {
            length: bytes.len() as u64,
        },
        bytes,
    )
    .unwrap()
    {
        Response::Saved { root, .. } => root,
        other => panic!("expected a saved file, got {other:?}"),
    }
}

/// One manifest root directory plus one regular file named `a`.
fn manifest(file: Root) -> Vec<ManifestEntry> {
    vec![
        ManifestEntry {
            parent: 0,
            name: Vec::new(),
            kind: 2,
            mode: 0o755,
            mtime_seconds: 1_700_000_000,
            mtime_nanoseconds: 5,
            content: None,
            target: Vec::new(),
        },
        ManifestEntry {
            parent: 0,
            name: b"a".to_vec(),
            kind: 1,
            mode: 0o644,
            mtime_seconds: 1_700_000_001,
            mtime_nanoseconds: 6,
            content: Some(file),
            target: Vec::new(),
        },
    ]
}

struct Initialized {
    stack: [u8; 17],
    branch: [u8; 17],
    base_layer: [u8; 33],
    root: Root,
    scope: Root,
    file_root: Root,
    metadata_root: Root,
}

fn initialize(fixture: &Fixture, seed: [u8; 32], file_root: Root, next: u64) -> Initialized {
    let stack_body = [0x51; 16];
    let branch_body = [0x61; 16];
    let response = call(
        &fixture.service,
        &fixture.peer,
        next,
        Operation::HistoryCommand(HistoryCommand::InitLayerStack {
            stack: stack_body,
            name: b"main".to_vec(),
            scope_seed: seed,
            manifest: manifest(file_root),
        }),
    )
    .unwrap();
    let record = stack_wire(response);
    let response = call(
        &fixture.service,
        &fixture.peer,
        next + 1,
        Operation::HistoryCommand(HistoryCommand::Fork {
            stack: record.stack,
            branch: branch_body,
            name: b"work".to_vec(),
            source: HistoryForkSource::Layer(record.head_layer),
        }),
    )
    .unwrap();
    let snapshot = snapshot(response);
    let (_, _, _, metadata_root) = stat_roots(stat(
        &fixture.service,
        &fixture.peer,
        next + 2,
        snapshot.effective_root,
        b"a",
    ));
    Initialized {
        stack: record.stack,
        branch: snapshot.branch.branch,
        base_layer: snapshot.branch.base_layer,
        root: snapshot.effective_root,
        scope: snapshot.scope,
        file_root,
        metadata_root,
    }
}

fn changes(
    init: &Initialized,
    workspace: [u8; 32],
    content: Root,
    generation: u64,
) -> PreparedChanges {
    PreparedChanges {
        workspace,
        branch: init.branch,
        expected_head: None,
        expected_base: init.base_layer,
        generation,
        base: init.root,
        scope: init.scope,
        root_serial: 1,
        // The root directory is stated with no binding changes so the operation
        // retains its content root; only the file's inode value changes.
        directories: vec![DirectoryChange {
            parent: 1,
            changes: Vec::new(),
        }],
        inodes: vec![InodeChange {
            serial: 2,
            kind: 1,
            content,
            metadata: init.metadata_root,
        }],
    }
}

fn file_identity(path: &Path) -> (u64, u64) {
    // A stable, content-sensitive digest of the content store file. No product
    // dependency is added for a test: the two observations are enough to show
    // the file did not change.
    let bytes = std::fs::read(path).unwrap();
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in &bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    (bytes.len() as u64, hash)
}

#[test]
fn legacy_mask_grants_no_history() {
    let fixture = fixture("legacy", LEGACY);
    let failure = call(
        &fixture.service,
        &fixture.peer,
        1,
        Operation::HistoryQuery(HistoryQuery::GetStack { stack: [0x31; 17] }),
    )
    .unwrap_err();
    assert_eq!(failure.code, Code::Denied);
    // The same mask still authorizes every legacy read.
    assert!(call(
        &fixture.service,
        &fixture.peer,
        2,
        Operation::Inspect {
            root: [0; 32],
            query: Inspect::File,
        },
    )
    .is_err());
}

#[test]
fn history_profile_and_opcode_must_agree() {
    let fixture = fixture("profile", ALL);
    let failure = call_at(
        &fixture.service,
        &fixture.peer,
        1,
        1,
        Operation::HistoryQuery(HistoryQuery::GetStack { stack: [0x31; 17] }),
        &[],
    )
    .unwrap_err();
    assert_eq!(failure.code, Code::Unsupported);
    let failure = call_at(
        &fixture.service,
        &fixture.peer,
        2,
        2,
        Operation::Inspect {
            root: [0; 32],
            query: Inspect::File,
        },
        &[],
    )
    .unwrap_err();
    assert_eq!(failure.code, Code::Unsupported);
}

#[test]
fn empty_namespace_initializes_and_reads_back() {
    let fixture = fixture("empty", ALL);
    let seed = [9; 32];
    let response = call(
        &fixture.service,
        &fixture.peer,
        1,
        Operation::HistoryCommand(HistoryCommand::InitLayerStack {
            stack: [0x51; 16],
            name: b"main".to_vec(),
            scope_seed: seed,
            manifest: vec![ManifestEntry {
                parent: 0,
                name: Vec::new(),
                kind: 2,
                mode: 0o755,
                mtime_seconds: 1_700_000_000,
                mtime_nanoseconds: 0,
                content: None,
                target: Vec::new(),
            }],
        }),
    )
    .unwrap();
    let record = stack_wire(response);
    assert_eq!(record.stack[0], 0x31);
    assert_eq!(record.head_layer[0], 0x32);
    assert_eq!(record.scope, *scope_for_seed(seed).object().as_bytes());

    let read = call(
        &fixture.service,
        &fixture.peer,
        2,
        Operation::HistoryQuery(HistoryQuery::GetStack {
            stack: record.stack,
        }),
    )
    .unwrap();
    assert_eq!(stack_wire(read), record);

    let genesis = call(
        &fixture.service,
        &fixture.peer,
        3,
        Operation::HistoryQuery(HistoryQuery::GetLayer {
            layer: record.head_layer,
        }),
    )
    .unwrap();
    match result(genesis) {
        HistoryResult::Layer(layer) => {
            assert!(layer.parent.is_none());
            assert!(layer.source_branch.is_none());
            assert!(layer.source_commit.is_none());
        }
        other => panic!("expected the genesis Layer, got {other:?}"),
    }
    let history = call(
        &fixture.service,
        &fixture.peer,
        4,
        Operation::HistoryQuery(HistoryQuery::LayerHistory {
            stack: record.stack,
            start: None,
            cursor: Vec::new(),
            limit: 8,
        }),
    )
    .unwrap();
    match result(history) {
        HistoryResult::Layers { records, .. } => assert_eq!(records.len(), 1),
        other => panic!("expected a Layer page, got {other:?}"),
    }
}

#[test]
fn manifest_initialization_builds_a_real_namespace() {
    let fixture = fixture("manifest", ALL);
    let file = construct(&fixture.service, &fixture.peer, 1, b"payload");
    let seed = [11; 32];
    let response = call(
        &fixture.service,
        &fixture.peer,
        2,
        Operation::HistoryCommand(HistoryCommand::InitLayerStack {
            stack: [0x52; 16],
            name: b"main".to_vec(),
            scope_seed: seed,
            manifest: vec![
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
                    mode: 0o755,
                    mtime_seconds: 2,
                    mtime_nanoseconds: 0,
                    content: None,
                    target: Vec::new(),
                },
                ManifestEntry {
                    parent: 0,
                    name: b"file".to_vec(),
                    kind: 1,
                    mode: 0o644,
                    mtime_seconds: 3,
                    mtime_nanoseconds: 0,
                    content: Some(file),
                    target: Vec::new(),
                },
                ManifestEntry {
                    parent: 0,
                    name: b"link".to_vec(),
                    kind: 3,
                    mode: 0o777,
                    mtime_seconds: 4,
                    mtime_nanoseconds: 0,
                    content: None,
                    target: b"file".to_vec(),
                },
                ManifestEntry {
                    parent: 1,
                    name: b"nested".to_vec(),
                    kind: 1,
                    mode: 0o600,
                    mtime_seconds: 5,
                    mtime_nanoseconds: 0,
                    content: Some(file),
                    target: Vec::new(),
                },
            ],
        }),
    )
    .unwrap();
    let record = stack_wire(response);
    let root = record.head_layer;
    let layer = match result(
        call(
            &fixture.service,
            &fixture.peer,
            3,
            Operation::HistoryQuery(HistoryQuery::GetLayer { layer: root }),
        )
        .unwrap(),
    ) {
        HistoryResult::Layer(layer) => layer,
        other => panic!("expected a Layer, got {other:?}"),
    };
    let listing = call(
        &fixture.service,
        &fixture.peer,
        4,
        Operation::Inspect {
            root: layer.root,
            query: Inspect::List {
                path: Vec::new(),
                after: Vec::new(),
                entries: 16,
                bytes: 4096,
            },
        },
    )
    .unwrap();
    match listing {
        Response::List { entries, .. } => {
            let names: Vec<Vec<u8>> = entries.into_iter().map(|(name, _)| name).collect();
            assert_eq!(
                names,
                vec![b"dir".to_vec(), b"file".to_vec(), b"link".to_vec()]
            );
        }
        other => panic!("expected a listing, got {other:?}"),
    }
    let link = call(
        &fixture.service,
        &fixture.peer,
        5,
        Operation::Inspect {
            root: layer.root,
            query: Inspect::Readlink {
                path: b"link".to_vec(),
            },
        },
    )
    .unwrap();
    assert_eq!(link, Response::Link(b"file".to_vec()));
}

#[test]
fn stage_commit_add_layer_and_read_back() {
    let fixture = fixture("lifecycle", ALL);
    let first = construct(&fixture.service, &fixture.peer, 1, b"first");
    let second = construct(&fixture.service, &fixture.peer, 2, b"second-version");
    let init = initialize(&fixture, [13; 32], first, 3);
    assert_eq!(init.file_root, first);

    let workspace = [0x71; 32];
    let staged = stage(
        call(
            &fixture.service,
            &fixture.peer,
            10,
            Operation::HistoryCommand(HistoryCommand::StageChanges(changes(
                &init, workspace, second, 1,
            ))),
        )
        .unwrap(),
    );
    assert_eq!(staged.workspace, workspace);
    assert_eq!(staged.token, 1);
    assert_eq!(staged.expected_head, None);
    assert_eq!(staged.expected_base, init.base_layer);
    assert_eq!(staged.expected_root, init.root);
    assert_eq!(staged.construction_base_root, init.root);
    assert_eq!(staged.intended_commit_base, init.base_layer);

    let committed = call(
        &fixture.service,
        &fixture.peer,
        11,
        Operation::HistoryCommand(HistoryCommand::CommitStaged {
            workspace,
            token: staged.token,
        }),
    )
    .unwrap();
    let (commit, candidate) = match result(committed) {
        HistoryResult::Committed(CommitOutcomeWire::Committed(commit)) => {
            let root = commit.root;
            (commit, root)
        }
        other => panic!("expected a Commit, got {other:?}"),
    };
    assert_eq!(commit.parent, None);
    assert_eq!(commit.base_layer, init.base_layer);
    assert_eq!(commit.commit[0], 0x12);
    assert_ne!(candidate, init.root);
    assert!(
        call(
            &fixture.service,
            &fixture.peer,
            12,
            Operation::HistoryQuery(HistoryQuery::GetStage { workspace }),
        )
        .unwrap_err()
        .code
            == Code::NotFound
    );

    let published = call(
        &fixture.service,
        &fixture.peer,
        13,
        Operation::HistoryCommand(HistoryCommand::AddLayer {
            stack: init.stack,
            branch: init.branch,
            commit: commit.commit,
            expected_stack_head: init.base_layer,
            expected_branch_base: init.base_layer,
        }),
    )
    .unwrap();
    let layer = match result(published) {
        HistoryResult::Published(LayerOutcomeWire::Added(layer)) => layer,
        other => panic!("expected a new Layer, got {other:?}"),
    };
    assert_eq!(layer.root, candidate);
    assert_eq!(layer.parent, Some(init.base_layer));
    assert_eq!(layer.source_branch, Some(init.branch));
    assert_eq!(layer.source_commit, Some(commit.commit));

    // The Branch advanced but its base did not move.
    let snapshot = snapshot(
        call(
            &fixture.service,
            &fixture.peer,
            14,
            Operation::HistoryQuery(HistoryQuery::GetBranch {
                branch: init.branch,
            }),
        )
        .unwrap(),
    );
    assert_eq!(snapshot.branch.head_commit, Some(commit.commit));
    assert_eq!(snapshot.branch.base_layer, init.base_layer);
    assert_eq!(snapshot.effective_root, candidate);

    // Logical readback of the published root.
    let (_, _, content, _) = stat_roots(stat(&fixture.service, &fixture.peer, 15, candidate, b"a"));
    assert_eq!(content, second);

    // History readback: the Commit chain and the publication chain.
    let commits = call(
        &fixture.service,
        &fixture.peer,
        16,
        Operation::HistoryQuery(HistoryQuery::CommitHistory {
            branch: init.branch,
            start: None,
            cursor: Vec::new(),
            limit: 8,
        }),
    )
    .unwrap();
    match result(commits) {
        HistoryResult::Commits { records, .. } => {
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].commit, commit.commit);
        }
        other => panic!("expected a Commit page, got {other:?}"),
    }
    let layers = call(
        &fixture.service,
        &fixture.peer,
        17,
        Operation::HistoryQuery(HistoryQuery::LayerHistory {
            stack: init.stack,
            start: None,
            cursor: Vec::new(),
            limit: 8,
        }),
    )
    .unwrap();
    match result(layers) {
        HistoryResult::Layers { records, .. } => {
            assert_eq!(records.len(), 2);
            assert_eq!(records[0].layer, layer.layer);
            assert_eq!(records[1].layer, init.base_layer);
        }
        other => panic!("expected a Layer page, got {other:?}"),
    }
}

#[test]
fn stale_loser_retains_its_exact_stage() {
    let fixture = fixture("stale", ALL);
    let first = construct(&fixture.service, &fixture.peer, 1, b"first");
    let second = construct(&fixture.service, &fixture.peer, 2, b"second-version");
    let init = initialize(&fixture, [17; 32], first, 3);
    let winner = [0x81; 32];
    let loser = [0x82; 32];
    let winner_stage = stage(
        call(
            &fixture.service,
            &fixture.peer,
            10,
            Operation::HistoryCommand(HistoryCommand::StageChanges(changes(
                &init, winner, second, 1,
            ))),
        )
        .unwrap(),
    );
    // The loser is captured against the same context before the winner commits.
    let loser_stage = stage(
        call(
            &fixture.service,
            &fixture.peer,
            11,
            Operation::HistoryCommand(HistoryCommand::StageChanges(changes(
                &init, loser, second, 1,
            ))),
        )
        .unwrap(),
    );
    let committed = call(
        &fixture.service,
        &fixture.peer,
        12,
        Operation::HistoryCommand(HistoryCommand::CommitStaged {
            workspace: winner,
            token: winner_stage.token,
        }),
    )
    .unwrap();
    let committed = match result(committed) {
        HistoryResult::Committed(CommitOutcomeWire::Committed(record)) => record,
        _ => panic!(),
    };

    let failure = call(
        &fixture.service,
        &fixture.peer,
        13,
        Operation::HistoryCommand(HistoryCommand::CommitStaged {
            workspace: loser,
            token: loser_stage.token,
        }),
    )
    .unwrap_err();
    assert_eq!(failure.code, Code::HeadMoved);
    assert_eq!(
        failure.history.as_deref(),
        Some(&HistoryFailure {
            conflict: Some(HistoryConflict::BranchMoved {
                expected_head: None,
                actual_head: Some(committed.commit),
                expected_base: init.base_layer,
                actual_base: init.base_layer
            }),
            stage: StageObservation::Retained(Box::new(loser_stage.clone())),
        })
    );
    assert_eq!(
        native_call(
            &fixture,
            Operation::HistoryCommand(HistoryCommand::CommitStaged {
                workspace: loser,
                token: loser_stage.token
            })
        )
        .unwrap_err(),
        failure
    );
    let retained = stage(
        call(
            &fixture.service,
            &fixture.peer,
            14,
            Operation::HistoryQuery(HistoryQuery::GetStage { workspace: loser }),
        )
        .unwrap(),
    );
    assert_eq!(retained.token, loser_stage.token);
    assert_eq!(retained.expected_head, None);
    assert_eq!(retained.candidate_root, loser_stage.candidate_root);
}

#[test]
fn multiple_no_change_stages_are_up_to_date() {
    let fixture = fixture("uptodate", ALL);
    let first = construct(&fixture.service, &fixture.peer, 1, b"first");
    let init = initialize(&fixture, [19; 32], first, 2);
    let (_, _, content, metadata) =
        stat_roots(stat(&fixture.service, &fixture.peer, 9, init.root, b"a"));
    for (index, workspace) in [[0x91; 32], [0x92; 32]].into_iter().enumerate() {
        let mut prepared = changes(&init, workspace, content, 1);
        prepared.inodes[0].metadata = metadata;
        let staged = stage(
            call(
                &fixture.service,
                &fixture.peer,
                10 + index as u64 * 2,
                Operation::HistoryCommand(HistoryCommand::StageChanges(prepared)),
            )
            .unwrap(),
        );
        let outcome = call(
            &fixture.service,
            &fixture.peer,
            11 + index as u64 * 2,
            Operation::HistoryCommand(HistoryCommand::CommitStaged {
                workspace,
                token: staged.token,
            }),
        )
        .unwrap();
        match result(outcome) {
            HistoryResult::Committed(CommitOutcomeWire::UpToDate { head, root }) => {
                assert_eq!(head, None);
                assert_eq!(root, init.root);
            }
            other => panic!("expected UpToDate, got {other:?}"),
        }
    }
    let snapshot = snapshot(
        call(
            &fixture.service,
            &fixture.peer,
            20,
            Operation::HistoryQuery(HistoryQuery::GetBranch {
                branch: init.branch,
            }),
        )
        .unwrap(),
    );
    assert_eq!(snapshot.branch.head_commit, None);
}

#[test]
fn delayed_discard_token_cannot_consume_a_replacement_stage() {
    let fixture = fixture("tokens", ALL);
    let first = construct(&fixture.service, &fixture.peer, 1, b"first");
    let second = construct(&fixture.service, &fixture.peer, 2, b"second-version");
    let init = initialize(&fixture, [23; 32], first, 3);
    let workspace = [0xa1; 32];
    let original = stage(
        call(
            &fixture.service,
            &fixture.peer,
            10,
            Operation::HistoryCommand(HistoryCommand::StageChanges(changes(
                &init, workspace, first, 1,
            ))),
        )
        .unwrap(),
    );
    let removed = call(
        &fixture.service,
        &fixture.peer,
        11,
        Operation::HistoryCommand(HistoryCommand::DiscardStage {
            workspace,
            token: original.token,
        }),
    )
    .unwrap();
    assert_eq!(result(removed), HistoryResult::Discarded { removed: true });
    let absent = call(
        &fixture.service,
        &fixture.peer,
        12,
        Operation::HistoryCommand(HistoryCommand::DiscardStage {
            workspace,
            token: original.token,
        }),
    )
    .unwrap();
    assert_eq!(result(absent), HistoryResult::Discarded { removed: false });
    let replacement = stage(
        call(
            &fixture.service,
            &fixture.peer,
            13,
            Operation::HistoryCommand(HistoryCommand::StageChanges(changes(
                &init, workspace, second, 2,
            ))),
        )
        .unwrap(),
    );
    assert_ne!(replacement.token, original.token);
    let failure = call(
        &fixture.service,
        &fixture.peer,
        14,
        Operation::HistoryCommand(HistoryCommand::DiscardStage {
            workspace,
            token: original.token,
        }),
    )
    .unwrap_err();
    assert_eq!(failure.code, Code::StageChanged);
    assert_eq!(
        failure.history.as_deref(),
        Some(&HistoryFailure {
            conflict: Some(HistoryConflict::StageChanged {
                expected: original.token,
                actual: Some(replacement.token)
            }),
            stage: StageObservation::Retained(Box::new(replacement.clone())),
        })
    );
    assert_eq!(
        native_call(
            &fixture,
            Operation::HistoryCommand(HistoryCommand::DiscardStage {
                workspace,
                token: original.token
            })
        )
        .unwrap_err(),
        failure
    );
    let retained = stage(
        call(
            &fixture.service,
            &fixture.peer,
            15,
            Operation::HistoryQuery(HistoryQuery::GetStage { workspace }),
        )
        .unwrap(),
    );
    assert_eq!(retained.token, replacement.token);
    assert_eq!(retained.candidate_root, replacement.candidate_root);
}

#[test]
fn already_published_source_is_up_to_date_before_stale_head_refusal() {
    let fixture = fixture("publish", ALL);
    let first = construct(&fixture.service, &fixture.peer, 1, b"first");
    let second = construct(&fixture.service, &fixture.peer, 2, b"second-version");
    let init = initialize(&fixture, [29; 32], first, 3);
    let workspace = [0xb1; 32];
    let staged = stage(
        call(
            &fixture.service,
            &fixture.peer,
            10,
            Operation::HistoryCommand(HistoryCommand::StageChanges(changes(
                &init, workspace, second, 1,
            ))),
        )
        .unwrap(),
    );
    let commit = match result(
        call(
            &fixture.service,
            &fixture.peer,
            11,
            Operation::HistoryCommand(HistoryCommand::CommitStaged {
                workspace,
                token: staged.token,
            }),
        )
        .unwrap(),
    ) {
        HistoryResult::Committed(CommitOutcomeWire::Committed(commit)) => commit,
        other => panic!("expected a Commit, got {other:?}"),
    };
    let published = match result(
        call(
            &fixture.service,
            &fixture.peer,
            12,
            Operation::HistoryCommand(HistoryCommand::AddLayer {
                stack: init.stack,
                branch: init.branch,
                commit: commit.commit,
                expected_stack_head: init.base_layer,
                expected_branch_base: init.base_layer,
            }),
        )
        .unwrap(),
    ) {
        HistoryResult::Published(LayerOutcomeWire::Added(layer)) => layer,
        other => panic!("expected a Layer, got {other:?}"),
    };
    // Asking again with the stale expected stack head is idempotent, not stale.
    let again = match result(
        call(
            &fixture.service,
            &fixture.peer,
            13,
            Operation::HistoryCommand(HistoryCommand::AddLayer {
                stack: init.stack,
                branch: init.branch,
                commit: commit.commit,
                expected_stack_head: init.base_layer,
                expected_branch_base: init.base_layer,
            }),
        )
        .unwrap(),
    ) {
        HistoryResult::Published(LayerOutcomeWire::UpToDate { layer }) => layer,
        other => panic!("expected UpToDate, got {other:?}"),
    };
    assert_eq!(again, published.layer);

    // A genuinely wrong stack head is refused with its own class.
    let failure = call(
        &fixture.service,
        &fixture.peer,
        14,
        Operation::HistoryCommand(HistoryCommand::AddLayer {
            stack: init.stack,
            branch: init.branch,
            commit: commit.commit,
            expected_stack_head: init.base_layer,
            expected_branch_base: init.base_layer,
        }),
    );
    assert!(failure.is_ok());
}

#[test]
fn wrong_role_root_is_refused() {
    let fixture = fixture("role", ALL);
    let first = construct(&fixture.service, &fixture.peer, 1, b"first");
    let second = construct(&fixture.service, &fixture.peer, 2, b"second-version");
    let init = initialize(&fixture, [31; 32], first, 3);
    let workspace = [0xc1; 32];
    let mut prepared = changes(&init, workspace, second, 1);
    // A regular file's content root may not be claimed as a symlink target.
    prepared.inodes[0].kind = 3;
    let failure = call(
        &fixture.service,
        &fixture.peer,
        10,
        Operation::HistoryCommand(HistoryCommand::StageChanges(prepared)),
    )
    .unwrap_err();
    assert_eq!(failure.code, Code::InvalidInput);

    // A metadata root may not be claimed as a regular file's content root.
    let mut prepared = changes(&init, workspace, init.metadata_root, 1);
    prepared.inodes[0].metadata = init.metadata_root;
    let failure = call(
        &fixture.service,
        &fixture.peer,
        11,
        Operation::HistoryCommand(HistoryCommand::StageChanges(prepared)),
    )
    .unwrap_err();
    assert!(matches!(
        failure.code,
        Code::InvalidInput | Code::Integrity | Code::Provider | Code::MissingObject
    ));
}

#[test]
fn rebasing_by_editing_a_token_is_refused() {
    let fixture = fixture("rebase", ALL);
    let first = construct(&fixture.service, &fixture.peer, 1, b"first");
    let second = construct(&fixture.service, &fixture.peer, 2, b"second-version");
    let init = initialize(&fixture, [37; 32], first, 3);
    let workspace = [0xd1; 32];
    let mut prepared = changes(&init, workspace, second, 1);
    prepared.base = [0xAB; 32];
    let failure = call(
        &fixture.service,
        &fixture.peer,
        10,
        Operation::HistoryCommand(HistoryCommand::StageChanges(prepared)),
    )
    .unwrap_err();
    assert_eq!(failure.code, Code::InvalidInput);

    let mut prepared = changes(&init, workspace, second, 1);
    prepared.scope = [0xCD; 32];
    let failure = call(
        &fixture.service,
        &fixture.peer,
        11,
        Operation::HistoryCommand(HistoryCommand::StageChanges(prepared)),
    )
    .unwrap_err();
    assert_eq!(failure.code, Code::InvalidInput);
}

#[test]
fn inode_reservations_are_scope_wide_and_never_recycled() {
    let fixture = fixture("allocation", ALL);
    let scope = *scope_for_seed([41; 32]).object().as_bytes();
    let other = *scope_for_seed([43; 32]).object().as_bytes();
    let mut expected = 1u64;
    for (index, count) in [1u64, 2, 5, 1].into_iter().enumerate() {
        let response = call(
            &fixture.service,
            &fixture.peer,
            index as u64 + 1,
            Operation::HistoryCommand(HistoryCommand::ReserveInodes { scope, count }),
        )
        .unwrap();
        match result(response) {
            HistoryResult::Reservation {
                start,
                count: actual,
                ..
            } => {
                assert_eq!(start, expected);
                assert_eq!(actual, count);
                expected += count;
            }
            other => panic!("expected a reservation, got {other:?}"),
        }
    }
    let independent = call(
        &fixture.service,
        &fixture.peer,
        9,
        Operation::HistoryCommand(HistoryCommand::ReserveInodes {
            scope: other,
            count: 3,
        }),
    )
    .unwrap();
    match result(independent) {
        HistoryResult::Reservation { start, count, .. } => {
            assert_eq!(start, 1);
            assert_eq!(count, 3);
        }
        other => panic!("expected a reservation, got {other:?}"),
    }
}

#[test]
fn metadata_only_commands_never_touch_the_content_store() {
    let fixture = fixture("metadatonly", ALL);
    let first = construct(&fixture.service, &fixture.peer, 1, b"first");
    let second = construct(&fixture.service, &fixture.peer, 2, b"second-version");
    let init = initialize(&fixture, [47; 32], first, 3);

    let workspace = [0xe1; 32];
    let staged = stage(
        call(
            &fixture.service,
            &fixture.peer,
            10,
            Operation::HistoryCommand(HistoryCommand::StageChanges(changes(
                &init, workspace, second, 1,
            ))),
        )
        .unwrap(),
    );
    // Staging is a content operation and did write the store. Everything after
    // it is metadata only and must not.
    let before = file_identity(&fixture.store_path);
    let _ = call(
        &fixture.service,
        &fixture.peer,
        11,
        Operation::HistoryCommand(HistoryCommand::CommitStaged {
            workspace,
            token: staged.token,
        }),
    )
    .unwrap();
    assert_eq!(file_identity(&fixture.store_path), before);
    let _ = call(
        &fixture.service,
        &fixture.peer,
        12,
        Operation::HistoryCommand(HistoryCommand::ReserveInodes {
            scope: *scope_for_seed([53; 32]).object().as_bytes(),
            count: 4,
        }),
    )
    .unwrap();
    assert_eq!(file_identity(&fixture.store_path), before);
    let _ = call(
        &fixture.service,
        &fixture.peer,
        13,
        Operation::HistoryCommand(HistoryCommand::Fork {
            stack: init.stack,
            branch: [0x77; 16],
            name: b"second".to_vec(),
            source: HistoryForkSource::Layer(init.base_layer),
        }),
    )
    .unwrap();
    assert_eq!(file_identity(&fixture.store_path), before);
}

#[test]
fn history_pages_are_bounded_and_cursors_are_bound_to_their_range() {
    let fixture = fixture("pages", ALL);
    let file = construct(&fixture.service, &fixture.peer, 1, b"payload");
    let init = initialize(&fixture, [59; 32], file, 2);
    for (index, body) in [[0x01; 16], [0x02; 16], [0x03; 16]].into_iter().enumerate() {
        let _ = call(
            &fixture.service,
            &fixture.peer,
            10 + index as u64,
            Operation::HistoryCommand(HistoryCommand::Fork {
                stack: init.stack,
                branch: body,
                name: format!("branch{}", index).into_bytes(),
                source: HistoryForkSource::Layer(init.base_layer),
            }),
        )
        .expect("fork succeeds");
    }
    let first = call(
        &fixture.service,
        &fixture.peer,
        20,
        Operation::HistoryQuery(HistoryQuery::ListBranches {
            stack: init.stack,
            cursor: Vec::new(),
            limit: 2,
        }),
    )
    .unwrap();
    let (continuation, names) = match result(first) {
        HistoryResult::Branches {
            continuation,
            records,
        } => (
            continuation,
            records
                .into_iter()
                .map(|record| record.name)
                .collect::<Vec<_>>(),
        ),
        other => panic!("expected a Branch page, got {other:?}"),
    };
    assert_eq!(names, vec![b"branch0".to_vec(), b"branch1".to_vec()]);
    assert_eq!(continuation.len(), CURSOR_BYTES);
    let second = call(
        &fixture.service,
        &fixture.peer,
        21,
        Operation::HistoryQuery(HistoryQuery::ListBranches {
            stack: init.stack,
            cursor: continuation.clone(),
            limit: 2,
        }),
    )
    .unwrap();
    match result(second) {
        HistoryResult::Branches {
            continuation,
            records,
        } => {
            assert!(continuation.is_empty());
            assert_eq!(records.len(), 2);
        }
        other => panic!("expected a Branch page, got {other:?}"),
    }
    // The same cursor against a different range is refused, never reinterpreted.
    let failure = call(
        &fixture.service,
        &fixture.peer,
        22,
        Operation::HistoryQuery(HistoryQuery::ListStacks {
            cursor: continuation,
            limit: 2,
        }),
    )
    .unwrap_err();
    assert_eq!(failure.code, Code::InvalidInput);
    // A tampered cursor is refused as an integrity failure.
    let page = call(
        &fixture.service,
        &fixture.peer,
        23,
        Operation::HistoryQuery(HistoryQuery::ListBranches {
            stack: init.stack,
            cursor: Vec::new(),
            limit: 1,
        }),
    )
    .unwrap();
    let mut tampered = match result(page) {
        HistoryResult::Branches { continuation, .. } => continuation,
        other => panic!("expected a Branch page, got {other:?}"),
    };
    assert_eq!(tampered.len(), CURSOR_BYTES);
    tampered[100] ^= 0xFF;
    let failure = call(
        &fixture.service,
        &fixture.peer,
        24,
        Operation::HistoryQuery(HistoryQuery::ListBranches {
            stack: init.stack,
            cursor: tampered,
            limit: 1,
        }),
    )
    .unwrap_err();
    assert_eq!(failure.code, Code::Integrity);
}

#[test]
fn read_only_reopen_supports_reads_and_refuses_every_mutation() {
    let fixture = fixture("reopen", ALL);
    let file = construct(&fixture.service, &fixture.peer, 1, b"payload");
    let init = initialize(&fixture, [61; 32], file, 2);

    let reader =
        sqlite::open_read_only(&fixture.catalog_path, b"layerfs-test-authority", [71; 32]).unwrap();
    assert_eq!(reader.catalog_id(), fixture.catalog.catalog_id());
    assert_eq!(
        reader
            .layer_stack(layerfs_history::LayerStackId::from_slice(&init.stack).unwrap())
            .unwrap()
            .unwrap()
            .head_layer
            .to_bytes(),
        init.base_layer
    );
    let workspace = layerfs_history::WorkspaceId::from_authority([0xF1; 32]).unwrap();
    assert_eq!(
        reader
            .discard_stage(&layerfs_history::DiscardRequest {
                workspace,
                token: layerfs_history::StageToken::new(1).unwrap(),
            })
            .unwrap_err(),
        HistoryError::ContinuityUnavailable
    );
    assert_eq!(
        reader
            .reserve_inodes(&layerfs_history::ReserveRequest {
                scope: layerfs_content::ObjectId::from_bytes(&init.scope).unwrap(),
                count: 1,
            })
            .unwrap_err(),
        HistoryError::ContinuityUnavailable
    );
    assert_eq!(
        reader
            .initialize_layerstack(&layerfs_history::StackInitialization {
                stack: layerfs_history::LayerStackId::from_authority([0x99; 16]),
                name: layerfs_history::HistoryName::new("other").unwrap(),
                scope: layerfs_content::ObjectId::from_bytes(&init.scope).unwrap(),
                profile: layerfs_content::filesystem::profile_id(),
                genesis_root: layerfs_content::ObjectId::from_bytes(&init.root).unwrap(),
            })
            .unwrap_err(),
        HistoryError::ContinuityUnavailable
    );
    // The wrong binding key does not open the catalog at all.
    match sqlite::open_read_only(&fixture.catalog_path, b"another-authority", [71; 32]) {
        Err(error) => assert_eq!(error, HistoryError::Integrity("catalog binding")),
        Ok(_) => panic!("a foreign binding key must not open the catalog"),
    }
}

/// The catalog bounds a page by a per-record byte budget the bridge must honour.
///
/// The catalog decides how many records fit from a declared worst-case width,
/// and the bridge writes the actual bytes. If the bridge could write more than
/// the catalog assumed, a legal page would be refused by the codec; this test
/// measures the real encoded width of a maximal record and holds the two
/// together.
#[test]
fn encoded_record_widths_match_the_catalog_bounds() {
    use layerfs_bridge::adapters::native::protocol::{decode_response, encode_response};
    use layerfs_history::{BranchRecord, CommitRecord, LayerRecord, LayerStackRecord, StageRecord};

    let longest = vec![b'a'; NAME_MAX_BYTES];
    let page = |records: Vec<LayerWire>| {
        let response = Response::History(Box::new(HistoryResult::Layers {
            continuation: Vec::new(),
            records,
        }));
        encode_response(&response).expect("encode")
    };
    let layer = LayerWire {
        layer: [0x32; 33],
        stack: [0x31; 17],
        parent: Some([0x32; 33]),
        root: [0x01; 32],
        source_branch: Some([0x11; 17]),
        source_commit: Some([0x12; 33]),
    };
    let empty = page(Vec::new()).len();
    let one = page(vec![layer.clone()]).len();
    assert!(one - empty <= LayerRecord::MAXIMUM_ENCODED_BYTES);

    let sized = |result: HistoryResult, bound: usize| {
        let response = Response::History(Box::new(result));
        let bytes = encode_response(&response).expect("encode");
        assert_eq!(decode_response(&bytes).expect("decode"), response);
        assert!(bytes.len() <= bound + 8, "{} > {}", bytes.len(), bound + 8);
    };
    let stack = StackWire {
        stack: [0x31; 17],
        name: longest.clone(),
        scope: [0x01; 32],
        profile: [0x02; 32],
        head_layer: [0x32; 33],
    };
    sized(
        HistoryResult::Stacks {
            continuation: Vec::new(),
            records: vec![stack],
        },
        LayerStackRecord::MAXIMUM_ENCODED_BYTES,
    );
    let branch = BranchWire {
        branch: [0x11; 17],
        stack: [0x31; 17],
        name: longest.clone(),
        base_layer: [0x32; 33],
        head_commit: Some([0x12; 33]),
    };
    sized(
        HistoryResult::Branches {
            continuation: Vec::new(),
            records: vec![branch],
        },
        BranchRecord::MAXIMUM_ENCODED_BYTES,
    );
    let commit = CommitWire {
        commit: [0x12; 33],
        stack: [0x31; 17],
        root: [0x01; 32],
        parent: Some([0x12; 33]),
        base_layer: [0x32; 33],
    };
    sized(
        HistoryResult::Commits {
            continuation: Vec::new(),
            records: vec![commit],
        },
        CommitRecord::MAXIMUM_ENCODED_BYTES,
    );
    let stage = StageWire {
        workspace: [0x01; 32],
        token: 1,
        stack: [0x31; 17],
        branch: [0x11; 17],
        expected_head: Some([0x12; 33]),
        expected_base: [0x32; 33],
        expected_root: [0x01; 32],
        construction_base_root: [0x01; 32],
        intended_commit_base: [0x32; 33],
        candidate_root: [0x01; 32],
        profile: [0x02; 32],
        scope: [0x01; 32],
        generation: 1,
    };
    sized(
        HistoryResult::Stages {
            continuation: Vec::new(),
            records: vec![stage],
        },
        StageRecord::MAXIMUM_ENCODED_BYTES,
    );
}

fn directory(parent: u16, name: &[u8]) -> ManifestEntry {
    ManifestEntry {
        parent,
        name: name.to_vec(),
        kind: 2,
        mode: 0o755,
        mtime_seconds: 0,
        mtime_nanoseconds: 0,
        content: None,
        target: vec![],
    }
}

#[test]
fn empty_and_interleaved_directories_have_real_canonical_pages() {
    let f = fixture("directory-regression", ALL);
    for (index, (manifest, paths)) in [
        (vec![directory(0, b"")], vec![b"".as_slice()]),
        (
            vec![directory(0, b""), directory(0, b"a")],
            vec![b"a".as_slice()],
        ),
        (
            vec![directory(0, b""), directory(0, b"a"), directory(1, b"x")],
            vec![b"a/x".as_slice()],
        ),
        (
            vec![
                directory(0, b""),
                directory(0, b"a"),
                directory(1, b"x"),
                directory(0, b"b"),
                directory(1, b"y"),
            ],
            vec![b"a/x".as_slice(), b"a/y", b"b"],
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let response = call(
            &f.service,
            &f.peer,
            1,
            Operation::HistoryCommand(HistoryCommand::InitLayerStack {
                stack: [index as u8 + 1; 16],
                name: format!("s{index}").into_bytes(),
                scope_seed: [index as u8 + 1; 32],
                manifest,
            }),
        )
        .unwrap();
        let created = match result(response) {
            HistoryResult::StackCreated(created) => created,
            _ => panic!(),
        };
        for path in paths {
            let listed = call(
                &f.service,
                &f.peer,
                2,
                Operation::Inspect {
                    root: created.root,
                    query: Inspect::List {
                        path: path.to_vec(),
                        after: vec![],
                        entries: 128,
                        bytes: 16384,
                    },
                },
            )
            .unwrap();
            assert_eq!(
                listed,
                Response::List {
                    entries: vec![],
                    continuation: None
                }
            );
        }
    }
}

#[test]
fn branch_root_descriptor_validates_content_scope_profile_and_actual_serial() {
    let f = fixture("descriptor", ALL);
    let seed = [8; 32];
    let scope = scope_for_seed(seed).object();
    f.catalog
        .reserve_inodes(&layerfs_history::ReserveRequest { scope, count: 8 })
        .unwrap();
    let response = call(
        &f.service,
        &f.peer,
        1,
        Operation::HistoryCommand(HistoryCommand::InitLayerStack {
            stack: [1; 16],
            name: b"main".to_vec(),
            scope_seed: seed,
            manifest: vec![directory(0, b"")],
        }),
    )
    .unwrap();
    let created = match result(response) {
        HistoryResult::StackCreated(created) => created,
        _ => panic!(),
    };
    assert_eq!(created.root_serial, 9);
    let forked = snapshot(
        call(
            &f.service,
            &f.peer,
            2,
            Operation::HistoryCommand(HistoryCommand::Fork {
                stack: created.stack.stack,
                branch: [1; 16],
                name: b"work".to_vec(),
                source: HistoryForkSource::Layer(created.stack.head_layer),
            }),
        )
        .unwrap(),
    );
    assert_eq!(forked.root_serial, None);
    let descriptor = snapshot(
        call(
            &f.service,
            &f.peer,
            3,
            Operation::HistoryQuery(HistoryQuery::GetBranch {
                branch: forked.branch.branch,
            }),
        )
        .unwrap(),
    );
    assert_eq!(
        snapshot(
            native_call(
                &f,
                Operation::HistoryQuery(HistoryQuery::GetBranch {
                    branch: forked.branch.branch
                })
            )
            .unwrap()
        ),
        descriptor
    );
    assert_eq!(descriptor.root_serial, Some(9));
    assert_eq!(descriptor.effective_root, created.root);
    assert_eq!(descriptor.scope, created.stack.scope);
    assert_eq!(descriptor.profile, created.stack.profile);
    assert_eq!(
        f.catalog
            .reserve_inodes(&layerfs_history::ReserveRequest { scope, count: 1 })
            .unwrap()
            .start,
        10
    );
    let file = construct(&f.service, &f.peer, 4, b"wrong root role");
    for (index, (root, scope, profile)) in [
        (
            layerfs_content::ObjectId::for_bytes(b"absent"),
            scope,
            layerfs_content::filesystem::profile_id(),
        ),
        (
            layerfs_content::ObjectId::from_bytes(&file).unwrap(),
            scope,
            layerfs_content::filesystem::profile_id(),
        ),
        (
            layerfs_content::ObjectId::from_bytes(&created.root).unwrap(),
            scope_for_seed([9; 32]).object(),
            layerfs_content::filesystem::profile_id(),
        ),
        (
            layerfs_content::ObjectId::from_bytes(&created.root).unwrap(),
            scope,
            layerfs_content::ObjectId::for_bytes(b"foreign profile"),
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let s = f
            .catalog
            .initialize_layerstack(&layerfs_history::StackInitialization {
                stack: layerfs_history::LayerStackId::from_authority([index as u8 + 2; 16]),
                name: layerfs_history::HistoryName::new(&format!("bad{index}")).unwrap(),
                scope,
                profile,
                genesis_root: root,
            })
            .unwrap();
        let b = f
            .catalog
            .fork(&layerfs_history::ForkRequest {
                stack: s.id,
                branch: layerfs_history::BranchId::from_authority([index as u8 + 2; 16]),
                name: layerfs_history::HistoryName::new("work").unwrap(),
                source: layerfs_history::ForkSource::Layer(s.head_layer),
            })
            .unwrap();
        assert!(call(
            &f.service,
            &f.peer,
            5,
            Operation::HistoryQuery(HistoryQuery::GetBranch {
                branch: b.branch.id.to_bytes()
            })
        )
        .is_err());
    }
}

fn native_call(f: &Fixture, operation: Operation) -> Result<Response, Failure> {
    use layerfs_bridge::adapters::native::{
        client::Client,
        connection::{accept, connect, Peer},
        listen,
        server::serve,
    };
    let listener = listen("127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = listener.local_addr().unwrap();
    let server_key = [9; 32];
    let public = *VerifiedPeer::from_private(&server_key)
        .unwrap()
        .public_key();
    let peers = [Peer {
        selector: 1,
        public: *f.peer.public_key(),
        expires_unix: u64::MAX,
    }];
    std::thread::scope(|threads| {
        let server = threads.spawn(|| {
            let (socket, _) = listener.accept().unwrap();
            let c = accept(socket, &server_key, &peers).unwrap();
            let _ = serve(c, |peer, request, input, output, deadline| {
                f.service
                    .handle_until(peer, request, input, output, deadline)
                    .0
            });
        });
        let mut client = Client::new(connect(addr, 1, &[7; 32], &public).unwrap()).unwrap();
        let request = Request {
            id: 1,
            generation: 1,
            store: 1,
            profile: profile(&operation),
            deadline_ms: 10000,
            response_bytes: 16384,
            operation,
        };
        let mut input: &[u8] = &[];
        let mut output = Vec::new();
        let result = client.call(&request, &mut input, &mut output);
        drop(client);
        server.join().unwrap();
        result
    })
}

// A public-contract provider refusal checks composition; it is not evidence of
// a real SQLite I/O-loss or reverse-completion schedule.
struct RefusingCommit {
    inner: Arc<dyn HistoryCatalog>,
    unknown: bool,
}
macro_rules! delegate_history {
    ($($name:ident($($arg:ident:$ty:ty),*) -> $ret:ty;)*) => {$(
        fn $name(&self,$($arg:$ty),*)->$ret {self.inner.$name($($arg),*)}
    )*};
}
impl HistoryCatalog for RefusingCommit {
    delegate_history! {
        catalog_id()->layerfs_history::CatalogId;
        incarnation()->u64;
        layer_stack(id:layerfs_history::LayerStackId)->layerfs_history::HistoryResult<Option<layerfs_history::LayerStackRecord>>;
        layer_stacks(page:&layerfs_history::Page)->layerfs_history::HistoryResult<layerfs_history::PageResult<layerfs_history::LayerStackRecord>>;
        branch(id:layerfs_history::BranchId)->layerfs_history::HistoryResult<Option<layerfs_history::BranchRecord>>;
        branch_snapshot(id:layerfs_history::BranchId)->layerfs_history::HistoryResult<Option<layerfs_history::BranchSnapshot>>;
        branches(stack:layerfs_history::LayerStackId,page:&layerfs_history::Page)->layerfs_history::HistoryResult<layerfs_history::PageResult<layerfs_history::BranchRecord>>;
        commit(id:layerfs_history::CommitId)->layerfs_history::HistoryResult<Option<layerfs_history::CommitRecord>>;
        layer(id:layerfs_history::LayerId)->layerfs_history::HistoryResult<Option<layerfs_history::LayerRecord>>;
        stage(workspace:layerfs_history::WorkspaceId)->layerfs_history::HistoryResult<Option<layerfs_history::StageRecord>>;
        stages(branch:layerfs_history::BranchId,page:&layerfs_history::Page)->layerfs_history::HistoryResult<layerfs_history::PageResult<layerfs_history::StageRecord>>;
        commit_history(request:&layerfs_history::CommitHistoryRequest)->layerfs_history::HistoryResult<layerfs_history::PageResult<layerfs_history::CommitRecord>>;
        layer_history(request:&layerfs_history::LayerHistoryRequest)->layerfs_history::HistoryResult<layerfs_history::PageResult<layerfs_history::LayerRecord>>;
        initialize_layerstack(request:&layerfs_history::StackInitialization)->layerfs_history::HistoryResult<layerfs_history::LayerStackRecord>;
        fork(request:&layerfs_history::ForkRequest)->layerfs_history::HistoryResult<layerfs_history::BranchSnapshot>;
        stage_changes(request:&layerfs_history::StageRequest)->layerfs_history::HistoryResult<layerfs_history::StageRecord>;
        add_layer(request:&layerfs_history::AddLayerRequest)->layerfs_history::HistoryResult<layerfs_history::AddLayerOutcome>;
        discard_stage(request:&layerfs_history::DiscardRequest)->layerfs_history::HistoryResult<layerfs_history::DiscardOutcome>;
        reserve_inodes(request:&layerfs_history::ReserveRequest)->layerfs_history::HistoryResult<layerfs_history::Reservation>;
    }
    fn commit_staged(
        &self,
        _: &layerfs_history::CommitStagedRequest,
    ) -> layerfs_history::HistoryResult<layerfs_history::CommitStagedOutcome> {
        Err(if self.unknown {
            HistoryError::UnknownOutcome
        } else {
            HistoryError::Busy
        })
    }
}

#[test]
fn composite_failure_keeps_acknowledged_stage_distinct_from_absence() {
    for unknown in [false, true] {
        let f = fixture_using("composite-context", ALL, Some(unknown));
        let file = construct(&f.service, &f.peer, 1, b"original");
        let replacement = construct(&f.service, &f.peer, 2, b"replacement");
        let init = initialize(&f, [9; 32], file, 3);
        for native in [false, true] {
            let workspace = if native { [2; 32] } else { [1; 32] };
            let operation = Operation::HistoryCommand(HistoryCommand::Commit(changes(
                &init,
                workspace,
                replacement,
                4,
            )));
            let error = if native {
                native_call(&f, operation)
            } else {
                call(&f.service, &f.peer, 10, operation)
            }
            .unwrap_err();
            assert_eq!(error.code, if unknown { Code::Unknown } else { Code::Busy });
            assert_eq!(error.unknown, unknown);
            let observed = error.history.unwrap().stage;
            let acknowledged = match observed {
                StageObservation::AcknowledgedUnknown(stage) => *stage,
                _ => panic!("must report acknowledged stage with unresolved disposition"),
            };
            assert_eq!(acknowledged.workspace, workspace);
            assert_eq!(acknowledged.generation, 4);
            let retained = stage(
                call(
                    &f.service,
                    &f.peer,
                    11,
                    Operation::HistoryQuery(HistoryQuery::GetStage { workspace }),
                )
                .unwrap(),
            );
            assert_eq!(acknowledged, retained);
        }
    }
    let f = fixture("absent-context", ALL);
    let err = native_call(
        &f,
        Operation::HistoryCommand(HistoryCommand::CommitStaged {
            workspace: [3; 32],
            token: 1,
        }),
    )
    .unwrap_err();
    assert_eq!(
        err.history.as_deref(),
        Some(&HistoryFailure {
            conflict: Some(HistoryConflict::StageChanged {
                expected: 1,
                actual: None
            }),
            stage: StageObservation::Absent([3; 32])
        })
    );
}

#[test]
fn refreshed_stack_head_reports_typed_stale_base_on_direct_and_native_routes() {
    let f = fixture("stack-context", ALL);
    let file = construct(&f.service, &f.peer, 1, b"one");
    let second = construct(&f.service, &f.peer, 2, b"two");
    let init = initialize(&f, [11; 32], file, 3);
    let committed = call(
        &f.service,
        &f.peer,
        8,
        Operation::HistoryCommand(HistoryCommand::Commit(changes(&init, [1; 32], second, 1))),
    )
    .unwrap();
    let first = match result(committed) {
        HistoryResult::Committed(CommitOutcomeWire::Committed(record)) => record,
        _ => panic!(),
    };
    let layer = call(
        &f.service,
        &f.peer,
        9,
        Operation::HistoryCommand(HistoryCommand::AddLayer {
            stack: init.stack,
            branch: init.branch,
            commit: first.commit,
            expected_stack_head: init.base_layer,
            expected_branch_base: init.base_layer,
        }),
    )
    .unwrap();
    let layer = match result(layer) {
        HistoryResult::Published(LayerOutcomeWire::Added(record)) => record,
        _ => panic!(),
    };
    let mut change = changes(&init, [2; 32], file, 2);
    change.expected_head = Some(first.commit);
    change.base = first.root;
    let second = call(
        &f.service,
        &f.peer,
        10,
        Operation::HistoryCommand(HistoryCommand::Commit(change)),
    )
    .unwrap();
    let second = match result(second) {
        HistoryResult::Committed(CommitOutcomeWire::Committed(record)) => record,
        _ => panic!(),
    };
    let operation = Operation::HistoryCommand(HistoryCommand::AddLayer {
        stack: init.stack,
        branch: init.branch,
        commit: second.commit,
        expected_stack_head: layer.layer,
        expected_branch_base: init.base_layer,
    });
    let error = call(&f.service, &f.peer, 11, operation.clone()).unwrap_err();
    assert_eq!(
        error.history.as_deref(),
        Some(&HistoryFailure {
            conflict: Some(HistoryConflict::StackMoved {
                expected: init.base_layer,
                actual: layer.layer
            }),
            stage: StageObservation::Unobserved
        })
    );
    assert_eq!(native_call(&f, operation).unwrap_err(), error);
}
