//! #810 F5: the pinned Store work of Commits that change many base files,
//! on SQLite and on PostgreSQL. One mounted Workspace over a project of
//! `DIRS` x `SUBDIRS` x `FILES` small files makes three measured Commits:
//!
//! - `commit.metadata` (C3-like): `chmod` and a new mtime on `CHANGED`
//!   files, no byte written. The capture leaves their content as the base
//!   holds it, so the Workspace reader reads no base content: no object
//!   demand at all (`content_reads` 0), and no length demand either, because
//!   each length is remembered from the stat the change made.
//! - `commit.edits` (C4-like): an in-place same-length edit of the first
//!   bytes of the next `CHANGED` files. Each changed file's base length and
//!   content root are read once per captured inode page (the overlay's
//!   `PAGE_ROWS` window), not once per file.
//! - `commit.hidden`: changes that keep a file's size or restore its mtime
//!   while changing its bytes, beside metadata-only ones; the next mount
//!   reads every file back (`hidden_changes_are_committed`).
//!
//! Counts come from the sessions' own counters (`SqlWork`): writer and
//! Workspace readers as [connects, round trips, statements, transactions],
//! the Commit phases' combined round trips (writer plus readers), and the
//! Store's reader demand counts ([object demands, length demands]).
//!
//! The PostgreSQL half needs `LAYERFS_TEST_POSTGRES_URL` and reports
//! NOT_RUN without it.
use layerfs_history::{
    BranchId, ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig, HistoryName,
    LayerStackId, WorkspaceId,
};
use layerfs_persistence::{
    PersistenceConfig, PostgresHandles, PostgresSelection, SqlWork, SqliteAcquisitionSchema,
    SqliteHandles, SqlitePersistenceProfile, StoreSelection,
};
use layerfs_project::{init, InitRequest};
use layerfs_storage::{ReservationBlocks, Storage, StoragePolicy};
use layerfs_store_service::bootstrap::open_store_observed;
use layerfs_store_service::store::{BindRequest, BoundWorkspace, CommitOptions};
use layerfs_store_service::telemetry::{attribute_sql, phase_sql, Recording};
use layerfs_store_service::write::mutation::Mutation;
use layerfs_store_service::{Owner, OwnerConfig, StoreService};
use layerfs_telemetry::timer::Timing;
use layerfs_vfs::{Operation, Time};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const BINDING: &[u8] = b"commit-batched-base-files";
const CURSOR: [u8; 32] = [81; 32];
const PROJECT: &str = "f5";
const DIRS: usize = 21;
const SUBDIRS: usize = 10;
const FILES: usize = 11;
/// Files each measured Commit changes: more than one captured inode page
/// (`PAGE_ROWS`, 1,024), so the pins show one batch per page.
const CHANGED: usize = 1_100;
/// Bytes of each file.
const LENGTH: usize = 3_000;
/// The Mutation clock: the same change gives the same root in every run.
const NOW: i64 = 1_800_000_000;

/// [connects, round trips, statements, transactions].
type Counts = [u64; 4];

/// One Commit's result and Workspace-reader demands, the same on both
/// backends: [object demands, length demands].
struct Pin {
    scenario: &'static str,
    root: &'static str,
    demands: [u64; 2],
}

/// One Commit's PostgreSQL session counts and combined round trips by phase.
struct Wire {
    scenario: &'static str,
    writer: Counts,
    workspace: Counts,
    phases: &'static [(&'static str, u64)],
}

