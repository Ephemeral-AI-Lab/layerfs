//! Typed C5 admission under real occupied C2 Save slots (#287 R1a).
//!
//! Every requested operation enters the public authorized handler with real
//! Store/catalog providers. Held prepared sources plus persisted active-slot
//! rows establish overlap; a SaveFile spooling barrier would not establish it.
//! R1c additionally observes two admitted native scratch owners before their
//! first body reads, and checked removal before known stage responses.
use layerfs_bridge::{adapters::native::connection::VerifiedPeer, contract::*};
use layerfs_history::{sqlite, HistoryCatalog, HistoryCatalogConfig};
use layerfs_server::{Grant, Service, StoreAccess};
use layerfs_storage::Store;
use layerfs_telemetry::{operation::OperationRecorder, timer::Timing};
use std::{
    fmt::Write as FmtWrite,
    io::{self, BufRead, BufReader, Cursor, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[allow(dead_code)]
#[path = "support/file_save.rs"]
mod file_save;
#[path = "support/catalog_oracle.rs"]
mod oracle;

const ALL: u8 = 0xff;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(
        String::with_capacity(bytes.len() * 2),
        |mut output, byte| {
            write!(&mut output, "{byte:02x}").unwrap();
            output
        },
    )
}

struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn directory(name: &str) -> Temp {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "layerfs-catalog-admission-{name}-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&path).unwrap();
    Temp(path)
}

fn peer() -> VerifiedPeer {
    VerifiedPeer::from_private(&[7; 32]).unwrap()
}

fn store(path: &Path) -> Store {
    let store = Timing::disabled("create", |scope| {
        Store::create(path, Store::default_policy(), scope.child("create"))
    })
    .0
    .unwrap();
    assert_eq!(store.max_concurrent_writes().unwrap(), 2);
    store
}

fn catalog(path: &Path) -> Arc<dyn HistoryCatalog> {
    Arc::new(
        sqlite::create(
            path,
            &HistoryCatalogConfig {
                cursor_key: [71; 32],
                binding_key: b"layerfs-admission-authority".to_vec(),
                incarnation: 1,
            },
        )
        .unwrap(),
    )
}

fn access(id: u32, store: Store, catalog: Arc<dyn HistoryCatalog>, operations: u8) -> StoreAccess {
    StoreAccess {
        id,
        store,
        history: Some(catalog),
        grants: vec![Grant {
            public_key: *peer().public_key(),
            operations,
            expires_unix: u64::MAX,
        }],
    }
}

fn request(id: u64, store: u32, operation: Operation) -> Request {
    Request {
        id,
        generation: 1,
        store,
        profile: if matches!(
            operation,
            Operation::HistoryQuery(_) | Operation::HistoryCommand(_)
        ) {
            HISTORY_PROFILE
        } else {
            1
        },
        deadline_ms: 30_000,
        response_bytes: if matches!(
            operation,
            Operation::SaveFile { .. } | Operation::SaveFileV2 { .. }
        ) {
            0
        } else {
            MAX_FILE
        },
        operation,
    }
}

fn call(service: &Service, id: u64, store: u32, operation: Operation) -> Result<Response, Failure> {
    service
        .handle(
            &peer(),
            &request(id, store, operation),
            &mut io::empty(),
            &mut io::sink(),
        )
        .0
}

fn history(response: Response) -> HistoryResult {
    match response {
        Response::History(result) => *result,
        other => panic!("expected history, got {other:?}"),
    }
}

fn reserve(service: &Service, id: u64, store: u32, scope: Root, count: u64) -> (u64, u64) {
    match history(
        call(
            service,
            id,
            store,
            Operation::HistoryCommand(HistoryCommand::ReserveInodes { scope, count }),
        )
        .unwrap(),
    ) {
        HistoryResult::Reservation {
            scope: actual,
            start,
            count,
        } => {
            assert_eq!(actual, scope);
            (start, count)
        }
        other => panic!("expected reservation, got {other:?}"),
    }
}

/// Provider state observed externally; no product visibility or hook is added.
fn sql_number(path: &Path, sql: &str, parameter: Option<Root>) -> u64 {
    let mut command = Command::new("python3");
    command.args(["-c", "import sqlite3,sys\nc=sqlite3.connect('file:'+sys.argv[1]+'?mode=ro',uri=True,timeout=0)\np=() if len(sys.argv)==3 else (bytes.fromhex(sys.argv[3]),)\nr=c.execute(sys.argv[2],p).fetchone()\nprint(0 if r is None else r[0])"])
        .arg(path).arg(sql);
    if let Some(parameter) = parameter {
        command.arg(hex(&parameter));
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

fn active(path: &Path) -> u64 {
    sql_number(
        path,
        "SELECT count(*) FROM saves WHERE active_slot IS NOT NULL",
        None,
    )
}

fn highwater(path: &Path, scope: Root) -> u64 {
    sql_number(
        path,
        "SELECT highwater FROM scope_allocator WHERE scope_id=?",
        Some(scope),
    )
}

/// Observe genuine scratch owners through native metadata and independent SQL.
/// The observer runs only while prepared-body gates keep both producers idle.
fn scratch_files(parent: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(parent).unwrap() {
        let entry = entry.unwrap();
        if !entry.file_name().to_string_lossy().starts_with(".lfcs-") {
            continue;
        }
        let metadata = std::fs::symlink_metadata(entry.path()).unwrap();
        assert!(metadata.is_dir());
        assert!(!metadata.file_type().is_symlink());
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_eq!(metadata.mode() & 0o777, 0o700);
            assert_eq!(metadata.uid(), std::fs::metadata(parent).unwrap().uid());
        }
        for file in std::fs::read_dir(entry.path()).unwrap() {
            let path = file.unwrap().path();
            assert_eq!(path.extension().unwrap(), "sqlite");
            let metadata = std::fs::symlink_metadata(&path).unwrap();
            assert!(metadata.is_file());
            assert!(!metadata.file_type().is_symlink());
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                assert_eq!(metadata.mode() & 0o777, 0o600);
                assert_eq!(metadata.uid(), std::fs::metadata(parent).unwrap().uid());
                assert_eq!(metadata.nlink(), 1);
                assert_eq!(metadata.blocks() * 512, 16 * 1024 * 1024);
            }
            assert_eq!(
                sql_number(&path, "PRAGMA application_id", None),
                0x4c46_4353
            );
            assert_eq!(sql_number(&path, "PRAGMA user_version", None), 8);
            assert_eq!(
                sql_number(&path, "SELECT length(header) FROM session_owner", None),
                386
            );
            assert_eq!(
                sql_number(&path, "SELECT count(*) FROM directory_roots", None),
                0
            );
            files.push(path);
        }
    }
    files.sort();
    files
}

struct Fixture {
    temp: Temp,
    service: Arc<Service>,
    catalog: Arc<dyn HistoryCatalog>,
    store_path: PathBuf,
    catalog_path: PathBuf,
    snapshot: BranchSnapshotWire,
    file_content: Root,
    file_metadata: Root,
    root_metadata: Root,
}

fn fixture(name: &str) -> Fixture {
    fixture_at(directory(name))
}

fn fixture_at(temp: Temp) -> Fixture {
    // Existing independently sealed v1 whole-file identity, also preserved by
    // the published R0 oracle. Candidate output does not define this pin.
    let fixed = hex(&oracle::file(b"layerfs-stage02-fixture"));
    assert_eq!(
        fixed,
        "81bb74372b166542e1547b1a03c6637b72dc8dacb08f63489de0fe0fab1dcfa3"
    );
    let store_path = temp.0.join("store.sqlite");
    let catalog_path = temp.0.join("history.sqlite");
    let catalog = catalog(&catalog_path);
    let mut service = Service::new(
        vec![access(1, store(&store_path), Arc::clone(&catalog), ALL)],
        OperationRecorder::disabled(),
    )
    .unwrap();
    let source = temp.0.join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("a"), b"original-admission-bytes").unwrap();
    service.set_import_root(&source).unwrap();
    let created = match history(
        call(
            &service,
            1,
            1,
            Operation::HistoryCommand(HistoryCommand::ImportNativeDirectory {
                stack: [31; 16],
                name: b"main".to_vec(),
                scope_seed: [51; 32],
            }),
        )
        .unwrap(),
    ) {
        HistoryResult::StackCreated(created) => created,
        other => panic!("{other:?}"),
    };
    assert_eq!(created.root_serial, 1);
    let snapshot = match history(
        call(
            &service,
            2,
            1,
            Operation::HistoryCommand(HistoryCommand::Fork {
                stack: created.stack.stack,
                branch: [61; 16],
                name: b"work".to_vec(),
                source: HistoryForkSource::Layer(created.stack.head_layer),
            }),
        )
        .unwrap(),
    ) {
        HistoryResult::BranchSnapshot(snapshot) => snapshot,
        other => panic!("{other:?}"),
    };
    let stat = |path: &[u8]| {
        call(
            &service,
            3,
            1,
            Operation::Inspect {
                root: created.root,
                query: Inspect::Stat {
                    path: path.to_vec(),
                },
            },
        )
        .unwrap()
    };
    let (file_content, file_metadata) = match stat(b"a") {
        Response::Stat {
            serial: 2,
            kind: 1,
            content,
            metadata,
            ..
        } => (content, metadata),
        other => panic!("{other:?}"),
    };
    assert_eq!(file_content, oracle::file(b"original-admission-bytes"));
    let root_metadata = match stat(b"") {
        Response::Stat {
            serial: 1,
            kind: 2,
            metadata,
            ..
        } => metadata,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        created.root,
        oracle::namespace(
            snapshot.scope,
            root_metadata,
            &[oracle::File {
                name: b"a",
                serial: 2,
                content: file_content,
                metadata: file_metadata,
            }]
        )
    );
    Fixture {
        temp,
        service: Arc::new(service),
        catalog,
        store_path,
        catalog_path,
        snapshot,
        file_content,
        file_metadata,
        root_metadata,
    }
}

