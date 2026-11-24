//! #768: the pinned SQL work of one Workspace mount through the Store
//! service, by Branch id and by project and Branch name, on SQLite and on
//! PostgreSQL; and of the Commits and the unmount of one mounted Workspace.
//!
//! Each mount scenario is the first mount on a newly opened Store (`first`),
//! or a second mount on the same Store after the first one closed (`again`).
//! The Commit scenarios run on a third Store open, after one mount: the
//! first and the second Commit of a one-file change, an UpToDate Commit,
//! then the unmount (`BoundWorkspace::close` and the overlay namespace's
//! retirement). They cover the whole Store-service Commit, the pre-lane
//! whiteout read (`materialize_commit`) included; the lane itself is pinned
//! by persistence `postgresql_commit_round_trips.rs`.
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
use layerfs_store_service::bootstrap::open_store_observed;
use layerfs_store_service::store::{BindRequest, BoundWorkspace, CommitOptions, NamedBindRequest};
use layerfs_store_service::write::mutation::Mutation;
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

// The Commit scenarios, measured by [`commit_scenarios`]. A real Commit's
// whiteout step (`materialize_commit`) reads the bound node's serial and the
// project's markers on the control reader. Before #768's mount change
// (measured on this branch with only `hidden_paths` put back to its
// d3fe2c4506 form, two reads: `node_history_serial`, then `path_removals` in
// a Read transaction) that cost SQLite 6 statements in 2 transactions per
// Commit; now it is one read (3 statements, 1 transaction). The writer and
// the Workspace reader are unchanged by it, and an UpToDate Commit never
// reaches the whiteout step.
//
// #768's Commit cuts then took 6 statements and 2 transactions off each real
// Commit's writer (first 52/11, second 44/9 before): no write transaction
// for a digest window of only small files (BEGIN IMMEDIATE, COMMIT), one
// statement for the Branch, its roots and its project epoch in
// `commit_branch` (three before), and one publication at a Save's finish
// for its last wave and its sealed tail (two before).
//
// #768's stage-1 cuts then took three reads off a real Commit. Validation
// checks the candidate from the objects its construction built, not read
// back through the Workspace reader (5 statements before, on both Commits).
// The whiteout step uses the bound node's markers read at mount, and a
// Commit this Workspace published hides nothing: no control read (3
// statements, 1 transaction before, on both). `logical.known` answers the
// size of the Commit this Workspace published from memory: the second
// Commit's writer drops its Read transaction (38/7 before); the first still
// reads the base Layer's, which the bind snapshot does not carry.
const SQLITE_COMMIT: &[Pin] = &[
    Pin {
        scenario: "commit.first",
        writer: [0, 0, 46, 9],
        control: [0, 0, 0, 0],
        workspace: [0, 0, 0, 0],
    },
    Pin {
        scenario: "commit.second",
        writer: [0, 0, 35, 6],
        control: [0, 0, 0, 0],
        workspace: [0, 0, 0, 0],
    },
    Pin {
        scenario: "commit.up_to_date",
        writer: [0, 0, 3, 1],
        control: [0, 0, 0, 0],
        workspace: [0, 0, 0, 0],
    },
    Pin {
        scenario: "unmount",
        writer: [0, 0, 3, 1],
        control: [0, 0, 0, 0],
        workspace: [0, 0, 0, 0],
    },
];

