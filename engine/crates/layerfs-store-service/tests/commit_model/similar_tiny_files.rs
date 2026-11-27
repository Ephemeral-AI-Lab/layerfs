//! #810 F2: a Workspace Commit of many similar small files publishes once
//! per preparation wave, not once per similar file.
//!
//! The similarity ring proposes, as a whole-file object's delta base, the
//! last similar object this Save stored FULL, often while that one is still
//! unpublished. Selection may use a base only once it is published, so the
//! Save used to publish it first: one transaction, one reservation and one
//! pack per similar file. The Save now decides before it publishes
//! (`layerfs-storage` `save/select.rs`, `save/deferred.rs`):
//!
//! * an object whose declared predecessor selection acquires never asks the
//!   ring, so its proposal is not published early;
//! * a trial against the base's kept bytes that does not beat FULL stores
//!   FULL at once (below about 70 bytes a PREFIX record never wins: it
//!   carries the base's 32-byte identity);
//! * a trial that wins holds the object until the preparation wave's own
//!   publication has published the base, then selects it as before.
//!
//! Three measured Commits after one mount and a warm-up Commit: `overwrite`
//! rewrites the 150 seeded files with near-identical text of about 85 bytes
//! (each names its earlier version, which loses); `add` adds 150 new similar
//! files of about 55 bytes (each loses against the last one stored FULL);
//! `families` adds 30 families of five similar 400-byte files (each member
//! after the first wins against the first). Pinned per Commit, on SQLite and
//! PostgreSQL: the writer's, control reader's and Workspace readers'
//! [connects, round trips, statements, transactions]; the Save's separate
//! publications (`Diagnostics::publish`); its storage: new objects, FULL and
//! PREFIX records, packs, and the bytes of its payload packs; and its
//! whole-file objects by delta depth. The metadata packs hold the Commit's
//! modification times and vary by a few bytes between runs: printed, not
//! pinned. Before #810 (e11d8c8d0f) the same Commits stored the same records
//! at the same depths, on both backends, with:
//!
//! | Commit | publications | PostgreSQL writer round trips | packs | payload bytes |
//! | --- | --- | --- | --- | --- |
//! | overwrite | 86 | 522 | 87 | 12,952 |
//! | add | 40 | 280 | 41 | 9,120 |
//! | families | 31 | 217 | 32 | 17,255 |
//!
//! The PostgreSQL half needs `LAYERFS_TEST_POSTGRES_URL` and reports
//! NOT_RUN without it.
use layerfs_history::records::CommitOutcome;
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
use layerfs_store_service::filesystem::change::mutation::Mutation;
use layerfs_store_service::store::open::open_store_observed;
use layerfs_store_service::store::{BindRequest, BoundWorkspace, CommitOptions, CommitSuccess};
use layerfs_store_service::{Owner, OwnerConfig, StoreService};
use layerfs_telemetry::timer::Timing;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const BINDING: &[u8] = b"commit-similar-tiny-files";
const CURSOR: [u8; 32] = [81; 32];
const PROJECT: &str = "similar";
const BRANCH: &str = "work";
/// Seeded files, and files each measured Commit changes or adds.
const FILES: usize = 150;
/// Directories they spread over.
const DIRECTORIES: usize = 5;
/// Families of similar 400-byte files the third measured Commit adds, and
/// members per family.
const FAMILIES: usize = 30;
const MEMBERS: usize = 5;

/// One session group's counted work: connects, round trips, statements and
/// transactions.
type Counts = [u64; 4];

/// The pinned work and storage of one measured Commit.
#[derive(Debug)]
struct Pin {
    scenario: &'static str,
    /// Writer, control reader, Workspace readers.
    counts: [Counts; 3],
    /// Separate publications of the Save before the lane's.
    publications: u64,
    /// New objects, FULL records, PREFIX records, packs, payload bytes.
    storage: [u64; 5],
    /// Whole-file objects stored at delta depth 0, 1 and 2.
    depths: [u64; 3],
}