// The roots are the ones `e11d8c8d0f` (the campaign's base, before F5)
// commits for the same changes, on both backends: F5 changes reads only.
// Before F5 the Workspace reader made one object and one length demand per
// changed base file (`commit.metadata` and `commit.edits` 1,100 and 1,100,
// `commit.hidden` 7 and 7). Now a metadata-only change reads nothing, and
// the files to open are asked in one length demand and fetched in one
// object batch per captured inode page (two pages here). In
// `commit.hidden` five of its seven files are opened (bytes changed, or
// size not the base length) and two kept (mode only, mtime only).
const PINS: &[Pin] = &[
    Pin {
        scenario: "commit.warm",
        root: "fc94bee56c6d5ad2ad421ecff313447a6557a03a8e49759de3ff48e2f46c410d",
        demands: [0, 0],
    },
    Pin {
        scenario: "commit.metadata",
        root: "8e430591ea53b7af824ed34c361ebc9eed164563953946ffd8b7dfd07e2cdade",
        demands: [0, 0],
    },
    Pin {
        scenario: "commit.edits",
        root: "688265dae6831561d4cf3ca6c83962e8bc64e92985ac96aa4638b3415470fb81",
        demands: [2, 2],
    },
];
const HIDDEN: &[Pin] = &[Pin {
    scenario: "commit.hidden",
    root: "65c70deea0ad66ec74c90b46eadc7083e4c47c632879bc924a24bfea244e98bd",
    demands: [1, 1],
}];

// F5 alone left the writer unchanged. Before F5 (`e11d8c8d0f`), the
// Workspace reader sent `commit.metadata` 72-76 round trips and
// `commit.edits` 72-73 (the count moved with the reader's kept pack
// caches), construct 99-100 and 106-107 combined: about one `locate` and
// one span read per changed file, cut down by those caches. SQLite's
// statement counts vary by a few between runs (its reads of the same pages
// are not always the same statements), so only PostgreSQL's are pinned.
const POSTGRESQL: &[Wire] = &[
    Wire {
        scenario: "commit.warm",
        writer: [0, 27, 25, 3],
        workspace: [0, 0, 0, 0],
        phases: &[
            ("begin", 4),
            ("bindings", 1),
            ("construct", 11),
            ("finish", 5),
            ("logical.count", 2),
            ("logical.known", 2),
            ("publish.fused", 2),
        ],
    },
    Wire {
        scenario: "commit.metadata",
        writer: [0, 12, 17, 2],
        workspace: [0, 0, 0, 0],
        phases: &[
            ("construct", 2),
            ("finish", 5),
            ("logical.count", 3),
            ("publish.fused", 2),
        ],
    },
    Wire {
        scenario: "commit.edits",
        writer: [0, 13, 25, 3],
        workspace: [0, 3, 3, 0],
        phases: &[("construct", 9), ("finish", 6), ("publish.fused", 1)],
    },
];

#[test]
fn sqlite_batched_base_file_counts() {
    let measured = on_sqlite("sqlite", scenarios);
    check("sqlite", &measured, PINS, &[]);
}

#[test]
fn postgresql_batched_base_file_counts() {
    if let Some(measured) = on_postgresql("postgresql", "counts", scenarios) {
        check("postgresql", &measured, PINS, POSTGRESQL);
    }
}

#[test]
fn sqlite_hidden_changes_are_committed() {
    check("sqlite", &on_sqlite("sqlite-hidden", hidden), HIDDEN, &[]);
}

#[test]
fn postgresql_hidden_changes_are_committed() {
    if let Some(measured) = on_postgresql("postgresql-hidden", "hidden", hidden) {
        check("postgresql", &measured, HIDDEN, &[]);
    }
}

