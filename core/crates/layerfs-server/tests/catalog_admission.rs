//! Typed C5 admission under real occupied C2 Save slots (#287 R1a).
//!
//! Every requested operation enters the public authorized handler with real
//! Store/catalog providers. Held prepared sources plus persisted active-slot
//! rows establish overlap; a SaveFile spooling barrier would not establish it.
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
    // Existing independently sealed v1 whole-file identity, also preserved by
    // the published R0 oracle. Candidate output does not define this pin.
    let fixed = hex(&oracle::file(b"layerfs-stage02-fixture"));
    assert_eq!(
        fixed,
        "81bb74372b166542e1547b1a03c6637b72dc8dacb08f63489de0fe0fab1dcfa3"
    );
    let temp = directory(name);
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
    fn finish(mut self) -> Response {
        self.release.send(()).unwrap();
        self.worker.take().unwrap().join().unwrap().unwrap()
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
    let saves = sql_number(&f.store_path, "SELECT count(*) FROM saves", None);
    // The successful response and committed high-water are observed before
    // either source is released, not inferred from a launched request.
    assert_eq!(
        reserve(&f.service, 6, 1, f.snapshot.scope, 4096),
        (4099, 4096)
    );
    assert_eq!(highwater(&f.catalog_path, f.snapshot.scope), 8194);
    assert_eq!(active(&f.store_path), 2);
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
    let second = stage(second.finish());
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