/// One measured Commit: its pinned part and its metadata pack bytes.
struct Measured {
    pin: Pin,
    metadata_bytes: u64,
}

const SQLITE: &[Pin] = &[
    Pin {
        scenario: "commit.overwrite",
        counts: [[0, 0, 28, 2], [0, 0, 0, 0], [0, 0, 1, 0]],
        publications: 1,
        storage: [155, 153, 2, 2, 10_232],
        depths: [150, 0, 0],
    },
    Pin {
        scenario: "commit.add",
        counts: [[0, 0, 27, 2], [0, 0, 0, 0], [0, 0, 0, 0]],
        publications: 1,
        storage: [164, 161, 3, 2, 7_872],
        depths: [150, 0, 0],
    },
    Pin {
        scenario: "commit.families",
        counts: [[0, 0, 35, 3], [0, 0, 0, 0], [0, 0, 0, 0]],
        publications: 2,
        storage: [159, 35, 124, 3, 16_327],
        depths: [30, 120, 0],
    },
];

// #816 Stage 2 (db54e9d75f) takes one writer statement from each shape and
// one round trip from add and families; stored objects are unchanged.
const POSTGRESQL: &[Pin] = &[
    Pin {
        scenario: "commit.overwrite",
        counts: [[0, 10, 15, 2], [0, 0, 0, 0], [0, 1, 1, 0]],
        publications: 1,
        storage: [155, 153, 2, 2, 10_232],
        depths: [150, 0, 0],
    },
    Pin {
        scenario: "commit.add",
        counts: [[0, 6, 14, 2], [0, 0, 0, 0], [0, 0, 0, 0]],
        publications: 1,
        storage: [164, 161, 3, 2, 7_872],
        depths: [150, 0, 0],
    },
    Pin {
        scenario: "commit.families",
        counts: [[0, 15, 18, 3], [0, 0, 0, 0], [0, 0, 0, 0]],
        publications: 2,
        storage: [159, 35, 124, 3, 16_327],
        depths: [30, 120, 0],
    },
];

type Scenarios = fn(&Path, &dyn Fn() -> StoreSelection, BranchId) -> Vec<Measured>;

#[test]
fn sqlite_similar_tiny_file_commits() {
    check("sqlite", &on_sqlite("sqlite", scenarios), SQLITE);
}

#[test]
fn postgresql_similar_tiny_file_commits() {
    if let Some(measured) = on_postgresql("postgresql", "pg", scenarios) {
        check("postgresql", &measured, POSTGRESQL);
    }
}

/// Runs `run` on a new sealed SQLite Store.
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

/// Runs `run` on a new PostgreSQL Store in its own schema, or reports
/// NOT_RUN without a server.
fn on_postgresql(label: &str, tag: &str, run: Scenarios) -> Option<Vec<Measured>> {
    let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
        return None;
    };
    let dir = scratch(label);
    let schema = format!("lfs810_{tag}_{}", std::process::id());
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
    let dir = std::env::temp_dir().join(format!("lfs810-similar-{label}-{}", std::process::id()));
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

fn seeded(n: usize) -> String {
    format!("/src/module_{:02}/component_{n:05}.rs", n % DIRECTORIES)
}

fn added(n: usize) -> String {
    format!("/new1k/d{:02}/n{n}.rs", n % DIRECTORIES)
}

/// The near-identical text of the file at `path`: about 60 bytes.
fn family_path(family: usize, member: usize) -> String {
    format!("/families/f{family:02}_m{member}.txt")
}