fn saved_file(service: &Service, id: u64, bytes: &[u8]) -> Root {
    let result = service
        .handle(
            &peer(),
            &request(id, 1, file_save::fresh(bytes.len() as u64)),
            &mut Cursor::new(file_save::fresh_body(bytes)),
            &mut io::sink(),
        )
        .0
        .unwrap();
    let Response::Saved { root, length, .. } = result else {
        panic!("{result:?}")
    };
    assert_eq!(length, bytes.len() as u64);
    assert_eq!(root, oracle::file(bytes));
    root
}

fn prepared(
    f: &Fixture,
    workspace: [u8; 32],
    identities: &[(u64, Root, bool)],
    names: &[(Vec<u8>, Option<u64>)],
) -> (Request, Vec<u8>) {
    let mut body = Vec::new();
    put_stream_tag(&mut body).unwrap();
    put_directory_row(&mut body, 1, names).unwrap();
    for (serial, content, fresh) in identities {
        put_rooted_identity(
            &mut body,
            if *fresh {
                ROLE_FRESH_FILE
            } else {
                ROLE_EXISTING_FILE
            },
            *serial,
            content,
            &f.file_metadata,
        )
        .unwrap();
    }
    let changes = PreparedChanges {
        workspace,
        branch: f.snapshot.branch.branch,
        expected_head: f.snapshot.branch.head_commit,
        expected_base: f.snapshot.branch.base_layer,
        generation: 1,
        base: f.snapshot.effective_root,
        scope: f.snapshot.scope,
        root_serial: 1,
        totals: PreparedTotals {
            directories: 1,
            identities: identities.len() as u64,
            names: names.len() as u64,
            name_bytes: names.iter().map(|(name, _)| 10 + name.len() as u64).sum(),
            fresh: identities.iter().filter(|row| row.2).count() as u64,
            ..PreparedTotals::default()
        },
    };
    assert_eq!(changes.stream_bytes().unwrap(), body.len() as u64);
    (
        request(
            u64::from(workspace[0]),
            1,
            Operation::HistoryCommand(HistoryCommand::StageChanges(changes)),
        ),
        body,
    )
}

/// A real source barrier, reached only after the Stage handler begins its Save.
struct SourceGate {
    opened: Sender<()>,
    release: Receiver<()>,
    waiting: bool,
    body: Cursor<Vec<u8>>,
}
impl Read for SourceGate {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if self.waiting {
            self.waiting = false;
            self.opened.send(()).unwrap();
            self.release
                .recv_timeout(Duration::from_secs(30))
                .map_err(|error| io::Error::new(io::ErrorKind::TimedOut, error))?;
        }
        self.body.read(bytes)
    }
}

struct Held {
    release: Sender<()>,
    worker: Option<std::thread::JoinHandle<Result<Response, Failure>>>,
}
impl Held {
    fn finish_result(mut self) -> Result<Response, Failure> {
        self.release.send(()).unwrap();
        self.worker.take().unwrap().join().unwrap()
    }