// On PostgreSQL the mount no longer opens the control reader, so the first
// Commit's whiteout read opens it (1 connect). Measured the same way, the
// two reads were control [1, 7, 3, 1] on the first Commit and [0, 4, 2, 1]
// on the second (a fenced autocommit statement, then a Read transaction:
// `BEGIN` batch, statement, `COMMIT`); the one statement is now 4 and 3
// round trips fewer. The writer's first Commit opens the lane connection.
// The unmount is the owner's release on the writer, cold on this session (2
// statements, each prepared once): `postgresql_gc_sessions` pins the warm
// release at 2 round trips, 8 before #768's admission change.
//
// #768's Commit cuts then took 11 round trips off the first Commit's writer
// (65 before) and 10 off the second's (42 before; 40 of them warm): the
// empty digest write (2), the last wave's own publication (4), the probe,
// epoch read and re-read as one statement (2), the lane check in the
// COMMIT round trip (1) and the head check in the lane's BEGIN batch (1).
// The first Commit also prepares the new joined statement once fewer than
// the three it replaces. Ledger and rules: `docs/architecture/engine/store/
// postgresql.md`, "Commit round trips".
//
// The stage-1 cuts (see the SQLite pins) then took the validation read-back
// (5 round trips, 5 statements before, on both Commits) and the whiteout
// read (first [1, 3, 2, 0] with the control reader's connect, second
// [0, 1, 1, 0]) off both Commits, so no Commit here opens the control
// reader; and `logical.known` off the second Commit's writer ([0, 32, 19,
// 4] before: that read was the Commit-row statement's first use on this
// session, a prepare and its execution, so 2 round trips; 1 once warm).
const POSTGRESQL_COMMIT: &[Pin] = &[
    Pin {
        scenario: "commit.first",
        writer: [1, 54, 23, 5],
        control: [0, 0, 0, 0],
        workspace: [0, 0, 0, 0],
    },
    Pin {
        scenario: "commit.second",
        writer: [0, 30, 18, 4],
        control: [0, 0, 0, 0],
        workspace: [0, 0, 0, 0],
    },
    Pin {
        scenario: "commit.up_to_date",
        writer: [0, 1, 1, 0],
        control: [0, 0, 0, 0],
        workspace: [0, 0, 0, 0],
    },
    Pin {
        scenario: "unmount",
        writer: [0, 4, 2, 0],
        control: [0, 0, 0, 0],
        workspace: [0, 0, 0, 0],
    },
];

/// What one test measures on a Store holding Branch `work`.
type Scenarios = fn(&Path, &dyn Fn() -> StoreSelection, BranchId) -> Vec<Measured>;

#[test]
fn sqlite_mount_counts() {
    check("sqlite", &on_sqlite("sqlite", scenarios), SQLITE);
}

#[test]
fn postgresql_mount_counts() {
    if let Some(measured) = on_postgresql("postgresql", "mount", scenarios) {
        check("postgresql", &measured, POSTGRESQL);
    }
}

#[test]
fn sqlite_commit_counts() {
    let measured = on_sqlite("sqlite-commit", commit_scenarios);
    check("sqlite", &measured, SQLITE_COMMIT);
}

#[test]
fn postgresql_commit_counts() {
    if let Some(measured) = on_postgresql("postgresql-commit", "commit", commit_scenarios) {
        check("postgresql", &measured, POSTGRESQL_COMMIT);
    }
}

#[test]
fn sqlite_merge_commit_validates_through_the_reader() {
    check_merge("sqlite", &on_sqlite("sqlite-merge", merge_scenarios));
}