/// Member `member` of family `family`: about 400 bytes of text the family
/// shares, then one line of its own, so each member's PREFIX against the
/// family's first beats FULL.
fn family_member(family: usize, member: usize) -> Vec<u8> {
    let mut state = (family as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    let mut bytes: Vec<u8> = (0..400)
        .map(|n| {
            if n % 64 == 63 {
                return b'\n';
            }
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            b'a' + (state % 26) as u8
        })
        .collect();
    bytes.extend_from_slice(format!("member {member}\n").as_bytes());
    bytes
}

fn similar(path: &str) -> Vec<u8> {
    format!("// edited c1000 {path}\nfn main() {{}}\n").into_bytes()
}

/// Inits a project of [`FILES`] distinct tiny files and forks Branch `work`.
fn initialize(
    dir: &Path,
    storage: &Storage,
    history: &dyn HistoryCatalog,
    acquisition: &dyn layerfs_storage::port::acquisition::Acquisition,
) -> BranchId {
    let source = dir.join("source");
    for n in 0..FILES {
        let path = source.join(seeded(n).trim_start_matches('/'));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, format!("pub fn f{n}() -> u32 {{\n    {n}\n}}\n")).unwrap();
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
            name: HistoryName::new(BRANCH).unwrap(),
            source: ForkSource::Layer(initialized.stack.head_layer),
        })
        .unwrap();
    std::fs::remove_dir_all(source).unwrap();
    branch
}