    fn finish(self) -> Response {
        self.finish_result().unwrap()
    }
}
impl Drop for Held {
    fn drop(&mut self) {
        let _ = self.release.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn hold(service: &Arc<Service>, request: Request, body: Vec<u8>) -> Held {
    let (opened, arrived) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let service = Arc::clone(service);
    let worker = std::thread::spawn(move || {
        service
            .handle(
                &peer(),
                &request,
                &mut SourceGate {
                    opened,
                    release: held,
                    waiting: true,
                    body: Cursor::new(body),
                },
                &mut io::sink(),
            )
            .0
    });
    let held = Held {
        release,
        worker: Some(worker),
    };
    arrived
        .recv_timeout(Duration::from_secs(30))
        .expect("source reached its body read");
    held
}

struct Unread;
impl Read for Unread {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        panic!("admission refusal must precede input consumption")
    }
}

fn stage(response: Response) -> StageWire {
    match history(response) {
        HistoryResult::Stage(stage) => stage,
        other => panic!("{other:?}"),
    }
}

fn read_bytes(service: &Service, root: Root, expected: &[u8]) {
    let mut output = Vec::new();
    let response = service
        .handle(
            &peer(),
            &request(
                99,
                1,
                Operation::ReadFile {
                    root,
                    start: 0,
                    end: expected.len() as u64,
                },
            ),
            &mut io::empty(),
            &mut output,
        )
        .0
        .unwrap();
    assert_eq!(
        response,
        Response::Read {
            length: expected.len() as u64
        }
    );
    assert_eq!(output, expected);
}

#[test]
fn allocator_finishes_with_two_real_save_slots_occupied_and_a_third_save_is_refused() {
    let f = fixture("occupied");
    let changed = saved_file(&f.service, 4, b"known-new-admission-bytes");
    // Use the proposed profile's real range width, without shrinking it to
    // force a refill. This proves catalog RPC progress, not a Workspace range
    // lease: only serials 3/4 enter this small prepared namespace and every
    // other reserved serial remains burned by the real C5 allocator.
    let initial = reserve(&f.service, 5, 1, f.snapshot.scope, 4096);
    assert_eq!(initial, (3, 4096));
    assert_eq!(highwater(&f.catalog_path, f.snapshot.scope), 4098);
    assert!(scratch_files(&f.temp.0).is_empty());
    let expected_first = oracle::namespace(
        f.snapshot.scope,
        f.root_metadata,
        &[
            oracle::File {
                name: b"a",
                serial: 2,
                content: f.file_content,
                metadata: f.file_metadata,
            },
            oracle::File {
                name: b"b",
                serial: 3,
                content: changed,
                metadata: f.file_metadata,
            },
            oracle::File {
                name: b"c",
                serial: 4,
                content: changed,
                metadata: f.file_metadata,
            },
        ],
    );
    let expected_second = oracle::namespace(
        f.snapshot.scope,
        f.root_metadata,
        &[oracle::File {
            name: b"a",
            serial: 2,
            content: changed,
            metadata: f.file_metadata,
        }],
    );
    let (first, body) = prepared(
        &f,
        [81; 32],
        &[(3, changed, true), (4, changed, true)],
        &[(b"b".to_vec(), Some(3)), (b"c".to_vec(), Some(4))],
    );
    let first = hold(&f.service, first, body);
    let (second, body) = prepared(&f, [82; 32], &[(2, changed, false)], &[]);
    let second = hold(&f.service, second, body);
    assert_eq!(
        active(&f.store_path),
        2,
        "both actual Save owners are persisted"
    );
    let occupied_scratch = scratch_files(&f.temp.0);
    assert_eq!(occupied_scratch.len(), 2);
    let saves = sql_number(&f.store_path, "SELECT count(*) FROM saves", None);
    // The successful response and committed high-water are observed before
    // either source is released, not inferred from a launched request.
    assert_eq!(
        reserve(&f.service, 6, 1, f.snapshot.scope, 4096),
        (4099, 4096)
    );
    assert_eq!(highwater(&f.catalog_path, f.snapshot.scope), 8194);
    assert_eq!(active(&f.store_path), 2);
    assert_eq!(scratch_files(&f.temp.0), occupied_scratch);
    assert_eq!(
        sql_number(&f.store_path, "SELECT count(*) FROM saves", None),
        saves
    );
    let refusal = f
        .service
        .handle(
            &peer(),
            &request(7, 1, file_save::fresh(64)),
            &mut Unread,
            &mut io::sink(),
        )
        .0
        .unwrap_err();
    assert_eq!(refusal, Failure::from(Code::Capacity));
    assert_eq!(active(&f.store_path), 2);
    assert_eq!(scratch_files(&f.temp.0), occupied_scratch);
    assert_eq!(
        sql_number(&f.store_path, "SELECT count(*) FROM saves", None),
        saves
    );
    let observed = call(
        &f.service,
        8,
        1,
        Operation::HistoryQuery(HistoryQuery::GetBranch {
            branch: f.snapshot.branch.branch,
        }),
    )
    .unwrap();
    let HistoryResult::BranchSnapshot(observed) = history(observed) else {
        panic!("branch snapshot")
    };
    assert_eq!(observed.effective_root, f.snapshot.effective_root);
    let first = stage(first.finish());
    assert_eq!(
        scratch_files(&f.temp.0),
        occupied_scratch,
        "first known scratch stays charged Idle while the second is active"
    );
    let second = stage(second.finish());
    assert_eq!(scratch_files(&f.temp.0), occupied_scratch);
    assert_eq!(f.service.drain_construction_idle().unwrap(), 2);
    assert_eq!(f.service.drain_construction_idle().unwrap(), 0);
    assert!(scratch_files(&f.temp.0).is_empty());
    for (stage, expected, workspace) in [
        (&first, expected_first, [81; 32]),
        (&second, expected_second, [82; 32]),
    ] {
        assert_eq!(stage.workspace, workspace);
        assert_eq!(stage.candidate_root, expected);
        assert_eq!(stage.expected_root, f.snapshot.effective_root);
        assert_eq!(stage.construction_base_root, f.snapshot.effective_root);
        assert_eq!(stage.expected_head, None);
        assert_eq!(stage.expected_base, f.snapshot.branch.base_layer);
        assert_eq!(stage.intended_commit_base, f.snapshot.branch.base_layer);
        assert_eq!(stage.scope, f.snapshot.scope);
        assert_eq!(stage.branch, f.snapshot.branch.branch);
        let queried = call(
            &f.service,
            9,
            1,
            Operation::HistoryQuery(HistoryQuery::GetStage { workspace }),
        )
        .unwrap();
        assert_eq!(history(queried), HistoryResult::Stage(stage.clone()));
    }
    assert_ne!(first.token, second.token);
    assert_eq!(active(&f.store_path), 0);
    read_bytes(&f.service, f.file_content, b"original-admission-bytes");
    read_bytes(&f.service, changed, b"known-new-admission-bytes");
    assert_eq!(highwater(&f.catalog_path, f.snapshot.scope), 8194);
    for stage in [first, second] {
        assert_eq!(
            history(
                call(
                    &f.service,
                    10,
                    1,
                    Operation::HistoryCommand(HistoryCommand::DiscardStage {
                        workspace: stage.workspace,
                        token: stage.token,
                    })
                )
                .unwrap()
            ),
            HistoryResult::Discarded { removed: true }
        );
    }
    assert_eq!(active(&f.store_path), 0);
    assert_eq!(
        highwater(&f.catalog_path, f.snapshot.scope),
        8194,
        "discard cannot refund exposed serials"
    );
}

#[test]
fn store_aliases_share_save_capacity_and_one_actual_scratch_domain() {
    let f = fixture("store-aliases");
    let changed = saved_file(&f.service, 120, b"known-alias-bytes");
    let opened = Timing::disabled("open", |scope| {
        Store::open(&f.store_path, scope.child("open"))
    })
    .0
    .unwrap();
    let linked_path = f.temp.0.join("store-hardlink.sqlite");
    std::fs::hard_link(&f.store_path, &linked_path).unwrap();
    let linked = Timing::disabled("link", |scope| {
        Store::open(&linked_path, scope.child("open"))
    })
    .0
    .unwrap();
    assert!(opened.same_authority(&opened.clone()));
    assert!(opened.same_authority(&linked));
    let other = store(&f.temp.0.join("other.sqlite"));
    assert!(!opened.same_authority(&other));
    let service = Arc::new(
        Service::new(
            vec![
                access(1, opened.clone(), Arc::clone(&f.catalog), ALL),
                access(2, linked, Arc::clone(&f.catalog), ALL),
                access(3, opened, Arc::clone(&f.catalog), ALL),
            ],
            OperationRecorder::disabled(),
        )
        .unwrap(),
    );
    let (first, body) = prepared(&f, [121; 32], &[(2, changed, false)], &[]);
    let first = hold(&service, first, body);
    let (mut second, body) = prepared(&f, [122; 32], &[(2, changed, false)], &[]);
    second.store = 2;
    let second = hold(&service, second, body);
    assert_eq!(active(&f.store_path), 2);
    let scratch = scratch_files(&f.temp.0);
    assert_eq!(scratch.len(), 2);
    assert_eq!(
        scratch[0].parent(),
        scratch[1].parent(),
        "same actual scratch authority"
    );
    let (mut third, _) = prepared(&f, [123; 32], &[(2, changed, false)], &[]);
    third.store = 3;
    let error = service
        .handle(&peer(), &third, &mut Unread, &mut io::sink())
        .0
        .unwrap_err();
    assert_eq!(error, Failure::from(Code::Capacity));
    assert_eq!(scratch_files(&f.temp.0), scratch);
    assert_eq!(active(&f.store_path), 2);
    let expected = oracle::namespace(
        f.snapshot.scope,
        f.root_metadata,
        &[oracle::File {
            name: b"a",
            serial: 2,
            content: changed,
            metadata: f.file_metadata,
        }],
    );
    for (result, workspace) in [(first.finish(), [121; 32]), (second.finish(), [122; 32])] {
        let value = stage(result);
        assert_eq!(value.workspace, workspace);
        assert_eq!(value.candidate_root, expected);
        assert_eq!(value.expected_root, f.snapshot.effective_root);
        assert_eq!(
            history(
                call(
                    &service,
                    124,
                    1,
                    Operation::HistoryCommand(HistoryCommand::DiscardStage {
                        workspace: value.workspace,
                        token: value.token,
                    })
                )
                .unwrap()
            ),
            HistoryResult::Discarded { removed: true }
        );
    }
    assert_eq!(active(&f.store_path), 0);
    assert_eq!(scratch_files(&f.temp.0), scratch);
    assert_eq!(service.drain_construction_idle().unwrap(), 2);
    assert_eq!(service.drain_construction_idle().unwrap(), 0);
    assert!(scratch_files(&f.temp.0).is_empty());
    read_bytes(&service, changed, b"known-alias-bytes");
}

#[test]
fn aliases_share_one_catalog_allowance_and_distinct_provider_cells_keep_their_own() {
    let temp = directory("aliases");
    let first_path = temp.0.join("first.sqlite");
    let second_path = temp.0.join("second.sqlite");
    let first = catalog(&first_path);
    let second = catalog(&second_path);
    assert_eq!(first.catalog_id(), second.catalog_id());
    let service = Arc::new(
        Service::new(
            vec![
                access(1, store(&temp.0.join("s1.sqlite")), Arc::clone(&first), ALL),
                access(2, store(&temp.0.join("s2.sqlite")), first, ALL),
                access(3, store(&temp.0.join("s3.sqlite")), second, ALL),
            ],
            OperationRecorder::disabled(),
        )
        .unwrap(),
    );
    let scope = [19; 32];
    let first = hold(
        &service,
        request(
            1,
            1,
            Operation::HistoryCommand(HistoryCommand::ReserveInodes { scope, count: 1 }),
        ),
        Vec::new(),
    );
    let refused = service
        .handle(
            &peer(),
            &request(
                2,
                2,
                Operation::HistoryCommand(HistoryCommand::ReserveInodes { scope, count: 2 }),
            ),
            &mut Unread,
            &mut io::sink(),
        )
        .0
        .unwrap_err();
    assert_eq!(refused, Failure::from(Code::Capacity));
    assert_eq!(highwater(&first_path, scope), 0);
    assert_eq!(reserve(&service, 3, 3, scope, 2), (1, 2));
    assert_eq!(highwater(&second_path, scope), 2);
    let query = call(
        &service,
        4,
        2,
        Operation::HistoryQuery(HistoryQuery::ListStacks {
            cursor: Vec::new(),
            limit: 1,
        }),
    )
    .unwrap();
    assert_eq!(
        history(query),
        HistoryResult::Stacks {
            continuation: Vec::new(),
            records: Vec::new()
        }
    );
    // A C5 owner consumes no content allowance, even on its StoreAccess alias.
    saved_file(&service, 5, b"content-during-catalog");
    assert_eq!(
        history(first.finish()),
        HistoryResult::Reservation {
            scope,
            start: 1,
            count: 1
        }
    );
    assert_eq!(reserve(&service, 6, 2, scope, 2), (2, 2));
    assert_eq!(highwater(&first_path, scope), 3);
}

/// A separate SQLite process owns the kernel-visible write transaction.
struct CatalogLock(Child);
impl CatalogLock {
    fn acquire(path: &Path) -> Self {
        let mut child = Command::new("python3").args(["-u", "-c", "import sqlite3,sys\nc=sqlite3.connect(sys.argv[1],timeout=0)\nc.execute('BEGIN IMMEDIATE')\nprint('held',flush=True)\nsys.stdin.buffer.read(1)\nc.rollback()"])
            .arg(path).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn().unwrap();
        let mut ready = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut ready)
            .unwrap();
        assert_eq!(ready.trim(), "held");
        Self(child)
    }
}
impl Drop for CatalogLock {
    fn drop(&mut self) {
        let _ = self.0.stdin.take().unwrap().write_all(b"x");
        assert!(self.0.wait().unwrap().success());
    }
}

