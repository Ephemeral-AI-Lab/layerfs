//! Shared prepared-directory admission and native Service publication.
//! Allocation provenance is a caller precondition; Service checks scope/base absence.
use layerfs_bridge::{adapters::native::connection::VerifiedPeer, contract::*};
use layerfs_history::{sqlite, HistoryCatalogConfig};
use layerfs_service::{Grant, Service, StoreAccess};
use layerfs_storage::Store;
use layerfs_telemetry::{operation::OperationRecorder, timer::Timing};
use std::{io::Cursor, path::PathBuf, sync::Arc, time::SystemTime};

struct Fixture {
    service: Service,
    peer: VerifiedPeer,
    directory: PathBuf,
    snapshot: BranchSnapshotWire,
    next: u64,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.directory).unwrap();
    }
}

fn history(response: Response) -> HistoryResult {
    match response {
        Response::History(result) => *result,
        other => panic!("expected history: {other:?}"),
    }
}

fn request(operation: Operation) -> Request {
    Request {
        id: 1,
        generation: 1,
        store: 1,
        profile: if matches!(
            operation,
            Operation::HistoryQuery(_) | Operation::HistoryCommand(_)
        ) {
            HISTORY_PROFILE
        } else {
            1
        },
        deadline_ms: 10_000,
        response_bytes: 16_384,
        operation,
    }
}