/// One Store open as the daemon opens it, with its overlay owner.
struct Opened {
    service: StoreService,
    owner: Owner,
}
impl Opened {
    /// The daemon's open: the Store, then the writer's mount statements
    /// (`OpenedStore::prepare_mounts`, #768).
    fn open(dir: &Path, selection: StoreSelection, tag: &str) -> Self {
        let opened = Self::open_plain(dir, selection, tag);
        opened.service.opened.prepare_mounts().unwrap();
        opened
    }
    /// The Store open alone, as an SDK call opens it.
    fn open_plain(dir: &Path, selection: StoreSelection, tag: &str) -> Self {
        let opened = open_store_observed(
            selection,
            BINDING,
            CURSOR,
            OwnerConfig::default().namespaces,
            64 * 1024 * 1024,
            ReservationBlocks::default(),
        )
        .unwrap();
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
    /// Writer, control reader and Workspace readers (live and retired).
    fn work(&self) -> [SqlWork; 3] {
        let opened = &self.service.opened;
        let writer = opened.session_diagnostics(None).unwrap().unwrap();
        let control = opened
            .session_diagnostics(Some(0))
            .unwrap()
            .unwrap_or_default();
        let mut readers = SqlWork::default();
        for reader in &opened.diagnostics().unwrap().readers {
            readers.accumulate(reader);
        }
        [writer, control, readers.since(&control)]
    }
    fn workspace(n: u8) -> WorkspaceId {
        WorkspaceId::from_authority([n; 32]).unwrap()
    }
    fn by_id(&self, n: u8, branch: BranchId) -> BoundWorkspace {
        self.service
            .store
            .bind(
                self.owner.client(),
                BindRequest {
                    branch,
                    workspace: Self::workspace(n),
                },
            )
            .unwrap()
            .workspace
    }
    /// One Workspace Commit, as the daemon's `workspace.commit` runs it.
    fn commit(&self, workspace: &BoundWorkspace) -> CommitSuccess {
        let success = workspace
            .commit_captured(&CommitOptions {
                force: false,
                message: Some("similar tiny files".to_owned()),
                expected: None,
            })
            .unwrap_or_else(|failure| panic!("commit: {failure}"));
        assert!(
            matches!(success.history, Some(CommitOutcome::Committed { .. })),
            "{:?}",
            success.history
        );
        success
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

fn counts(work: &SqlWork) -> Counts {
    [
        work.connects,
        work.round_trips,
        work.statements,
        work.transactions,
    ]
}

/// One measured Commit of `workspace`.
fn measure(opened: &Opened, scenario: &'static str, workspace: &BoundWorkspace) -> Measured {
    let before = opened.work();
    let success = opened.commit(workspace);
    let after = opened.work();
    let mut counts_ = [[0; 4]; 3];
    for (slot, (after, before)) in counts_.iter_mut().zip(after.iter().zip(before.iter())) {
        *slot = counts(&after.since(before));
    }
    let saved = success.saved;
    let pin = Pin {
        scenario,
        counts: counts_,
        publications: success.storage.publish,
        storage: [
            saved.inserted,
            saved.full_records,
            saved.prefix_records,
            saved.packs,
            saved.stored_bytes - saved.metadata_bytes,
        ],
        depths: [
            saved.delta.whole_file_depths[0],
            saved.delta.whole_file_depths[1],
            saved.delta.whole_file_depths[2],
        ],
    };
    Measured {
        pin,
        metadata_bytes: saved.metadata_bytes,
    }
}

/// After one mount and a warm-up Commit: the overwrite, then the add.
fn scenarios(
    dir: &Path,
    selection: &dyn Fn() -> StoreSelection,
    branch: BranchId,
) -> Vec<Measured> {
    let mut out = Vec::new();
    let opened = Opened::open(dir, selection(), "a");
    let workspace = opened.by_id(1, branch);
    let mut change = Mutation::new(&workspace).unwrap();
    change.write("/warm", b"warm\n", None).unwrap();
    change.finish().unwrap();
    opened.commit(&workspace);
    let mut change = Mutation::new(&workspace).unwrap();
    for n in 0..FILES {
        change
            .write(&seeded(n), &similar(&seeded(n)), None)
            .unwrap();
    }
    change.finish().unwrap();
    out.push(measure(&opened, "commit.overwrite", &workspace));
    let mut change = Mutation::new(&workspace).unwrap();
    change.mkdir("/new1k", None).unwrap();
    for d in 0..DIRECTORIES {
        change.mkdir(&format!("/new1k/d{d:02}"), None).unwrap();
    }
    for n in 0..FILES {
        change.write(&added(n), &similar(&added(n)), None).unwrap();
    }
    change.finish().unwrap();
    out.push(measure(&opened, "commit.add", &workspace));
    let mut change = Mutation::new(&workspace).unwrap();
    change.mkdir("/families", None).unwrap();
    for family in 0..FAMILIES {
        for member in 0..MEMBERS {
            change
                .write(
                    &family_path(family, member),
                    &family_member(family, member),
                    None,
                )
                .unwrap();
        }
    }
    change.finish().unwrap();
    out.push(measure(&opened, "commit.families", &workspace));
    let view = Mutation::new(&workspace).unwrap();
    for path in [seeded(0), seeded(FILES - 1), added(0), added(FILES - 1)] {
        let stat = view.resolve(&path).unwrap();
        assert_eq!(
            view.read(stat.serial, 0, 4096).unwrap(),
            similar(&path),
            "{path}"
        );
    }
    for (family, member) in [(0, 0), (0, MEMBERS - 1), (FAMILIES - 1, 1)] {
        let path = family_path(family, member);
        let stat = view.resolve(&path).unwrap();
        assert_eq!(
            view.read(stat.serial, 0, 4096).unwrap(),
            family_member(family, member),
            "{path}"
        );
    }
    view.finish().unwrap();
    opened.unmount(workspace);
    opened.stop();
    out
}

/// Prints every Commit, then compares each with its pin.
fn check(backend: &str, measured: &[Measured], pins: &[Pin]) {
    for m in measured {
        println!(
            "SIMILAR_COUNTS {{\"backend\":\"{backend}\",\"scenario\":\"{}\",\"writer\":{:?},\"control\":{:?},\"workspace\":{:?},\"publications\":{},\"storage\":{:?},\"depths\":{:?},\"metadata_bytes\":{}}}",
            m.pin.scenario,
            m.pin.counts[0],
            m.pin.counts[1],
            m.pin.counts[2],
            m.pin.publications,
            m.pin.storage,
            m.pin.depths,
            m.metadata_bytes
        );
    }
    assert_eq!(measured.len(), pins.len(), "{backend}");
    for (m, pin) in measured.iter().map(|m| &m.pin).zip(pins) {
        assert_eq!(
            (m.scenario, m.counts, m.publications, m.storage, m.depths),
            (
                pin.scenario,
                pin.counts,
                pin.publications,
                pin.storage,
                pin.depths
            ),
            "{backend}: (scenario, [writer, control, workspace] x [connects, round_trips, \
             statements, transactions], publications, [inserted, full, prefix, packs, \
             payload bytes], whole-file depths)"
        );
    }
}
