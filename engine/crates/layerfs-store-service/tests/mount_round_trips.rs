//! #768: the pinned SQL work of one Workspace mount through the Store
//! service, by Branch id and by project and Branch name, on SQLite and on
//! PostgreSQL.
//!
//! Each scenario is the first mount on a newly opened Store (`first`), or a
//! second mount on the same Store after the first one closed (`again`).
//! Counts come from the sessions' own counters (`SqlWork`), split into the
//! writer (with its lane connection), the control reader (session 0) and
//! the Workspace readers:
//!
//! - `connects`: connections opened (PostgreSQL only).
//! - `round_trips`: PostgreSQL client round trips, `BEGIN`/`COMMIT`
//!   batches and statement prepares included (zero on SQLite).
//! - `statements`: on PostgreSQL the statements a body sends, without the
//!   `BEGIN` batch, `COMMIT` or `ROLLBACK`; on SQLite every prepared
//!   statement run, transaction controls included.
//! - `transactions`: explicit transactions begun.
//!
//! The PostgreSQL half needs `LAYERFS_TEST_POSTGRES_URL` and reports
//! NOT_RUN without it. Mount admission is still being reworked (#768), so
//! these pins are expected to move with it; they never loosen a guard.
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
use layerfs_store_service::store::{BindRequest, BoundWorkspace, NamedBindRequest};
use layerfs_store_service::{Owner, OwnerConfig, StoreService};
use layerfs_telemetry::timer::Timing;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const BINDING: &[u8] = b"mount-round-trips";
const CURSOR: [u8; 32] = [76; 32];
const PROJECT: &str = "rt";
const BRANCH: &str = "work";

/// One session group's counted work: connects, round trips, statements and
/// transactions.
type Counts = [u64; 4];

/// The pinned counts of one scenario: writer, control reader, Workspace
/// readers.
struct Pin {
    scenario: &'static str,
    writer: Counts,
    control: Counts,
    workspace: Counts,
}

// Before #768's mount change (d3fe2c4506), SQLite read the bound node's
// serial and markers on the control reader (6 statements, 2 transactions)
// and a bind by Branch id read the project name on the writer (3
// statements, 1 transaction). Now both mount kinds touch only the writer
// (admission) and their own reader: the snapshot, then one statement with
// the serial and markers after the intent commits.
const SQLITE: &[Pin] = &[
    Pin {
        scenario: "mount.by_id.first",
        writer: [0, 0, 5, 1],
        control: [0, 0, 0, 0],
        workspace: [0, 0, 47, 4],
    },
    Pin {
        scenario: "mount.by_id.again",
        writer: [0, 0, 5, 1],
        control: [0, 0, 0, 0],
        workspace: [0, 0, 42, 4],
    },
    Pin {
        scenario: "mount.by_name.first",
        writer: [0, 0, 5, 1],
        control: [0, 0, 0, 0],
        workspace: [0, 0, 47, 4],
    },
];

// Before (d3fe2c4506), every first mount opened the control reader (1
// connect, 7 round trips) for the serial and markers (4 round trips once
// open), and a bind by Branch id read the project name on the writer in a
// Read transaction: by id 2 connects, 34 round trips, 17 statements, 3
// transactions; again 17/10/3; by name 2 connects, 30/16/2. The writer's
// admission then takes every advisory key in one statement and registers
// the mount outside the publication lane: 2 round trips and 2 statements
// fewer on the writer than the 12/6 (first) and 8/6 (again) it had after the
// read changes alone.
const POSTGRESQL: &[Pin] = &[
    Pin {
        scenario: "mount.by_id.first",
        writer: [0, 10, 4, 1],
        control: [0, 0, 0, 0],
        workspace: [1, 13, 8, 0],
    },
    Pin {
        scenario: "mount.by_id.again",
        writer: [0, 6, 4, 1],
        control: [0, 0, 0, 0],
        workspace: [0, 3, 2, 0],
    },
    Pin {
        scenario: "mount.by_name.first",
        writer: [0, 10, 4, 1],
        control: [0, 0, 0, 0],
        workspace: [1, 13, 8, 0],
    },
];

#[test]
fn sqlite_mount_counts() {
    let dir = scratch("sqlite");
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
    let measured = scenarios(&dir, || StoreSelection::Sqlite(config.clone()), branch);
    std::fs::remove_dir_all(&dir).unwrap();
    check("sqlite", &measured, SQLITE);
}