struct Measured {
    scenario: &'static str,
    writer: Counts,
    workspace: Counts,
    phases: Vec<(&'static str, u64)>,
    demands: [u64; 2],
    root: String,
}

fn path(index: usize) -> String {
    let (dir, rest) = (index / (SUBDIRS * FILES), index % (SUBDIRS * FILES));
    format!("/d{dir}/s{}/f{}.txt", rest / FILES, rest % FILES)
}

/// `LENGTH` incompressible bytes, fixed by `index`: the project's packs are
/// then far larger than a reader's kept caches, as in a real tree.
fn content(index: usize) -> Vec<u8> {
    let mut state = (index as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    (0..LENGTH)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

fn scenarios(
    dir: &Path,
    selection: &dyn Fn() -> StoreSelection,
    branch: BranchId,
) -> Vec<Measured> {
    let opened = Opened::open(dir, selection(), "o");
    let workspace = opened.bind(1, branch);
    let mut out = Vec::new();
    {
        let mut m = pinned(&workspace);
        m.write("/warm", b"warm\n", None).unwrap();
        m.finish().unwrap();
    }
    measure(&opened, &workspace, "commit.warm", &mut out);
    {
        let mut m = pinned(&workspace);
        for index in 0..CHANGED {
            let target = path(index);
            m.chmod(&target, 0o755).unwrap();
            let serial = m.resolve(&target).unwrap().serial;
            m.apply(Operation::SetAttributes {
                serial,
                mode: None,
                mtime: Some(Time {
                    seconds: NOW + 1_000,
                    nanoseconds: 0,
                }),
                size: None,
            })
            .unwrap();
        }
        m.finish().unwrap();
    }
    measure(&opened, &workspace, "commit.metadata", &mut out);
    {
        let mut m = pinned(&workspace);
        for index in CHANGED..2 * CHANGED {
            let serial = m.resolve(&path(index)).unwrap().serial;
            m.write_at(serial, 0, format!("EDIT{index:05}").as_bytes())
                .unwrap();
        }
        m.finish().unwrap();
    }
    measure(&opened, &workspace, "commit.edits", &mut out);
    opened.unmount(workspace);
    opened.stop();
    out
}

/// Each file of `commit.hidden`: its change, and its bytes and mode after.
fn hidden(dir: &Path, selection: &dyn Fn() -> StoreSelection, branch: BranchId) -> Vec<Measured> {
    let opened = Opened::open(dir, selection(), "h");
    let workspace = opened.bind(2, branch);
    let original = |index: usize| content(index);
    let mut expected: BTreeMap<String, (Vec<u8>, u32)> = BTreeMap::new();
    {
        let mut m = pinned(&workspace);
        let restore = |m: &mut Mutation<'_>, serial: u64| {
            m.apply(Operation::SetAttributes {
                serial,
                mode: None,
                mtime: Some(Time {
                    seconds: NOW - 5,
                    nanoseconds: 0,
                }),
                size: None,
            })
            .unwrap();
        };
        // 0: same length, other bytes, mtime set back, mode changed.
        let serial = m.resolve(&path(0)).unwrap().serial;
        let mut bytes = original(0);
        bytes[3] ^= 0x20;
        m.write_at(serial, 3, &bytes[3..4]).unwrap();
        restore(&mut m, serial);
        m.chmod(&path(0), 0o600).unwrap();
        expected.insert(path(0), (bytes, 0o600));
        // 1: truncated to 10, extended back to its length (zeros), mtime
        // set back: same size, no written cell.
        let serial = m.resolve(&path(1)).unwrap().serial;
        let length = original(1).len() as u64;
        for size in [10, length] {
            m.apply(Operation::SetAttributes {
                serial,
                mode: None,
                mtime: None,
                size: Some(size),
            })
            .unwrap();
        }
        restore(&mut m, serial);
        m.chmod(&path(1), 0o600).unwrap();
        let mut bytes = original(1)[..10].to_vec();
        bytes.resize(length as usize, 0);
        expected.insert(path(1), (bytes, 0o600));
        // 2: truncated only, mode changed: every byte left is inherited.
        let serial = m.resolve(&path(2)).unwrap().serial;
        m.apply(Operation::SetAttributes {
            serial,
            mode: None,
            mtime: None,
            size: Some(20),
        })
        .unwrap();
        m.chmod(&path(2), 0o600).unwrap();
        expected.insert(path(2), (original(2)[..20].to_vec(), 0o600));
        // 3: the same bytes written over its own, mode changed.
        let serial = m.resolve(&path(3)).unwrap().serial;
        m.write_at(serial, 0, &original(3)[..8]).unwrap();
        m.chmod(&path(3), 0o600).unwrap();
        expected.insert(path(3), (original(3), 0o600));
        // 4: mode only; 5: mtime only.
        m.chmod(&path(4), 0o600).unwrap();
        expected.insert(path(4), (original(4), 0o600));
        let serial = m.resolve(&path(5)).unwrap().serial;
        restore(&mut m, serial);
        expected.insert(path(5), (original(5), 0o644));
        // 6: extended by one byte, mtime set back.
        let serial = m.resolve(&path(6)).unwrap().serial;
        let length = original(6).len() as u64;
        m.write_at(serial, length, b"+").unwrap();
        restore(&mut m, serial);
        let mut bytes = original(6);
        bytes.push(b'+');
        expected.insert(path(6), (bytes, 0o644));
        m.finish().unwrap();
    }
    let mut out = Vec::new();
    measure(&opened, &workspace, "commit.hidden", &mut out);
    opened.unmount(workspace);
    opened.stop();
    // A fresh Store open and mount: every byte comes from the Commit.
    let opened = Opened::open(dir, selection(), "h2");
    let workspace = opened.bind(3, branch);
    let m = Mutation::new(&workspace).unwrap();
    for index in 0..12 {
        let target = path(index);
        let stat = m.resolve(&target).unwrap();
        let bytes = m.read(stat.serial, 0, 64 * 1024).unwrap();
        let (want, mode) = expected
            .get(&target)
            .cloned()
            .unwrap_or_else(|| (original(index), 0o644));
        assert_eq!(bytes, want, "{target}: committed bytes");
        assert_eq!(stat.logical_len, want.len() as u64, "{target}: length");
        assert_eq!(stat.metadata.mode & 0o7777, mode, "{target}: mode");
    }
    m.finish().unwrap();
    opened.unmount(workspace);
    opened.stop();
    out
}

fn pinned(workspace: &BoundWorkspace) -> Mutation<'_> {
    let mut m = Mutation::new(workspace).unwrap();
    m.now = Time {
        seconds: NOW,
        nanoseconds: 0,
    };
    m
}

fn counts(work: &SqlWork) -> Counts {
    [
        work.connects,
        work.round_trips,
        work.statements,
        work.transactions,
    ]
}

fn measure(
    opened: &Opened,
    workspace: &BoundWorkspace,
    scenario: &'static str,
    out: &mut Vec<Measured>,
) {
    let sessions = opened.service.opened.sql_sessions();
    let probe = opened.service.opened.sql_sessions();
    let recording = Recording::start();
    attribute_sql(Some(Box::new(move || {
        let (writer, readers, _) = probe.totals();
        let mut total = writer.unwrap_or_default();
        total.accumulate(&readers.unwrap_or_default());
        total
    })));
    let (writer, readers, _) = sessions.totals();
    let demands = opened.service.opened.store.work();
    let started = Instant::now();
    let success = workspace
        .commit_captured(&CommitOptions {
            force: false,
            message: Some(scenario.to_owned()),
            expected: None,
        })
        .unwrap_or_else(|failure| panic!("{scenario}: {failure}"));
    let ms = started.elapsed().as_secs_f64() * 1e3;
    let mut phases: BTreeMap<&'static str, SqlWork> = BTreeMap::new();
    for (name, work) in phase_sql() {
        phases.entry(name).or_default().accumulate(&work);
    }
    attribute_sql(None);
    drop(recording.finish());
    let (writer_after, readers_after, _) = sessions.totals();
    let after = opened.service.opened.store.work();
    let root = match &success.history {
        Some(layerfs_history::records::CommitOutcome::Committed { record, .. }) => {
            record.root.to_string()
        }
        other => panic!("{scenario}: {other:?}"),
    };
    let measured = Measured {
        scenario,
        writer: counts(&writer_after.unwrap().since(&writer.unwrap())),
        workspace: counts(
            &readers_after
                .unwrap_or_default()
                .since(&readers.unwrap_or_default()),
        ),
        phases: phases
            .into_iter()
            .filter(|(_, work)| work.round_trips != 0)
            .map(|(name, work)| (name, work.round_trips))
            .collect(),
        demands: [
            after.object_batches - demands.object_batches,
            after.length_batches - demands.length_batches,
        ],
        root,
    };
    println!(
        "F5_COUNTS {{\"scenario\":\"{}\",\"writer\":{:?},\"workspace\":{:?},\"phases\":{:?},\"demands\":{:?},\"root\":\"{}\",\"ms\":{ms:.1}}}",
        measured.scenario,
        measured.writer,
        measured.workspace,
        measured.phases,
        measured.demands,
        measured.root
    );
    out.push(measured);
}

fn check(backend: &str, measured: &[Measured], pins: &[Pin], wire: &[Wire]) {
    assert_eq!(measured.len(), pins.len(), "{backend}: scenarios");
    for (m, pin) in measured.iter().zip(pins) {
        assert_eq!(m.scenario, pin.scenario);
        assert_eq!(m.root, pin.root, "{backend} {}: root", m.scenario);
        assert_eq!(
            m.demands, pin.demands,
            "{backend} {}: Workspace-reader [object, length] demands",
            m.scenario
        );
    }
    if wire.is_empty() {
        return;
    }
    assert_eq!(measured.len(), wire.len(), "{backend}: scenarios");
    for (m, pin) in measured.iter().zip(wire) {
        assert_eq!(m.scenario, pin.scenario);
        assert_eq!(
            (m.writer, m.workspace),
            (pin.writer, pin.workspace),
            "{backend} {}: (writer, workspace) x [connects, round_trips, statements, transactions]",
            m.scenario
        );
        assert_eq!(
            m.phases, pin.phases,
            "{backend} {}: round trips by phase",
            m.scenario
        );
    }
}

type Scenarios = fn(&Path, &dyn Fn() -> StoreSelection, BranchId) -> Vec<Measured>;

fn on_sqlite(label: &str, run: Scenarios) -> Vec<Measured> {
    let dir = scratch(label);
    let config = PersistenceConfig::sqlite(dir.join("store.sqlite"))
        .with_sqlite_profile(SqlitePersistenceProfile::Normal)
        .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables);
    let handles =
        SqliteHandles::create(config.clone(), StoragePolicy::frozen_default(), &catalog()).unwrap();
    let storage = Storage::new(handles.storage.clone()).unwrap();
    let branch = initialize(&dir, &storage, &handles.history, &handles.acquisition);
    drop(storage);
    let sealed = handles.seal().unwrap();
    let config = PersistenceConfig::sqlite(&sealed.path)
        .with_sqlite_profile(SqlitePersistenceProfile::Normal);
    let measured = run(&dir, &|| StoreSelection::Sqlite(config.clone()), branch);
    std::fs::remove_dir_all(&dir).unwrap();
    measured
}

