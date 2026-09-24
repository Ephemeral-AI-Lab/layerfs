//! Fresh symlink declarations through shared prepared saves and explicit C5 publication.
use layerfs_bridge::{adapters::native::connection::VerifiedPeer, contract::*};
use layerfs_history::{sqlite, HistoryCatalogConfig};
use layerfs_server::{Grant, Service, StoreAccess};
use layerfs_storage::Store;
use layerfs_telemetry::{operation::OperationRecorder, timer::Timing};
use std::{
    io::Cursor,
    path::PathBuf,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

const BYTES: &[u8] = b"../dangling/\xff";
struct Fixture {
    service: Service,
    peer: VerifiedPeer,
    path: PathBuf,
    snapshot: BranchSnapshotWire,
    empty: Root,
    file: Root,
    self_link: Root,
    next_self_link: Root,
    metadata: Root,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
fn request(operation: Operation) -> Request {
    Request {
        id: 1,
        generation: 1,
        store: 1,
        profile: if matches!(
            operation,
            Operation::HistoryCommand(_) | Operation::HistoryQuery(_)
        ) {
            2
        } else {
            1
        },
        deadline_ms: 10_000,
        response_bytes: if matches!(
            operation,
            Operation::ConstructPortableMetadata { .. } | Operation::ConstructSymlink { .. }
        ) {
            0
        } else {
            16384
        },
        operation,
    }
}
fn send(
    service: &Service,
    peer: &VerifiedPeer,
    operation: Operation,
    input: &[u8],
) -> Result<(Response, Vec<u8>), Failure> {
    let mut output = Vec::new();
    service
        .handle(
            peer,
            &request(operation),
            &mut Cursor::new(input),
            &mut output,
        )
        .0
        .map(|r| (r, output))
}
fn history(response: Response) -> HistoryResult {
    let Response::History(result) = response else {
        panic!("history")
    };
    *result
}
fn content(service: &Service, peer: &VerifiedPeer, bytes: &[u8]) -> Root {
    let (Response::Saved { root, length, .. }, output) = send(
        service,
        peer,
        Operation::ConstructFile {
            length: bytes.len() as u64,
        },
        bytes,
    )
    .unwrap() else {
        panic!("file")
    };
    assert_eq!(length, bytes.len() as u64);
    assert!(output.is_empty());
    root
}
fn symlink(service: &Service, peer: &VerifiedPeer, target: &[u8]) -> Root {
    let (Response::Saved { root, length, .. }, output) = send(
        service,
        peer,
        Operation::ConstructSymlink {
            target: target.to_vec(),
        },
        &[],
    )
    .unwrap() else {
        panic!("symlink");
    };
    assert_eq!(length, target.len() as u64);
    assert!(output.is_empty());
    root
}
impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "layerfs-prepared-symlinks-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        let store = Timing::disabled("create", |s| {
            Store::create(
                path.join("store.sqlite"),
                Store::default_policy(),
                s.child("store"),
            )
        })
        .0
        .unwrap();
        let catalog = sqlite::create(
            &path.join("history.sqlite"),
            &HistoryCatalogConfig {
                cursor_key: [71; 32],
                binding_key: b"prepared-symlinks".to_vec(),
                incarnation: 1,
            },
        )
        .unwrap();
        let peer = VerifiedPeer::from_private(&[7; 32]).unwrap();
        let service = Service::new(
            vec![StoreAccess {
                id: 1,
                store,
                history: Some(Arc::new(catalog)),
                grants: vec![Grant {
                    public_key: *peer.public_key(),
                    operations: 255,
                    expires_unix: u64::MAX,
                }],
            }],
            OperationRecorder::disabled(),
        )
        .unwrap();
        let old = content(&service, &peer, b"old-data");
        let empty = symlink(&service, &peer, &[]);
        let file = symlink(&service, &peer, BYTES);
        let self_link = symlink(&service, &peer, b"self");
        let next_self_link = symlink(&service, &peer, b"next-self");
        let metadata = match send(
            &service,
            &peer,
            Operation::ConstructPortableMetadata {
                kind: 3,
                mode: 0o777,
                mtime_seconds: -2,
                mtime_nanoseconds: 17,
            },
            &[],
        )
        .unwrap()
        .0
        {
            Response::MetadataConstructed { metadata, .. } => metadata,
            other => panic!("{other:?}"),
        };
        let created = match history(
            send(
                &service,
                &peer,
                Operation::HistoryCommand(HistoryCommand::InitLayerStack {
                    stack: [0x51; 16],
                    name: b"main".to_vec(),
                    scope_seed: [13; 32],
                    manifest: vec![
                        ManifestEntry {
                            parent: 0,
                            name: vec![],
                            kind: 2,
                            mode: 0o755,
                            mtime_seconds: 1,
                            mtime_nanoseconds: 2,
                            content: None,
                            target: vec![],
                        },
                        ManifestEntry {
                            parent: 0,
                            name: b"old".to_vec(),
                            kind: 1,
                            mode: 0o644,
                            mtime_seconds: 3,
                            mtime_nanoseconds: 4,
                            content: Some(old),
                            target: vec![],
                        },
                    ],
                }),
                &[],
            )
            .unwrap()
            .0,
        ) {
            HistoryResult::StackCreated(value) => value,
            other => panic!("{other:?}"),
        };
        let snapshot = match history(
            send(
                &service,
                &peer,
                Operation::HistoryCommand(HistoryCommand::Fork {
                    stack: created.stack.stack,
                    branch: [0x61; 16],
                    name: b"work".to_vec(),
                    source: HistoryForkSource::Layer(created.stack.head_layer),
                }),
                &[],
            )
            .unwrap()
            .0,
        ) {
            HistoryResult::BranchSnapshot(value) => value,
            other => panic!("{other:?}"),
        };
        Self {
            service,
            peer,
            path,
            snapshot,
            empty,
            file,
            self_link,
            next_self_link,
            metadata,
        }
    }
    fn call(&self, operation: Operation) -> Result<Response, Failure> {
        let (response, output) = send(&self.service, &self.peer, operation, &[])?;
        assert!(output.is_empty());
        Ok(response)
    }
    fn reserve(&self, count: u64) -> u64 {
        match history(
            self.call(Operation::HistoryCommand(HistoryCommand::ReserveInodes {
                scope: self.snapshot.scope,
                count,
            }))
            .unwrap(),
        ) {
            HistoryResult::Reservation {
                scope,
                start,
                count: actual,
            } => {
                assert_eq!(scope, self.snapshot.scope);
                assert_eq!(actual, count);
                start
            }
            other => panic!("{other:?}"),
        }
    }
    fn branch(&self) -> BranchSnapshotWire {
        match history(
            self.call(Operation::HistoryQuery(HistoryQuery::GetBranch {
                branch: self.snapshot.branch.branch,
            }))
            .unwrap(),
        ) {
            HistoryResult::BranchSnapshot(value) => value,
            other => panic!("{other:?}"),
        }
    }
    fn commits(&self) -> Response {
        self.call(Operation::HistoryQuery(HistoryQuery::CommitHistory {
            branch: self.snapshot.branch.branch,
            start: None,
            cursor: vec![],
            limit: 8,
        }))
        .unwrap()
    }
    fn stage(&self) -> Result<Response, Failure> {
        self.call(Operation::HistoryQuery(HistoryQuery::GetStage {
            workspace: [0x71; 32],
        }))
    }
    fn attributes(&self, root: Root, path: &[u8]) -> Result<Response, Failure> {
        self.call(Operation::Inspect {
            root,
            query: Inspect::Attributes {
                path: path.to_vec(),
            },
        })
    }
    fn changes(&self, first: u64, prefix: &str) -> PreparedChanges {
        let snapshot = self.branch();
        PreparedChanges {
            workspace: [0x71; 32],
            branch: snapshot.branch.branch,
            expected_head: snapshot.branch.head_commit,
            expected_base: snapshot.branch.base_layer,
            generation: 1,
            base: snapshot.effective_root,
            scope: snapshot.scope,
            root_serial: snapshot.root_serial.unwrap(),
            directories: vec![DirectoryChange {
                parent: snapshot.root_serial.unwrap(),
                changes: vec![
                    (format!("{prefix}empty").into_bytes(), Some(first)),
                    (format!("{prefix}file").into_bytes(), Some(first + 1)),
                    (format!("{prefix}self").into_bytes(), Some(first + 2)),
                ],
            }],
            inodes: vec![
                InodeChange {
                    serial: first,
                    kind: 3,
                    content: self.empty,
                    metadata: self.metadata,
                },
                InodeChange {
                    serial: first + 1,
                    kind: 3,
                    content: self.file,
                    metadata: self.metadata,
                },
                InodeChange {
                    serial: first + 2,
                    kind: 3,
                    content: if prefix.is_empty() {
                        self.self_link
                    } else {
                        self.next_self_link
                    },
                    metadata: self.metadata,
                },
            ],
            new_directories: vec![],
            directory_metadata: vec![],
            new_file_serials: vec![],
            new_symlink_serials: vec![first, first + 1, first + 2],
        }
    }
    fn verify(&self, root: Root, first: u64, prefix: &str) {
        let self_name = format!("{prefix}self");
        let self_root = if prefix.is_empty() {
            self.self_link
        } else {
            self.next_self_link
        };
        for (name, serial, target, content) in [
            ("empty", first, &[][..], self.empty),
            ("file", first + 1, BYTES, self.file),
            ("self", first + 2, self_name.as_bytes(), self_root),
        ] {
            let path = format!("{prefix}{name}").into_bytes();
            assert_eq!(
                self.attributes(root, &path).unwrap(),
                Response::Attributes {
                    serial,
                    kind: 3,
                    references: 1,
                    content,
                    metadata: self.metadata,
                    mode: 0o777,
                    mtime: -2,
                    nanoseconds: 17,
                    size: target.len() as u64
                }
            );
            assert_eq!(
                self.call(Operation::Inspect {
                    root,
                    query: Inspect::Readlink { path }
                })
                .unwrap(),
                Response::Link(target.to_vec())
            );
        }
    }
}