#[test]
fn postgresql_merge_commit_validates_through_the_reader() {
    if let Some(measured) = on_postgresql("postgresql-merge", "merge", merge_scenarios) {
        check_merge("postgresql", &measured);
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
    let schema = format!("lfs768_{tag}_{}", std::process::id());
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
    /// One Workspace Commit, as the daemon's `workspace.commit` runs it.
    fn commit(&self, workspace: &BoundWorkspace) -> CommitOutcome {
        let success = workspace
            .commit_captured(&CommitOptions {
                force: false,
                message: Some("round trips".to_owned()),
                expected: None,
            })
            .unwrap_or_else(|failure| panic!("commit: {failure}"));
        success.history.expect("a published Commit")
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

fn measure<T>(
    opened: &Opened,
    scenario: &'static str,
    out: &mut Vec<Measured>,
    step: impl FnOnce() -> T,
) -> T {
    let before = opened.work();
    let value = step();
    let after = opened.work();
    let mut measured = [[0; 4]; 3];
    for (slot, (after, before)) in measured.iter_mut().zip(after.iter().zip(before.iter())) {
        *slot = counts(&after.since(before));
    }
    out.push(Measured {
        scenario,
        counts: measured,
    });
    value
}

fn scenarios(
    dir: &Path,
    selection: &dyn Fn() -> StoreSelection,
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

/// After one mount by Branch id on a newly opened Store: two Commits of a
/// one-file change, an UpToDate Commit, then the unmount. Each change is
/// written before its measured Commit, outside the count.
fn commit_scenarios(
    dir: &Path,
    selection: &dyn Fn() -> StoreSelection,
    branch: BranchId,
) -> Vec<Measured> {
    let mut out = Vec::new();
    let opened = Opened::open(dir, selection(), "c");
    let workspace = opened.by_id(4, branch);
    for (scenario, path) in [("commit.first", "/one"), ("commit.second", "/two")] {
        let mut change = Mutation::new(&workspace).unwrap();
        change.write(path, b"changed\n", None).unwrap();
        change.finish().unwrap();
        let outcome = measure(&opened, scenario, &mut out, || opened.commit(&workspace));
        assert!(
            matches!(outcome, CommitOutcome::Committed { .. }),
            "{scenario}: {outcome:?}"
        );
    }
    let outcome = measure(&opened, "commit.up_to_date", &mut out, || {
        opened.commit(&workspace)
    });
    assert!(
        matches!(outcome, CommitOutcome::UpToDate { .. }),
        "{outcome:?}"
    );
    measure(&opened, "unmount", &mut out, || opened.unmount(workspace));
    opened.stop();
    out
}

/// Two Workspaces on Branch `work`: the first Commits `/a`; the second,
/// bound before it, Commits `/b` over the moved head. Its merge builds the
/// root it publishes outside construction, so validation reads that root
/// through the Workspace reader (the fallback for objects construction did
/// not build) and the install still succeeds with both files. The second
/// Workspace's next Commit, over the merge it published, validates from
/// what it built again.
fn merge_scenarios(
    dir: &Path,
    selection: &dyn Fn() -> StoreSelection,
    branch: BranchId,
) -> Vec<Measured> {
    let mut out = Vec::new();
    let opened = Opened::open(dir, selection(), "d");
    let first = opened.by_id(5, branch);
    let second = opened.by_id(6, branch);
    let write = |workspace: &BoundWorkspace, path: &str| {
        let mut change = Mutation::new(workspace).unwrap();
        change.write(path, b"merged\n", None).unwrap();
        change.finish().unwrap();
    };
    let commit = |workspace: &BoundWorkspace| {
        workspace
            .commit_captured(&CommitOptions {
                force: false,
                message: Some("merge validation".to_owned()),
                expected: None,
            })
            .unwrap_or_else(|failure| panic!("commit: {failure}"))
    };
    write(&first, "/a");
    commit(&first);
    write(&second, "/b");
    let merged = measure(&opened, "commit.merge", &mut out, || commit(&second));
    assert!(merged.merge.is_some(), "the head moved: a merge runs");
    assert!(
        matches!(merged.history, Some(CommitOutcome::Committed { .. })),
        "{:?}",
        merged.history
    );
    let view = Mutation::new(&second).unwrap();
    for path in ["/a", "/b", "/seed"] {
        assert!(view.optional(path).unwrap().is_some(), "{path} installed");
    }
    // Its base source is released only by `finish`; a held one would keep
    // the next install waiting.
    view.finish().unwrap();
    write(&second, "/c");
    let after = measure(&opened, "commit.after_merge", &mut out, || commit(&second));
    assert!(after.merge.is_none(), "over its own head: no merge");
    assert!(
        matches!(after.history, Some(CommitOutcome::Committed { .. })),
        "{:?}",
        after.history
    );
    opened.unmount(first);
    opened.unmount(second);
    opened.stop();
    out
}

/// The merge's validation read through the Workspace reader; the Commit
/// after it read nothing there.
fn check_merge(backend: &str, measured: &[Measured]) {
    for m in measured {
        println!(
            "MERGE_COUNTS {{\"backend\":\"{backend}\",\"scenario\":\"{}\",\"writer\":{:?},\"control\":{:?},\"workspace\":{:?}}}",
            m.scenario, m.counts[0], m.counts[1], m.counts[2]
        );
    }
    let [merge, after] = measured else {
        panic!("two scenarios");
    };
    assert_eq!(merge.scenario, "commit.merge");
    assert!(
        merge.counts[2][2] > 0,
        "{backend}: the merged root is read back through the Workspace reader"
    );
    assert_eq!(after.scenario, "commit.after_merge");
    assert_eq!(
        after.counts[2],
        [0, 0, 0, 0],
        "{backend}: validated from the built objects"
    );
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