impl Fixture {
    fn new(reserve_count: u64) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "layerfs-prepared-directories-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        std::fs::create_dir(&directory).unwrap();
        let store = Timing::disabled("create", |s| {
            Store::create(
                directory.join("store.sqlite"),
                Store::default_policy(),
                s.child("store"),
            )
        })
        .0
        .unwrap();
        let catalog = sqlite::create(
            &directory.join("history.sqlite"),
            &HistoryCatalogConfig {
                cursor_key: [71; 32],
                binding_key: b"prepared-directories".to_vec(),
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
        let file = match service
            .handle(
                &peer,
                &request(Operation::ConstructFile { length: 9 }),
                &mut Cursor::new(b"preserved"),
                &mut std::io::sink(),
            )
            .0
            .unwrap()
        {
            Response::Saved { root, .. } => root,
            other => panic!("{other:?}"),
        };
        let entry = |parent, name: &[u8], kind, content| ManifestEntry {
            parent,
            name: name.to_vec(),
            kind,
            mode: if kind == 2 { 0o755 } else { 0o644 },
            mtime_seconds: 17,
            mtime_nanoseconds: 19,
            content,
            target: vec![],
        };
        let initialized = history(
            service
                .handle(
                    &peer,
                    &request(Operation::HistoryCommand(HistoryCommand::InitLayerStack {
                        stack: [0x51; 16],
                        name: b"main".to_vec(),
                        scope_seed: [13; 32],
                        manifest: vec![
                            entry(0, b"", 2, None),
                            entry(0, b"file", 1, Some(file)),
                            entry(0, b"old", 2, None),
                        ],
                    })),
                    &mut std::io::empty(),
                    &mut std::io::sink(),
                )
                .0
                .unwrap(),
        );
        let created = match initialized {
            HistoryResult::StackCreated(created) => created,
            other => panic!("{other:?}"),
        };
        let snapshot = match history(
            service
                .handle(
                    &peer,
                    &request(Operation::HistoryCommand(HistoryCommand::Fork {
                        stack: created.stack.stack,
                        branch: [0x61; 16],
                        name: b"work".to_vec(),
                        source: HistoryForkSource::Layer(created.stack.head_layer),
                    })),
                    &mut std::io::empty(),
                    &mut std::io::sink(),
                )
                .0
                .unwrap(),
        ) {
            HistoryResult::BranchSnapshot(snapshot) => snapshot,
            other => panic!("{other:?}"),
        };
        let mut f = Self {
            service,
            peer,
            directory,
            snapshot,
            next: 0,
        };
        f.next = f.reserve(reserve_count, false);
        f
    }

    fn call(&self, operation: Operation, native: bool) -> Result<Response, Failure> {
        let request = request(operation);
        if !native {
            return self
                .service
                .handle(
                    &self.peer,
                    &request,
                    &mut std::io::empty(),
                    &mut std::io::sink(),
                )
                .0;
        }
        use layerfs_bridge::adapters::native::{
            client::Client,
            connection::{accept, connect, Peer},
            listen,
            server::serve,
        };
        let listener = listen("127.0.0.1:0".parse().unwrap()).unwrap();
        let address = listener.local_addr().unwrap();
        let server_key = [9; 32];
        let public = *VerifiedPeer::from_private(&server_key)
            .unwrap()
            .public_key();
        let peers = [Peer {
            selector: 1,
            public: *self.peer.public_key(),
            expires_unix: u64::MAX,
        }];
        std::thread::scope(|threads| {
            let server = threads.spawn(|| {
                let (socket, _) = listener.accept().unwrap();
                let connection = accept(socket, &server_key, &peers).unwrap();
                let _ = serve(connection, |peer, request, input, output, deadline| {
                    self.service
                        .handle_until(peer, request, input, output, deadline)
                        .0
                });
            });
            let mut client = Client::new(connect(address, 1, &[7; 32], &public).unwrap()).unwrap();
            let mut input: &[u8] = &[];
            let result = client.call(&request, &mut input, &mut std::io::sink());
            drop(client);
            server.join().unwrap();
            result
        })
    }

    fn reserve(&self, count: u64, native: bool) -> u64 {
        match history(
            self.call(
                Operation::HistoryCommand(HistoryCommand::ReserveInodes {
                    scope: self.snapshot.scope,
                    count,
                }),
                native,
            )
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

    fn changes(&self) -> PreparedChanges {
        PreparedChanges {
            workspace: [0x71; 32],
            branch: self.snapshot.branch.branch,
            expected_head: self.snapshot.branch.head_commit,
            expected_base: self.snapshot.branch.base_layer,
            generation: 1,
            base: self.snapshot.effective_root,
            scope: self.snapshot.scope,
            root_serial: 1,
            directories: vec![],
            inodes: vec![],
            new_directories: vec![],
            new_file_serials: Vec::new(),
            directory_metadata: vec![],
        }
    }

    fn nested(&self) -> PreparedChanges {
        let mut changes = self.changes();
        changes.directories = vec![
            directory(1, &[(b"new", Some(self.next))]),
            directory(self.next, &[(b"nested", Some(self.next + 1))]),
            directory(self.next + 1, &[]),
        ];
        changes.directory_metadata = vec![DirectoryMetadata {
            serial: 1,
            mode: 0o755,
            mtime_seconds: 29,
            mtime_nanoseconds: 31,
        }];
        changes.new_directories = vec![
            new_directory(self.next),
            DirectoryMetadata {
                serial: self.next + 1,
                mode: 0o700,
                mtime_seconds: 3,
                mtime_nanoseconds: 4,
            },
        ];
        changes
    }

    fn inspect(&self, root: Root, query: Inspect, native: bool) -> Result<Response, Failure> {
        self.call(Operation::Inspect { root, query }, native)
    }

    fn attributes(&self, root: Root, path: &[u8], native: bool) -> Response {
        self.inspect(
            root,
            Inspect::Attributes {
                path: path.to_vec(),
            },
            native,
        )
        .unwrap()
    }

    fn branch(&self, native: bool) -> Response {
        self.call(
            Operation::HistoryQuery(HistoryQuery::GetBranch {
                branch: self.snapshot.branch.branch,
            }),
            native,
        )
        .unwrap()
    }

    fn commits(&self, native: bool) -> Response {
        self.call(
            Operation::HistoryQuery(HistoryQuery::CommitHistory {
                branch: self.snapshot.branch.branch,
                start: None,
                cursor: vec![],
                limit: 8,
            }),
            native,
        )
        .unwrap()
    }

    fn stage(&self, native: bool) -> Result<Response, Failure> {
        self.call(
            Operation::HistoryQuery(HistoryQuery::GetStage {
                workspace: [0x71; 32],
            }),
            native,
        )
    }
}

fn directory(parent: u64, changes: &[(&[u8], Option<u64>)]) -> DirectoryChange {
    DirectoryChange {
        parent,
        changes: changes.iter().map(|(n, s)| (n.to_vec(), *s)).collect(),
    }
}

fn new_directory(serial: u64) -> DirectoryMetadata {
    DirectoryMetadata {
        serial,
        mode: 0o1777,
        mtime_seconds: -2,
        mtime_nanoseconds: 987_654_321,
    }
}

fn update(changes: &PreparedChanges) -> Operation {
    Operation::UpdatePreparedFilesystem {
        base: changes.base,
        scope: changes.scope,
        root_serial: changes.root_serial,
        directories: changes.directories.clone(),
        inodes: changes.inodes.clone(),
        new_directories: changes.new_directories.clone(),
        new_file_serials: changes.new_file_serials.clone(),
        directory_metadata: changes.directory_metadata.clone(),
    }
}

fn candidate(response: Response) -> Root {
    match response {
        Response::FilesystemSaved { root, .. } => root,
        other => panic!("{other:?}"),
    }
}

fn existing_inode(f: &Fixture) -> InodeChange {
    match f.attributes(f.snapshot.effective_root, b"file", false) {
        Response::Attributes {
            serial,
            kind,
            content,
            metadata,
            ..
        } => InodeChange {
            serial,
            kind,
            content,
            metadata,
        },
        other => panic!("{other:?}"),
    }
}

#[test]
fn reserved_nested_directories_save_then_commit_fresh_identities() {
    for native in [false, true] {
        let f = Fixture::new(4);
        let changes = f.nested();
        let old = f.snapshot.effective_root;
        let old_file = f.attributes(old, b"file", native);
        let old_directory = f.attributes(old, b"old", native);
        let old_root_attributes = f.attributes(old, b"", native);
        let branch_before = f.branch(native);
        let commits_before = f.commits(native);
        let root = candidate(f.call(update(&changes), native).unwrap());
        assert_ne!(root, old);
        assert!(matches!(
            f.attributes(root, b"", native),
            Response::Attributes {
                mode: 0o755,
                mtime: 29,
                nanoseconds: 31,
                ..
            }
        ));
        assert_eq!(f.attributes(old, b"", native), old_root_attributes);
        assert_eq!(f.branch(native), branch_before);
        assert_eq!(f.commits(native), commits_before);
        assert_eq!(f.stage(native).unwrap_err().code, Code::NotFound);
        for (path, declaration) in [
            (b"new".as_slice(), &changes.new_directories[0]),
            (b"new/nested".as_slice(), &changes.new_directories[1]),
        ] {
            match f.attributes(root, path, native) {
                Response::Attributes {
                    serial,
                    kind,
                    references,
                    mode,
                    mtime,
                    nanoseconds,
                    size,
                    ..
                } => {
                    assert_eq!(
                        (serial, kind, references, size),
                        (declaration.serial, 2, 1, 0)
                    );
                    assert_eq!(
                        (mode, mtime, nanoseconds),
                        (
                            declaration.mode,
                            declaration.mtime_seconds,
                            declaration.mtime_nanoseconds
                        )
                    );
                }
                other => panic!("{other:?}"),
            }
        }
        for (path, entries) in [
            (b"new".as_slice(), vec![(b"nested".to_vec(), f.next + 1)]),
            (b"new/nested".as_slice(), vec![]),
        ] {
            assert_eq!(
                f.inspect(
                    root,
                    Inspect::List {
                        path: path.to_vec(),
                        after: vec![],
                        entries: 8,
                        bytes: 1024
                    },
                    native
                )
                .unwrap(),
                Response::List {
                    entries,
                    continuation: None
                }
            );
        }
        assert_eq!(f.attributes(root, b"file", native), old_file);
        assert_eq!(f.attributes(root, b"old", native), old_directory);
        assert_eq!(
            f.inspect(
                old,
                Inspect::Stat {
                    path: b"new".to_vec()
                },
                native
            )
            .unwrap_err()
            .code,
            Code::PathNotFound
        );
        // C1's allocation precondition is literal: this second preparation uses
        // two fresh reserved identities, never the exposed candidate's serials.
        let mut committed_changes = changes.clone();
        for declaration in &mut committed_changes.new_directories {
            declaration.serial += 2;
        }
        for directory in &mut committed_changes.directories {
            if directory.parent >= f.next {
                directory.parent += 2;
            }
            for (_, serial) in &mut directory.changes {
                if let Some(serial) = serial {
                    *serial += 2;
                }
            }
        }
        let committed = match history(
            f.call(
                Operation::HistoryCommand(HistoryCommand::Commit(committed_changes)),
                native,
            )
            .unwrap(),
        ) {
            HistoryResult::Committed(CommitOutcomeWire::Committed(commit)) => commit,
            other => panic!("{other:?}"),
        };
        assert_ne!(
            committed.root, root,
            "different reserved identities change the canonical root"
        );
        for (path, declaration) in [
            (b"new".as_slice(), &changes.new_directories[0]),
            (b"new/nested".as_slice(), &changes.new_directories[1]),
        ] {
            match f.attributes(committed.root, path, native) {
                Response::Attributes {
                    serial,
                    kind,
                    references,
                    mode,
                    mtime,
                    nanoseconds,
                    size,
                    ..
                } => {
                    assert_eq!(
                        (serial, kind, references, size),
                        (declaration.serial + 2, 2, 1, 0)
                    );
                    assert_eq!(
                        (mode, mtime, nanoseconds),
                        (
                            declaration.mode,
                            declaration.mtime_seconds,
                            declaration.mtime_nanoseconds
                        )
                    );
                }
                other => panic!("{other:?}"),
            }
        }
        let candidate_attributes = f.attributes(root, b"new/nested", native);
        assert!(
            matches!(candidate_attributes, Response::Attributes { serial, .. } if serial == f.next + 1)
        );
        assert_eq!(f.attributes(committed.root, b"file", native), old_file);
        assert_eq!(f.attributes(committed.root, b"old", native), old_directory);
        assert_eq!(committed.parent, f.snapshot.branch.head_commit);
        assert_eq!(committed.base_layer, f.snapshot.branch.base_layer);
        match history(f.branch(native)) {
            HistoryResult::BranchSnapshot(snapshot) => {
                assert_eq!(snapshot.effective_root, committed.root);
                assert_eq!(snapshot.branch.head_commit, Some(committed.commit));
                assert_eq!(snapshot.branch.base_layer, f.snapshot.branch.base_layer);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            history(f.commits(native)),
            HistoryResult::Commits {
                continuation: vec![],
                records: vec![committed],
            }
        );
        assert_eq!(f.stage(native).unwrap_err().code, Code::NotFound);
        assert_eq!(f.attributes(old, b"file", native), old_file);
        assert_eq!(f.attributes(old, b"old", native), old_directory);
        assert_eq!(f.reserve(1, native), f.next + 4);
    }
}

#[test]
fn unreferenced_new_directories_are_omitted_and_the_combined_boundary_is_128() {
    for native in [false, true] {
        let f = Fixture::new(128);
        let mut changes = f.changes();
        changes.inodes.push(existing_inode(&f));
        changes.directory_metadata.push(DirectoryMetadata {
            serial: 1,
            mode: 0o755,
            mtime_seconds: 17,
            mtime_nanoseconds: 19,
        });
        for serial in f.next..f.next + 126 {
            changes.new_directories.push(new_directory(serial));
            changes.directories.push(directory(serial, &[]));
        }
        let before = f.branch(native);
        let root = candidate(f.call(update(&changes), native).unwrap());
        assert_eq!(root, f.snapshot.effective_root);
        assert_eq!(f.branch(native), before);
        let mut undeclared = f.changes();
        undeclared.directories = vec![directory(1, &[(b"missing", Some(f.next))])];
        assert_eq!(
            f.call(update(&undeclared), native).unwrap_err().code,
            Code::InvalidInput
        );
        changes.new_directories.push(new_directory(f.next + 126));
        changes.directories.push(directory(f.next + 126, &[]));
        assert_eq!(
            f.call(update(&changes), native).unwrap_err().code,
            Code::Capacity
        );
        assert_eq!(
            f.reserve(1, native),
            f.next + 128,
            "omission never refunds a reservation"
        );
    }
}

fn refusals(f: &Fixture) -> Vec<(&'static str, PreparedChanges, Code)> {
    let mut cases = Vec::new();
    let mut add = |name, changes| cases.push((name, changes, Code::InvalidInput));
    for (name, serial) in [
        ("zero", 0),
        ("above-limit", i64::MAX as u64 + 1),
        ("root", 1),
        ("existing-file", 2),
        ("existing-directory", 3),
    ] {
        let mut c = f.changes();
        c.new_directories = vec![new_directory(serial)];
        c.directories = vec![directory(serial, &[])];
        add(name, c);
    }
    let mut c = f.nested();
    c.new_directories[0].mode = 0o4755;
    add("mode", c);
    let mut c = f.nested();
    c.new_directories[0].mtime_nanoseconds = 1_000_000_000;
    add("nanos", c);
    let mut c = f.nested();
    c.new_directories.swap(0, 1);
    add("order", c);
    let mut c = f.nested();
    c.new_directories[1].serial = f.next;
    add("duplicate", c);
    let mut c = f.nested();
    c.directories.pop();
    add("missing-own-directory-row", c);
    let mut c = f.nested();
    c.new_directories.pop();
    add("undeclared-parent", c);
    let mut c = f.nested();
    c.directories[1].changes[0].1 = Some(f.next + 2);
    add("undeclared-child", c);
    let mut c = f.nested();
    let mut inode = existing_inode(f);
    inode.serial = f.next;
    c.inodes.push(inode);
    add("overlap", c);
    let mut c = f.nested();
    c.directories[2].changes.push((b"root".to_vec(), Some(1)));
    add("root-binding", c);
    let mut c = f.nested();
    c.directories[0]
        .changes
        .push((b"second".to_vec(), Some(f.next)));
    add("multiple-parents", c);
    let mut c = f.nested();
    c.directories.remove(0);
    c.directories[1]
        .changes
        .push((b"cycle".to_vec(), Some(f.next)));
    add("disconnected-two-directory-cycle", c);
    let mut c = f.nested();
    c.directories[1]
        .changes
        .push((b"self".to_vec(), Some(f.next)));
    add("self-cycle", c);
    let mut c = f.nested();
    c.directories[0].changes.push((b"new".to_vec(), None));
    add("duplicate-name", c);
    let mut c = f.nested();
    c.scope = [0xee; 32];
    add("scope", c);
    let mut c = f.nested();
    c.directory_metadata = vec![new_directory(f.next)];
    add("new-patch-overlap", c);
    let mut c = f.nested();
    c.directory_metadata = vec![new_directory(2)];
    add("patch-regular-file", c);
    let mut c = f.nested();
    c.directory_metadata = vec![new_directory(2)];
    c.inodes.push(existing_inode(f));
    add("existing-patch-overlap", c);
    let mut c = f.nested();
    c.directory_metadata = vec![new_directory(f.next + 2)];
    add("patch-absent", c);
    let mut c = f.nested();
    c.directory_metadata = vec![new_directory(1), new_directory(1)];
    add("patch-duplicate", c);
    let mut c = f.nested();
    c.directory_metadata = vec![new_directory(3), new_directory(1)];
    add("patch-order", c);
    let mut c = f.nested();
    c.directory_metadata[0].mode = 0o4755;
    add("patch-mode", c);
    let mut c = f.nested();
    c.directory_metadata[0].mtime_nanoseconds = 1_000_000_000;
    add("patch-nanos", c);
    cases
}

#[test]
fn shared_refusals_preserve_branch_history_stage_and_old_root() {
    for native in [false, true] {
        let f = Fixture::new(128);
        // A previously acknowledged stage must survive each refused preparation.
        f.call(
            Operation::HistoryCommand(HistoryCommand::StageChanges(f.changes())),
            native,
        )
        .unwrap();
        let branch = f.branch(native);
        let commits = f.commits(native);
        let stage = f.stage(native).unwrap();
        let file = f.attributes(f.snapshot.effective_root, b"file", native);
        for (name, changes, code) in refusals(&f) {
            for operation in [
                update(&changes),
                Operation::HistoryCommand(HistoryCommand::StageChanges(changes.clone())),
                Operation::HistoryCommand(HistoryCommand::Commit(changes)),
            ] {
                let failure = f.call(operation, native).unwrap_err();
                assert_eq!(failure.code, code, "{name}, native={native}");
                assert!(!failure.unknown, "{name}: {failure:?}");
                assert_eq!(failure.cleanup, None, "{name}");
                assert_eq!(f.branch(native), branch, "{name}");
                assert_eq!(f.commits(native), commits, "{name}");
                assert_eq!(f.stage(native).unwrap(), stage, "{name}");
                assert_eq!(
                    f.attributes(f.snapshot.effective_root, b"file", native),
                    file,
                    "{name}"
                );
            }
        }
        assert_eq!(
            candidate(f.call(update(&f.changes()), native).unwrap()),
            f.snapshot.effective_root
        );
        assert_eq!(f.reserve(1, native), f.next + 128);
    }
}

fn generic_metadata(store: &Store) -> Root {
    use layerfs_content::filesystem::attributes::{
        build::build_attribute_tree, value::emit_value, AttributeEntry, AttributeKey,
    };
    use layerfs_content::FilesystemObjects;
    use layerfs_storage::{SaveHandoff, StoreProvider};
    Timing::disabled("generic-metadata-fixture", |scope| {
        let mut save = store.begin_save(scope.child("begin")).unwrap();
        let provider = StoreProvider::new(store);
        let mut handoff = SaveHandoff::new(&mut save);
        let mut objects = FilesystemObjects::new(&provider, &mut handoff);
        let mode = emit_value(&mut objects, &0o755u32.to_be_bytes()).unwrap();
        let mut timestamp = 17i64.to_be_bytes().to_vec();
        timestamp.extend_from_slice(&19u32.to_be_bytes());
        let mtime = emit_value(&mut objects, &timestamp).unwrap();
        let opaque = emit_value(&mut objects, b"exact retained generic directory value").unwrap();
        let mut entries = vec![
            AttributeEntry {
                key: AttributeKey::new("portable".into(), b"mode".to_vec()).unwrap(),
                value_root: mode,
            },
            AttributeEntry {
                key: AttributeKey::new("portable".into(), b"mtime".to_vec()).unwrap(),
                value_root: mtime,
            },
        ];
        for index in 0..300 {
            entries.push(AttributeEntry {
                key: AttributeKey::new("user".into(), format!("key-{index:04}").into_bytes())
                    .unwrap(),
                value_root: opaque,
            });
        }
        let (root, _) = build_attribute_tree(&mut objects, entries.into_iter().map(Ok)).unwrap();
        assert!(handoff.take_failure().is_none());
        drop(handoff);
        save.finish(scope.child("finish")).unwrap();
        Ok::<_, layerfs_content::ContentError>(*root.as_bytes())
    })
    .0
    .unwrap()
}

fn metadata_root(response: &Response) -> Root {
    match response {
        Response::Attributes { metadata, .. } => *metadata,
        other => panic!("{other:?}"),
    }
}

fn generic_entries(store: &Store, root: Root) -> Vec<(Vec<u8>, Root)> {
    use layerfs_content::filesystem::attributes::patch::visit_keys;
    let mut entries = Vec::new();
    visit_keys(
        &layerfs_storage::StoreProvider::new(store),
        layerfs_content::ObjectId::from_bytes(&root).unwrap(),
        |key, value| {
            if key.domain() == "user" {
                entries.push((key.key().to_vec(), *value.as_bytes()));
            }
            Ok(())
        },
    )
    .unwrap();
    entries
}

#[test]
fn existing_root_and_directory_metadata_patch_preserves_generic_values_in_one_preparation() {
    for native in [false, true] {
        let mut f = Fixture::new(1);
        let store = Timing::disabled("fixture-open", |scope| {
            Store::open(f.directory.join("store.sqlite"), scope.child("open"))
        })
        .0
        .unwrap();
        let generic = generic_metadata(&store);
        let expected_generic = generic_entries(&store, generic);
        assert_eq!(expected_generic.len(), 300);
        // Fixture setup uses public C1/C2 for generic values, then an explicit
        // public C5 Commit to publish them. This is separate from the patch.
        let mut setup = f.changes();
        for path in [b"".as_slice(), b"old".as_slice()] {
            match f.attributes(f.snapshot.effective_root, path, native) {
                Response::Attributes {
                    serial,
                    kind,
                    content,
                    ..
                } => {
                    setup.inodes.push(InodeChange {
                        serial,
                        kind,
                        content,
                        metadata: generic,
                    });
                }
                other => panic!("{other:?}"),
            }
        }
        f.call(
            Operation::HistoryCommand(HistoryCommand::Commit(setup)),
            native,
        )
        .unwrap();
        f.snapshot = match history(f.branch(native)) {
            HistoryResult::BranchSnapshot(snapshot) => snapshot,
            other => panic!("{other:?}"),
        };
        let old_root = f.snapshot.effective_root;
        let before = f.branch(native);
        let before_history = f.commits(native);
        let old = [
            f.attributes(old_root, b"", native),
            f.attributes(old_root, b"old", native),
        ];
        let mut patch = f.changes();
        patch.generation = 2;
        patch.directory_metadata = vec![
            DirectoryMetadata {
                serial: 1,
                mode: 0o1777,
                mtime_seconds: -21,
                mtime_nanoseconds: 23,
            },
            DirectoryMetadata {
                serial: 3,
                mode: 0o711,
                mtime_seconds: 25,
                mtime_nanoseconds: 27,
            },
        ];
        assert!(
            patch.directories.is_empty()
                && patch.inodes.is_empty()
                && patch.new_directories.is_empty()
        );
        let root = candidate(f.call(update(&patch), native).unwrap());
        assert_ne!(root, old_root);
        assert_eq!(f.branch(native), before);
        assert_eq!(f.commits(native), before_history);
        for (index, path) in [b"".as_slice(), b"old".as_slice()].into_iter().enumerate() {
            let actual = f.attributes(root, path, native);
            let changed = metadata_root(&actual);
            assert_ne!(changed, generic);
            assert_eq!(generic_entries(&store, changed), expected_generic);
            let mut expected = old[index].clone();
            if let Response::Attributes {
                metadata,
                mode,
                mtime,
                nanoseconds,
                ..
            } = &mut expected
            {
                let declaration = &patch.directory_metadata[index];
                *metadata = changed;
                *mode = declaration.mode;
                *mtime = declaration.mtime_seconds;
                *nanoseconds = declaration.mtime_nanoseconds;
            }
            assert_eq!(
                actual, expected,
                "content, identity and references remain exact"
            );
            assert_eq!(f.attributes(old_root, path, native), old[index]);
        }
        match history(
            f.call(
                Operation::HistoryCommand(HistoryCommand::Commit(patch)),
                native,
            )
            .unwrap(),
        ) {
            HistoryResult::Committed(CommitOutcomeWire::Committed(commit)) => {
                assert_eq!(commit.root, root)
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(generic_entries(&store, generic), expected_generic);
        assert_eq!(f.stage(native).unwrap_err().code, Code::NotFound);
    }
}