fn operation(route: u8, p: PreparedChanges) -> Operation {
    match route {
        0 => Operation::UpdatePreparedFilesystem {
            base: p.base,
            scope: p.scope,
            root_serial: p.root_serial,
            directories: p.directories,
            inodes: p.inodes,
            new_directories: p.new_directories,
            directory_metadata: p.directory_metadata,
            new_file_serials: p.new_file_serials,
            new_symlink_serials: p.new_symlink_serials,
        },
        1 => Operation::HistoryCommand(HistoryCommand::StageChanges(p)),
        2 => Operation::HistoryCommand(HistoryCommand::Commit(p)),
        _ => unreachable!(),
    }
}
fn committed(response: Response) -> CommitWire {
    match history(response) {
        HistoryResult::Committed(CommitOutcomeWire::Committed(commit)) => commit,
        other => panic!("{other:?}"),
    }
}

#[test]
fn fresh_empty_dangling_and_self_symlinks_use_distinct_reserved_ids() {
    let f = Fixture::new();
    let first = f.reserve(9);
    let before = f.branch();
    let old = f.attributes(before.effective_root, b"old").unwrap();
    let original_history = f.commits();
    let direct = match f.call(operation(0, f.changes(first, ""))).unwrap() {
        Response::FilesystemSaved { root, .. } => root,
        other => panic!("{other:?}"),
    };
    f.verify(direct, first, "");
    assert_eq!(f.branch(), before);
    assert_eq!(f.commits(), original_history);
    assert_eq!(f.stage().unwrap_err().code, Code::NotFound);
    let stage = match history(f.call(operation(1, f.changes(first + 3, ""))).unwrap()) {
        HistoryResult::Stage(stage) => stage,
        other => panic!("{other:?}"),
    };
    f.verify(stage.candidate_root, first + 3, "");
    assert_eq!(f.branch(), before);
    assert_eq!(f.commits(), original_history);
    assert_ne!(direct, stage.candidate_root);
    let one = committed(
        f.call(Operation::HistoryCommand(HistoryCommand::CommitStaged {
            workspace: stage.workspace,
            token: stage.token,
        }))
        .unwrap(),
    );
    assert_eq!(one.root, stage.candidate_root);
    assert_eq!(one.parent, before.branch.head_commit);
    assert_eq!(f.stage().unwrap_err().code, Code::NotFound);
    let two = committed(f.call(operation(2, f.changes(first + 6, "next-"))).unwrap());
    assert_eq!(two.parent, Some(one.commit));
    assert_eq!(f.branch().effective_root, two.root);
    f.verify(two.root, first + 6, "next-");
    f.verify(two.root, first + 3, "");
    f.verify(direct, first, "");
    assert_eq!(
        history(f.commits()),
        HistoryResult::Commits {
            continuation: vec![],
            records: vec![two.clone(), one]
        }
    );
    assert_eq!(
        f.attributes(before.effective_root, b"empty")
            .unwrap_err()
            .code,
        Code::PathNotFound
    );
    for root in [
        before.effective_root,
        direct,
        stage.candidate_root,
        two.root,
    ] {
        assert_eq!(f.attributes(root, b"old").unwrap(), old);
    }
    assert_eq!(
        f.reserve(1),
        first + 9,
        "prepared declarations allocate no additional serials"
    );
    // One direct mixed F+S save, independently reserved after the probe above.
    let mixed = f.reserve(2);
    assert_eq!(mixed, first + 10);
    let mixed_before = f.branch();
    let mixed_history = f.commits();
    let Response::Attributes {
        content, metadata, ..
    } = old
    else {
        panic!("old file attributes");
    };
    let mut p = f.changes(mixed, "");
    p.directories[0].changes = vec![
        (b"mixed-file".to_vec(), Some(mixed)),
        (b"mixed-link".to_vec(), Some(mixed + 1)),
    ];
    p.inodes = vec![
        InodeChange {
            serial: mixed,
            kind: 1,
            content,
            metadata,
        },
        InodeChange {
            serial: mixed + 1,
            kind: 3,
            content: f.file,
            metadata: f.metadata,
        },
    ];
    p.new_file_serials = vec![mixed];
    p.new_symlink_serials = vec![mixed + 1];
    let mixed_root = match f.call(operation(0, p)).unwrap() {
        Response::FilesystemSaved { root, .. } => root,
        other => panic!("{other:?}"),
    };
    let mut expected_file = f.attributes(before.effective_root, b"old").unwrap();
    if let Response::Attributes {
        serial, references, ..
    } = &mut expected_file
    {
        *serial = mixed;
        *references = 1;
    }
    assert_eq!(
        f.attributes(mixed_root, b"mixed-file").unwrap(),
        expected_file
    );
    let (response, bytes) = send(
        &f.service,
        &f.peer,
        Operation::ReadFile {
            root: content,
            start: 0,
            end: 8,
        },
        &[],
    )
    .unwrap();
    assert_eq!(response, Response::Read { length: 8 });
    assert_eq!(bytes, b"old-data");
    assert_eq!(
        f.attributes(mixed_root, b"mixed-link").unwrap(),
        Response::Attributes {
            serial: mixed + 1,
            kind: 3,
            references: 1,
            content: f.file,
            metadata: f.metadata,
            mode: 0o777,
            mtime: -2,
            nanoseconds: 17,
            size: BYTES.len() as u64,
        }
    );
    assert_eq!(
        f.call(Operation::Inspect {
            root: mixed_root,
            query: Inspect::Readlink {
                path: b"mixed-link".to_vec()
            }
        })
        .unwrap(),
        Response::Link(BYTES.to_vec())
    );
    assert_eq!(f.branch(), mixed_before);
    assert_eq!(f.commits(), mixed_history);
    for path in [b"mixed-file".as_slice(), b"mixed-link".as_slice()] {
        assert_eq!(
            f.attributes(mixed_before.effective_root, path)
                .unwrap_err()
                .code,
            Code::PathNotFound
        );
    }
}