#[test]
fn real_catalog_contention_and_range_exhaustion_preserve_allocator_and_content() {
    let f = fixture("provider");
    let before = std::fs::read(&f.store_path).unwrap();
    let initial = highwater(&f.catalog_path, f.snapshot.scope);
    let lock = CatalogLock::acquire(&f.catalog_path);
    let failure = call(
        &f.service,
        4,
        1,
        Operation::HistoryCommand(HistoryCommand::ReserveInodes {
            scope: f.snapshot.scope,
            count: 2,
        }),
    )
    .unwrap_err();
    assert_eq!(failure, Failure::from(Code::Busy));
    assert_eq!(highwater(&f.catalog_path, f.snapshot.scope), initial);
    assert_eq!(std::fs::read(&f.store_path).unwrap(), before);
    drop(lock);
    assert_eq!(
        reserve(&f.service, 5, 1, f.snapshot.scope, 2),
        (initial + 1, 2)
    );
    let scope = [91; 32];
    let count = i64::MAX as u64 - 1;
    assert_eq!(reserve(&f.service, 6, 1, scope, count), (1, count));
    let failure = call(
        &f.service,
        7,
        1,
        Operation::HistoryCommand(HistoryCommand::ReserveInodes { scope, count: 1 }),
    )
    .unwrap_err();
    assert_eq!(failure, Failure::from(Code::Capacity));
    assert_eq!(highwater(&f.catalog_path, scope), count);
    assert_eq!(std::fs::read(&f.store_path).unwrap(), before);
    assert_eq!(active(&f.store_path), 0);
    read_bytes(&f.service, f.file_content, b"original-admission-bytes");
}