fn on_postgresql(label: &str, tag: &str, run: Scenarios) -> Option<Vec<Measured>> {
    let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
        return None;
    };
    let dir = scratch(label);
    let schema = format!("lfs810_f5_{tag}_{}", std::process::id());
    let selection = PostgresSelection::new(&url, &schema);
    let result = std::panic::catch_unwind(|| {
        let handles =
            PostgresHandles::create(&selection, StoragePolicy::frozen_default(), &catalog())
                .unwrap();
        let storage = Storage::new(handles.storage.clone()).unwrap();
        let branch = initialize(&dir, &storage, &handles.history, &handles.acquisition);
        drop(storage);
        drop(handles);
        run(
            &dir,
            &|| StoreSelection::Postgres(selection.clone()),
            branch,
        )
    });
    if let Ok(mut client) = postgres::Client::connect(&url, postgres::NoTls) {
        let _ = client.batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"));
    }
    let _ = std::fs::remove_dir_all(&dir);
    match result {
        Ok(measured) => Some(measured),
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

fn scratch(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs810-f5-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn catalog() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: BINDING.to_vec(),
        cursor_key: CURSOR,
        incarnation: 1,
    }
}

fn initialize(
    dir: &Path,
    storage: &Storage,
    history: &dyn HistoryCatalog,
    acquisition: &dyn layerfs_storage::port::acquisition::Acquisition,
) -> BranchId {
    let source = dir.join("source");
    for index in 0..DIRS * SUBDIRS * FILES {
        let file = source.join(&path(index)[1..]);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, content(index)).unwrap();
        let mut permissions = std::fs::metadata(&file).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o644);
        std::fs::set_permissions(&file, permissions).unwrap();
        let time = std::time::UNIX_EPOCH + Duration::from_secs(NOW as u64 - 10);
        std::fs::File::options()
            .write(true)
            .open(&file)
            .unwrap()
            .set_modified(time)
            .unwrap();
    }
    // Directories too: the same project, root and pins in every run.
    let mut dirs = vec![source.clone()];
    for dir in 0..DIRS {
        let top = source.join(format!("d{dir}"));
        dirs.extend((0..SUBDIRS).map(|sub| top.join(format!("s{sub}"))));
        dirs.push(top);
    }
    for dir in dirs {
        std::fs::File::open(&dir)
            .unwrap()
            .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(NOW as u64 - 10))
            .unwrap();
    }
    let initialized = Timing::disabled("init", |timing| {
        init(
            storage,
            history,
            InitRequest {
                source: &source,
                acquisition,
                stack: LayerStackId::from_authority([81; 16]),
                name: HistoryName::new(PROJECT).unwrap(),
                scope_seed: [81; 32],
                deadline: Instant::now() + Duration::from_secs(60),
            },
            timing,
        )
    })
    .0
    .unwrap();
    let branch = BranchId::from_authority([82; 16]);
    history
        .fork(&ForkRequest {
            stack: initialized.stack.id,
            branch,
            name: HistoryName::new("work").unwrap(),
            source: ForkSource::Layer(initialized.stack.head_layer),
        })
        .unwrap();
    std::fs::remove_dir_all(source).unwrap();
    branch
}