#[test]
fn postgresql_mount_counts() {
    let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
        return;
    };
    let dir = scratch("postgresql");
    let schema = format!("lfs768_mount_{}", std::process::id());
    let selection = PostgresSelection::new(&url, &schema);
    let result = std::panic::catch_unwind(|| {
        let handles =
            PostgresHandles::create(&selection, StoragePolicy::frozen_default(), &catalog())
                .unwrap();
        let storage = Storage::new(handles.storage.clone()).unwrap();
        let branch = initialize(&dir, &storage, &handles.history, &handles.acquisition);
        drop(storage);
        drop(handles);
        scenarios(&dir, || StoreSelection::Postgres(selection.clone()), branch)
    });
    if let Ok(mut client) = postgres::Client::connect(&url, postgres::NoTls) {
        let _ = client.batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"));
    }
    let _ = std::fs::remove_dir_all(&dir);
    match result {
        Ok(measured) => check("postgresql", &measured, POSTGRESQL),
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

fn scratch(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs768-mount-{label}-{}", std::process::id()));
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

/// Inits a one-file project and forks Branch `work` from its Layer.
fn initialize(
    dir: &Path,
    storage: &Storage,
    history: &dyn HistoryCatalog,
    acquisition: &dyn layerfs_storage::port::acquisition::Acquisition,
) -> BranchId {
    let source = dir.join("source");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("seed"), b"seed\n").unwrap();
    let initialized = Timing::disabled("init", |timing| {
        init(
            storage,
            history,
            InitRequest {
                source: &source,
                acquisition,
                stack: LayerStackId::from_authority([76; 16]),
                name: HistoryName::new(PROJECT).unwrap(),
                scope_seed: [76; 32],
                deadline: Instant::now() + Duration::from_secs(60),
            },
            timing,
        )
    })
    .0
    .unwrap();
    let branch = BranchId::from_authority([77; 16]);
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
    fn by_name(&self, n: u8) -> BoundWorkspace {
        self.service
            .store
            .bind_named(
                self.owner.client(),
                NamedBindRequest {
                    workspace: Self::workspace(n),
                    project: PROJECT.to_owned(),
                    branch: BRANCH.to_owned(),
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

/// One scenario's counted work per session group.
struct Measured {
    scenario: &'static str,
    counts: [Counts; 3],
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
    scenario: &'static str,
    out: &mut Vec<Measured>,
    mount: impl FnOnce() -> BoundWorkspace,
) -> BoundWorkspace {
    let before = opened.work();
    let workspace = mount();
    let after = opened.work();
    let mut measured = [[0; 4]; 3];
    for (slot, (after, before)) in measured.iter_mut().zip(after.iter().zip(before.iter())) {
        *slot = counts(&after.since(before));
    }
    out.push(Measured {
        scenario,
        counts: measured,
    });
    workspace
}

fn scenarios(
    dir: &Path,
    selection: impl Fn() -> StoreSelection,
    branch: BranchId,
) -> Vec<Measured> {
    let mut out = Vec::new();
    let opened = Opened::open(dir, selection(), "a");
    let first = measure(&opened, "mount.by_id.first", &mut out, || {
        opened.by_id(1, branch)
    });
    opened.unmount(first);
    let again = measure(&opened, "mount.by_id.again", &mut out, || {
        opened.by_id(2, branch)
    });
    opened.unmount(again);
    opened.stop();

    let opened = Opened::open(dir, selection(), "b");
    let named = measure(&opened, "mount.by_name.first", &mut out, || {
        opened.by_name(3)
    });
    opened.unmount(named);
    opened.stop();
    out
}

/// Prints every scenario, then compares each with its pin.
fn check(backend: &str, measured: &[Measured], pins: &[Pin]) {
    for m in measured {
        println!(
            "MOUNT_COUNTS {{\"backend\":\"{backend}\",\"scenario\":\"{}\",\"writer\":{:?},\"control\":{:?},\"workspace\":{:?}}}",
            m.scenario, m.counts[0], m.counts[1], m.counts[2]
        );
    }
    assert_eq!(measured.len(), pins.len());
    for (m, pin) in measured.iter().zip(pins) {
        assert_eq!(m.scenario, pin.scenario);
        assert_eq!(
            m.counts,
            [pin.writer, pin.control, pin.workspace],
            "{backend} {}: [writer, control, workspace] x [connects, round_trips, statements, transactions]",
            m.scenario
        );
    }
}