#[test]
fn catalog_grants_read_only_authority_and_wrong_prepared_scope_refuse_before_save() {
    let f = fixture("authority");
    let reader: Arc<dyn HistoryCatalog> = Arc::new(
        sqlite::open_read_only(&f.catalog_path, b"layerfs-admission-authority", [71; 32]).unwrap(),
    );
    let denied = Service::new(
        vec![access(
            1,
            store(&f.temp.0.join("denied.sqlite")),
            Arc::clone(&f.catalog),
            0x20,
        )],
        OperationRecorder::disabled(),
    )
    .unwrap();
    let scope = [92; 32];
    let refusal = denied
        .handle(
            &peer(),
            &request(
                4,
                1,
                Operation::HistoryCommand(HistoryCommand::ReserveInodes { scope, count: 1 }),
            ),
            &mut Unread,
            &mut io::sink(),
        )
        .0
        .unwrap_err();
    assert_eq!(refusal, Failure::from(Code::Denied));
    let original = Timing::disabled("open", |scope| {
        Store::open(&f.store_path, scope.child("open"))
    })
    .0
    .unwrap();
    let readonly = Service::new(
        vec![access(1, original, reader, ALL)],
        OperationRecorder::disabled(),
    )
    .unwrap();
    let refusal = call(
        &readonly,
        5,
        1,
        Operation::HistoryCommand(HistoryCommand::ReserveInodes { scope, count: 1 }),
    )
    .unwrap_err();
    assert_eq!(refusal, Failure::from(Code::ContinuityUnavailable));
    assert_eq!(highwater(&f.catalog_path, scope), 0);
    let query = call(
        &readonly,
        6,
        1,
        Operation::HistoryQuery(HistoryQuery::GetBranch {
            branch: f.snapshot.branch.branch,
        }),
    )
    .unwrap();
    let HistoryResult::BranchSnapshot(snapshot) = history(query) else {
        panic!("branch snapshot")
    };
    assert_eq!(snapshot.effective_root, f.snapshot.effective_root);
    let (mut request, _) = prepared(&f, [83; 32], &[(2, f.file_content, false)], &[]);
    let Operation::HistoryCommand(HistoryCommand::StageChanges(changes)) = &mut request.operation
    else {
        panic!("prepared")
    };
    changes.scope = scope;
    let refusal = f
        .service
        .handle(&peer(), &request, &mut Unread, &mut io::sink())
        .0
        .unwrap_err();
    assert_eq!(refusal, Failure::from(Code::InvalidInput));
    assert_eq!(active(&f.store_path), 0);
    let absent = call(
        &f.service,
        7,
        1,
        Operation::HistoryQuery(HistoryQuery::GetStage {
            workspace: [83; 32],
        }),
    )
    .unwrap_err();
    assert_eq!(absent.code, Code::NotFound);
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn scratch_parent_refusal_precedes_save_and_body_without_a_hidden_retry() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture("scratch-parent-refusal");
    let saves = sql_number(&f.store_path, "SELECT count(*) FROM saves", None);
    let permissions = std::fs::metadata(&f.temp.0).unwrap().permissions();
    // Real kernel permissions make the prospective private parent ineligible.
    std::fs::set_permissions(&f.temp.0, std::fs::Permissions::from_mode(0o770)).unwrap();
    assert_eq!(
        reserve(&f.service, 30, 1, f.snapshot.scope, 4096),
        (3, 4096)
    );
    let (request, _) = prepared(&f, [84; 32], &[(2, f.file_content, false)], &[]);
    let first = f
        .service
        .handle(&peer(), &request, &mut Unread, &mut io::sink())
        .0
        .unwrap_err();
    assert_eq!(first, Failure::from(Code::Integrity));
    assert_eq!(active(&f.store_path), 0);
    assert_eq!(
        sql_number(&f.store_path, "SELECT count(*) FROM saves", None),
        saves
    );
    assert!(scratch_files(&f.temp.0).is_empty());
    std::fs::set_permissions(&f.temp.0, permissions).unwrap();
    // Restoring an external condition does not reattempt a refused constructor.
    let second = f
        .service
        .handle(&peer(), &request, &mut Unread, &mut io::sink())
        .0
        .unwrap_err();
    assert_eq!(second, first);
    assert_eq!(active(&f.store_path), 0);
    assert_eq!(
        sql_number(&f.store_path, "SELECT count(*) FROM saves", None),
        saves
    );
    assert!(scratch_files(&f.temp.0).is_empty());
    assert_eq!(
        reserve(&f.service, 31, 1, f.snapshot.scope, 4096),
        (4099, 4096)
    );
    let absent = call(
        &f.service,
        32,
        1,
        Operation::HistoryQuery(HistoryQuery::GetStage {
            workspace: [84; 32],
        }),
    )
    .unwrap_err();
    assert_eq!(absent.code, Code::NotFound);
}