struct Opened {
    service: StoreService,
    owner: Owner,
}
impl Opened {
    fn open(dir: &Path, selection: StoreSelection, tag: &str) -> Self {
        let opened = open_store_observed(
            selection,
            BINDING,
            CURSOR,
            OwnerConfig::default().namespaces,
            64 * 1024 * 1024,
            ReservationBlocks::default(),
        )
        .unwrap();
        opened.prepare_mounts().unwrap();
        let overlay = dir.join(tag);
        std::fs::create_dir_all(&overlay).unwrap();
        let owner = Owner::start(
            &overlay.join("overlay.sqlite"),
            layerfs_overlay::ProfileConfig::default(),
            OwnerConfig::default(),
        )
        .unwrap();
        let service = StoreService::new(opened, &owner);
        Self { service, owner }
    }
    fn bind(&self, n: u8, branch: BranchId) -> BoundWorkspace {
        self.service
            .store
            .bind(
                self.owner.client(),
                BindRequest {
                    branch,
                    workspace: WorkspaceId::from_authority([n; 32]).unwrap(),
                },
            )
            .unwrap()
            .workspace
    }
    fn unmount(&self, workspace: BoundWorkspace) {
        let namespace = workspace.route().namespace();
        workspace.close().unwrap();
        self.owner.client().retire_namespace(namespace).unwrap();
    }
    fn stop(self) {
        drop(self.service);
        self.owner.stop().unwrap();
    }
}