#[test]
fn invalid_fresh_symlink_roles_binding_and_parent_preserve_prior_stage() {
    let f = Fixture::new();
    let first = f.reserve(3);
    let before = f.branch();
    let old_history = f.commits();
    let mut no_change = f.changes(first, "");
    no_change.directories.clear();
    no_change.inodes.clear();
    no_change.new_symlink_serials.clear();
    let stage = f.call(operation(1, no_change)).unwrap();
    let directory_content = match f.attributes(before.effective_root, b"").unwrap() {
        Response::Attributes { content, .. } => content,
        other => panic!("{other:?}"),
    };
    for case in 0..18 {
        let mut p = f.changes(first, "");
        let mut code = Code::InvalidInput;
        match case {
            0 => p.new_symlink_serials[0] = 0,
            1 => p.new_symlink_serials[0] = i64::MAX as u64 + 1,
            2 => p.new_symlink_serials[0] = 1,
            3 => p.new_symlink_serials[0] = first + 99,
            4 => p.new_symlink_serials.reverse(),
            5 => p.new_symlink_serials[1] = first,
            6 => p.inodes[0].kind = 1,
            7 => {
                p.inodes[0].serial = 2;
                p.new_symlink_serials[0] = 2;
                p.directories[0].changes[0].1 = Some(2);
            }
            8 => p.inodes[0].content = directory_content,
            9 => p.inodes[0].metadata = f.file,
            10 => {
                p.inodes[0].content = [0xee; 32];
                code = Code::MissingObject;
            }
            11 => {
                p.inodes[0].metadata = [0xee; 32];
                code = Code::MissingObject;
            }
            12 => p.directories.clear(),
            13 => p.scope = [0xee; 32],
            14 => p.new_symlink_serials.clear(),
            15 => p.directories[0]
                .changes
                .push((b"zalias".to_vec(), Some(first))),
            16 => p.directories.push(DirectoryChange {
                parent: first,
                changes: vec![(b"child".to_vec(), Some(first + 1))],
            }),
            _ => {
                p.new_symlink_serials.pop();
                p.new_file_serials.push(first);
            }
        }
        for route in 0..3 {
            let error = f.call(operation(route, p.clone())).unwrap_err();
            assert_eq!(error.code, code, "{route}/{case}: {error:?}");
            assert!(!error.unknown);
            assert_eq!(error.cleanup, None);
            assert_eq!(f.branch(), before);
            assert_eq!(f.commits(), old_history);
            assert_eq!(f.stage().unwrap(), stage);
        }
    }
    assert_eq!(f.reserve(1), first + 3);
    assert_eq!(
        f.attributes(before.effective_root, b"empty")
            .unwrap_err()
            .code,
        Code::PathNotFound
    );
}