#[test]
fn declared_directory_state_over_capacity_refuses_before_scratch_save_or_body() {
    let f = fixture("directory-state-capacity");
    let saves = sql_number(&f.store_path, "SELECT count(*) FROM saves", None);
    let (mut request, _) = prepared(&f, [85; 32], &[(2, f.file_content, false)], &[]);
    let Operation::HistoryCommand(HistoryCommand::StageChanges(changes)) = &mut request.operation
    else {
        panic!("prepared")
    };
    changes.totals.directories = 65_537;
    // This is a syntactically admitted declared stream, with no body consumption.
    changes.stream_bytes().unwrap();
    let refused = f
        .service
        .handle(&peer(), &request, &mut Unread, &mut io::sink())
        .0
        .unwrap_err();
    assert_eq!(refused, Failure::from(Code::Capacity));
    assert_eq!(active(&f.store_path), 0);
    assert_eq!(
        sql_number(&f.store_path, "SELECT count(*) FROM saves", None),
        saves
    );
    assert!(scratch_files(&f.temp.0).is_empty());
    // The refusal is for the requested shape, not permanent authority failure.
    let (small, body) = prepared(&f, [86; 32], &[(2, f.file_content, false)], &[]);
    let accepted = f
        .service
        .handle(&peer(), &small, &mut Cursor::new(body), &mut io::sink())
        .0
        .unwrap();
    assert_eq!(stage(accepted).candidate_root, f.snapshot.effective_root);
    assert_eq!(scratch_files(&f.temp.0).len(), 1);
    assert_eq!(f.service.drain_construction_idle().unwrap(), 1);
    assert_eq!(f.service.drain_construction_idle().unwrap(), 0);
    assert!(scratch_files(&f.temp.0).is_empty());
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn selected_sqlite_library() -> PathBuf {
    let binary = std::env::current_exe().unwrap();
    #[cfg(target_os = "macos")]
    let paths = {
        let output = Command::new("/usr/bin/otool")
            .arg("-L")
            .arg(&binary)
            .output()
            .unwrap();
        assert!(output.status.success());
        let links = String::from_utf8(output.stdout).unwrap();
        links
            .lines()
            .filter(|line| line.contains("libsqlite3."))
            .map(|line| PathBuf::from(line.split_whitespace().next().unwrap()))
            .collect::<std::collections::BTreeSet<_>>()
    };
    #[cfg(target_os = "linux")]
    let paths = {
        let mut maps = String::new();
        std::fs::File::open("/proc/self/maps")
            .unwrap()
            .take(1_048_577)
            .read_to_string(&mut maps)
            .unwrap();
        assert!(maps.len() <= 1_048_576, "bounded native library inventory");
        maps.lines()
            .filter_map(|line| line.split_whitespace().last())
            .filter(|path| path.starts_with('/') && path.contains("libsqlite3.so"))
            .map(PathBuf::from)
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert_eq!(
        paths.len(),
        1,
        "exact dynamically selected SQLite required; no provider substitute: {paths:?}"
    );
    let library = paths.into_iter().next().unwrap();
    eprintln!(
        "Server scratch proof selected SQLite: binary={} library={}",
        binary.display(),
        library.display()
    );
    library
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn short_control_directory() -> Temp {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "lfcs-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&path).unwrap();
    Temp(path)
}

/// External selected-library lock owner; never opens scratch after Unknown.
#[cfg(any(target_os = "macos", target_os = "linux"))]
struct SqlProviderLock {
    child: Option<Child>,
    control: Option<std::os::unix::net::UnixStream>,
    _directory: Temp,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl SqlProviderLock {
    fn acquire(database: &Path, library: &Path, mode: &str) -> Self {
        use nix::poll::{poll, PollFd, PollFlags};
        use std::os::fd::AsFd;
        use std::os::unix::net::UnixListener;

        // Match the native authority's existing owned pathname before opening
        // with NOFOLLOW; Darwin's /var temporary ancestor is a symlink alias.
        let database = std::fs::canonicalize(database).unwrap();

        let directory = short_control_directory();
        let socket = directory.0.join("r");
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        let child = Command::new("python3")
            .arg("-u")
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/scratch_reader.py"))
            .arg(library)
            .arg(mode)
            .arg(database)
            .arg(&socket)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut owner = Self {
            child: Some(child),
            control: None,
            _directory: directory,
        };
        let mut events = [PollFd::new(listener.as_fd(), PollFlags::POLLIN)];
        assert_eq!(
            poll(&mut events, 5000u16).unwrap(),
            1,
            "finite acknowledged provider connection"
        );
        assert!(events[0].revents().unwrap().contains(PollFlags::POLLIN));
        owner.control = Some(listener.accept().unwrap().0);
        let control = owner.control.as_mut().unwrap();
        control.set_nonblocking(false).unwrap(); // Darwin accepted descriptors inherit listener mode.
        control
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        control
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut ready = [0];
        control.read_exact(&mut ready).unwrap();
        assert_eq!(ready, [1]);
        let mut length = [0; 4];
        control.read_exact(&mut length).unwrap();
        let length = u32::from_be_bytes(length) as usize;
        assert!(
            length > 0 && length <= 8192,
            "bounded provider identity frame"
        );
        let mut info = vec![0; length];
        control.read_exact(&mut info).unwrap();
        let info = String::from_utf8(info).unwrap();
        assert!(
            info.contains(&format!("\"mode\": \"{mode}\""))
                && info.contains("\"source\":")
                && info.contains("\"version\":")
        );
        eprintln!("Server provider lock acknowledged: {info}");
        owner
    }

    fn release_and_reap(&mut self) {
        let control = self.control.as_mut().unwrap();
        control.write_all(&[2]).unwrap();
        let mut released = [0];
        control.read_exact(&mut released).unwrap();
        assert_eq!(
            released,
            [3],
            "acknowledged ROLLBACK and selected connection close"
        );
        let status = self.child.as_mut().unwrap().wait().unwrap();
        let mut child = self.child.take().unwrap();
        assert_eq!(child.wait().unwrap(), status); // Cached known exit on the moved binding, no retry.
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        child
            .stdout
            .take()
            .unwrap()
            .take(16_385)
            .read_to_end(&mut stdout)
            .unwrap();
        child
            .stderr
            .take()
            .unwrap()
            .take(16_385)
            .read_to_end(&mut stderr)
            .unwrap();
        assert!(
            stdout.len() <= 16_384 && stderr.len() <= 16_384,
            "bounded provider output"
        );
        assert!(
            status.success(),
            "provider stdout={} stderr={}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        );
        eprintln!(
            "Server provider lock completion: {}",
            String::from_utf8(stdout).unwrap().trim()
        );
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
impl Drop for SqlProviderLock {
    fn drop(&mut self) {
        if self.child.is_none() {
            return;
        }
        if let Some(control) = self.control.take() {
            if let Err(error) = control.shutdown(std::net::Shutdown::Both) {
                eprintln!("provider control shutdown: {error}");
            }
        }
        let Some(mut child) = self.child.take() else {
            return;
        };
        let exited = match child.try_wait() {
            Ok(Some(status)) => Some(status),
            Ok(None) => None,
            Err(error) => {
                eprintln!("provider exit inspection: {error}");
                None
            }
        };
        let status = if let Some(status) = exited {
            // Known cached exit on this exact moved binding; no new wait/retry.
            match child.wait() {
                Ok(cached) => {
                    debug_assert_eq!(cached, status);
                    Some(cached)
                }
                Err(error) => {
                    eprintln!("provider cached exit unavailable: {error}");
                    None
                }
            }
        } else {
            if let Err(error) = child.kill() {
                eprintln!("provider owned kill: {error}");
            }
            match child.wait() {
                Ok(status) => Some(status),
                Err(error) => {
                    eprintln!("provider owned reap: {error}");
                    None
                }
            }
        };
        if let Some(status) = status {
            // Every failed ACK/accept path preserves bounded actual helper output
            // after a known exit. Never replace an exception with a guessed EOF.
            for (name, pipe) in [
                (
                    "stdout",
                    child
                        .stdout
                        .take()
                        .map(|pipe| Box::new(pipe) as Box<dyn Read>),
                ),
                (
                    "stderr",
                    child
                        .stderr
                        .take()
                        .map(|pipe| Box::new(pipe) as Box<dyn Read>),
                ),
            ] {
                if let Some(pipe) = pipe {
                    let mut output = Vec::new();
                    match pipe.take(16_385).read_to_end(&mut output) {
                        Ok(_) => eprintln!(
                            "provider failed-coordination exit={status} {name} bytes={}{}: {}",
                            output.len(),
                            if output.len() > 16_384 {
                                " OUTPUT_BOUND_EXCEEDED"
                            } else {
                                ""
                            },
                            String::from_utf8_lossy(&output)
                        ),
                        Err(error) => {
                            eprintln!("provider failed-coordination {name} unavailable: {error}")
                        }
                    }
                }
            }
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn native_scratch_paths(parent: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(parent).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name().to_string_lossy().starts_with(".lfcs-") {
            for file in std::fs::read_dir(entry.path()).unwrap() {
                paths.push(file.unwrap().path());
            }
        }
    }
    paths.sort();
    paths
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn native_scratch_identity(path: &Path) -> (u64, u64, u32, u32, u64, u64) {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::symlink_metadata(path).unwrap();
    assert!(metadata.is_file() && !metadata.file_type().is_symlink());
    assert_eq!(metadata.nlink(), 1);
    assert_eq!(metadata.mode() & 0o777, 0o600);
    assert_eq!(metadata.blocks() * 512, 16 * 1024 * 1024);
    (
        metadata.dev(),
        metadata.ino(),
        metadata.uid(),
        metadata.mode(),
        metadata.nlink(),
        metadata.blocks() * 512,
    )
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn scratch_unknown_preserves_publication_and_reports_owned_save_cleanup() {
    let directory = directory("scratch-unknown-proof");
    for locked in [false, true] {
        let base = directory.0.join(if locked {
            "store-locked"
        } else {
            "store-unlocked"
        });
        std::fs::create_dir(&base).unwrap();
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "server_scratch_unknown_child", "--nocapture"])
            .env("LAYERFS_SERVER_SCRATCH_UNKNOWN_BASE", &base)
            .env(
                "LAYERFS_SERVER_SCRATCH_STORE_LOCKED",
                if locked { "1" } else { "0" },
            )
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "child locked={locked}: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout)
            .contains("Server scratch Unknown proof complete"));
        eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        let retained = native_scratch_paths(&base);
        assert_eq!(
            retained.len(),
            1,
            "retained scratch exists after owned child exit; no product cleanup claim"
        );
        native_scratch_identity(&retained[0]);
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[test]
fn server_scratch_unknown_child() {
    let Some(base) = std::env::var_os("LAYERFS_SERVER_SCRATCH_UNKNOWN_BASE") else {
        return;
    };
    let locked = std::env::var("LAYERFS_SERVER_SCRATCH_STORE_LOCKED").unwrap() == "1";
    // Parent owns final fixture removal after this child exits. Preserve the
    // Service and its exact retained SQL/native capsule until process teardown,
    // including assertion failure; no Temp Drop or authority-loss close intervenes.
    let f = std::mem::ManuallyDrop::new(fixture_at(Temp(PathBuf::from(base))));
    let new_bytes = b"known-finished-scratch-unknown-bytes";
    let changed = saved_file(&f.service, 40, new_bytes);
    assert_eq!(reserve(&f.service, 41, 1, f.snapshot.scope, 1), (3, 1));
    let branch_before = match history(
        call(
            &f.service,
            39,
            1,
            Operation::HistoryQuery(HistoryQuery::GetBranch {
                branch: f.snapshot.branch.branch,
            }),
        )
        .unwrap(),
    ) {
        HistoryResult::BranchSnapshot(snapshot) => snapshot,
        other => panic!("{other:?}"),
    };
    assert_eq!(branch_before.effective_root, f.snapshot.effective_root);
    let workspace = if locked { [0xa2; 32] } else { [0xa1; 32] };
    let expected_candidate = oracle::namespace(
        f.snapshot.scope,
        f.root_metadata,
        &[
            oracle::File {
                name: b"a",
                serial: 2,
                content: f.file_content,
                metadata: f.file_metadata,
            },
            oracle::File {
                name: b"b",
                serial: 3,
                content: changed,
                metadata: f.file_metadata,
            },
        ],
    );
    assert_ne!(expected_candidate, f.snapshot.effective_root);
    // Independent v1 grammar for this exact two-file/root-directory shape:
    // canonical envelope13, page header33, name rows22, inode rows3*81,
    // filesystem-root value116. Only the68-byte directory leaf precedes state
    // append; even the complete three-object candidate is486 canonical bytes.
    let candidate_objects: usize = 3;
    let candidate_bytes: u64 = (13 + 33 + 22) + (13 + 33 + 3 * 81) + (13 + 116);
    let batch_objects = layerfs_storage::policy::BATCH_OBJECT_LIMIT;
    let batch_bytes = layerfs_storage::policy::WAVE_CANONICAL_BYTES_LIMIT;
    assert!(candidate_objects < batch_objects && candidate_bytes < batch_bytes);
    let saves = sql_number(&f.store_path, "SELECT count(*) FROM saves", None);
    assert_eq!(active(&f.store_path), 0);
    assert!(scratch_files(&f.temp.0).is_empty());
    let (request, body) = prepared(
        &f,
        workspace,
        &[(3, changed, true)],
        &[(b"b".to_vec(), Some(3))],
    );
    assert!(
        body.len() < 512,
        "one directory/identity body, no Store flush before scratch append"
    );
    let held = hold(&f.service, request, body);
    assert_eq!(
        active(&f.store_path),
        1,
        "source gate is reached only after persisted Save admission"
    );
    assert_eq!(
        sql_number(&f.store_path, "SELECT count(*) FROM saves", None),
        saves + 1
    );
    let scratch = scratch_files(&f.temp.0); // SQL inspection only before body/Unknown.
    assert_eq!(scratch.len(), 1);
    let identity = native_scratch_identity(&scratch[0]);
    assert_eq!(sql_number(&f.store_path, "SELECT count(*) FROM objects WHERE save_id IN (SELECT save_id FROM saves WHERE active_slot IS NOT NULL)", None), 0);
    let library = selected_sqlite_library();
    let mut reader = SqlProviderLock::acquire(&scratch[0], &library, "read");
    let mut store_lock = locked.then(|| SqlProviderLock::acquire(&f.store_path, &library, "write"));
    assert_eq!(active(&f.store_path), 1);
    eprintln!("Server source gate proof: persistedSave=1, nativeScratch16MiB=1, BEGIN+SELECT scratchSHARED acknowledged, storeLocked={locked}; body not yet released");
    eprintln!("Server independent fixture grammar: candidate_objects={candidate_objects}, candidate_canonical_bytes={candidate_bytes}, one68-byte directory leaf before scratchappend; actual Store thresholds objects={batch_objects}, canonical_bytes={batch_bytes}");
    // One small canonical directory leaf (<512 objects/<4MiB) precedes the
    // DirectoryRoots append. No private Store pack is flushed before that append;
    // only metadata scratch COMMIT tries EXCLUSIVE while the reader holds SHARED.
    let failure = held.finish_result().unwrap_err();
    assert_eq!(failure.code, Code::Provider);
    assert!(
        failure.unknown,
        "original scratch COMMIT uncertainty must survive Server mapping"
    );
    assert_eq!(
        failure.cleanup,
        if locked { Some(Code::Ownership) } else { None }
    );
    assert_eq!(active(&f.store_path), u64::from(locked));
    assert_eq!(
        sql_number(&f.store_path, "SELECT count(*) FROM saves", None),
        saves + u64::from(locked)
    );
    eprintln!(
        "Server scratch failure proof: {failure}; activeSave={} before external lock release",
        u64::from(locked)
    );
    if let Some(lock) = &mut store_lock {
        lock.release_and_reap();
    }
    reader.release_and_reap();
    assert_eq!(
        active(&f.store_path),
        u64::from(locked),
        "failed owned cleanup is not silently retried after restored Store condition"
    );
    assert_eq!(
        native_scratch_paths(&f.temp.0),
        scratch,
        "identity-only observation; unknown scratch never queried/adopted/cleaned"
    );
    assert_eq!(native_scratch_identity(&scratch[0]), identity);
    let absent = call(
        &f.service,
        42,
        1,
        Operation::HistoryQuery(HistoryQuery::GetStage { workspace }),
    )
    .unwrap_err();
    assert_eq!(absent.code, Code::NotFound);
    assert!(!absent.unknown);
    assert_eq!(
        sql_number(
            &f.catalog_path,
            "SELECT count(*) FROM workspace_stages WHERE workspace_id=?",
            Some(workspace)
        ),
        0
    );
    let snapshot = match history(
        call(
            &f.service,
            43,
            1,
            Operation::HistoryQuery(HistoryQuery::GetBranch {
                branch: f.snapshot.branch.branch,
            }),
        )
        .unwrap(),
    ) {
        HistoryResult::BranchSnapshot(snapshot) => snapshot,
        other => panic!("{other:?}"),
    };
    assert_eq!(snapshot, branch_before);
    assert_ne!(snapshot.effective_root, expected_candidate);
    read_bytes(&f.service, f.file_content, b"original-admission-bytes");
    read_bytes(&f.service, changed, new_bytes);
    assert_eq!(highwater(&f.catalog_path, f.snapshot.scope), 3);
    eprintln!("Server publication proof: C5Stage absent, Branch unchanged, two independent finished-file roots/bytes intact, retained scratch identity={identity:?}; no query-based Unknown adoption or speed/resource admission");
    println!("Server scratch Unknown proof complete; storeLocked={locked}");
}
