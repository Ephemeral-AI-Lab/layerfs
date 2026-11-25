//! #810 F1/F4: the pinned writer work of the Commit phases that read the
//! Store before the lane (`construct`, `finish`, `logical.count`,
//! `bindings`), for small shapes of the measured cells, on SQLite and on
//! PostgreSQL, with the Save's storage outcome and the resulting root.
//!
//! A one-file Init; then, on one Store open, a Commit of a tree of
//! 4 x 4 x 4 x 4 = 256 leaf directories, four levels deep, with 64 files
//! each (16,384 files) of 109-byte names, and three Commits that each add
//! one file to every leaf directory (so each directory page has a delta
//! chain). Each scenario then runs on a newly opened Store (cold caches, as
//! a fresh daemon) with a newly bound Workspace: a warm-up Commit, then its
//! change, outside the count, and one counted Commit:
//!
//! - `edits` (C4): one file rewritten in every leaf directory, 256 edits
//!   spread over the inode table; no directory listing changes.
//! - `adds` (C4 setup 4): one new file in every leaf directory, 256
//!   changed directories.
//! - `one` (B3/D2): one file rewritten, the cheap shape.
//!
//! Counts come from the writer session's own counters (`SqlWork`), read at
//! each phase's start and end (`attribute_sql`): PostgreSQL round trips and
//! statements, SQLite statements (its round trips are zero). The Workspace
//! reader's total is pinned too. Each scenario's Save outcome (`inserted`,
//! `reused`, `packs`, `stored_bytes`) and root are pinned (every change is
//! stamped with a fixed clock): a read-path change must leave them
//! identical, and they are the same on both backends.
//!
//! The PostgreSQL half needs `LAYERFS_TEST_POSTGRES_URL` and reports
//! NOT_RUN without it.
use layerfs_history::records::CommitOutcome;
use layerfs_history::{
    BranchId, ForkRequest, ForkSource, HistoryCatalogConfig, HistoryName, LayerStackId, WorkspaceId,
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
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const BINDING: &[u8] = b"commit-batched-reads";
const CURSOR: [u8; 32] = [81; 32];
const FANOUT: usize = 4;
/// Uncounted Commits that each add one file to every leaf directory
/// before the counted one, so each directory page has a delta chain.
const ADD_ROUNDS: usize = 3;
const FILES: usize = 64;
/// The phases pinned, in order.
const PHASES: [&str; 4] = ["construct", "finish", "logical.count", "bindings"];

/// Writer `[round_trips, statements]` per phase of [`PHASES`], the writer's
/// whole Commit, and the Workspace readers' whole Commit.
type Work = [[u64; 2]; 6];

/// One scenario's pin: its work, the Save's `[inserted, reused, packs,
/// stored_bytes]` and the root.
struct Pin {
    scenario: &'static str,
    work: Work,
    saved: [u64; 4],
    root: &'static str,
}

struct Measured {
    scenario: &'static str,
    work: Work,
    saved: [u64; 4],
    root: String,
}

// Base (e11d8c8d0f), PostgreSQL writer round trips [construct, finish,
// logical.count, bindings, total]: edits [803, 3, 263, 393, 1463], adds
// [1421, 261, 200, 201, 2085], one [10, 4, 3, 0, 18]; SQLite statements:
// edits [1540, 3, 264, 393, 2219], adds [2717, 262, 203, 209, 3414], one
// [26, 5, 13, 0, 58]. #810 F1 (directory windows) left them unchanged: these
// shapes' directory pages are served from the Save's caches (its effect is
// on cold directories, the C4s4 cell). #810 F4: one difference walk with
// key-bounded read-ahead for the logical count and the hints, and the
// Save's leaf reads and selection's base leaves rebuilt a window at a time
// with one value lookup each. The Workspace readers' reads are Lane C's
// (#810 F5); `adds`' remaining construct reads are the directory rewrites
// (#810 F3).
const SQLITE: &[Pin] = &[
    Pin {
        scenario: "edits",
        work: [[0, 365], [0, 2], [0, 12], [0, 0], [0, 398], [0, 200]],
        saved: [519, 0, 5, 38478],
        root: "9bc9a227567d0c246b1338f7e47ce57828917068926b918e1eae31253e07f533",
    },
    Pin {
        scenario: "adds",
        work: [[0, 1498], [0, 10], [0, 14], [0, 12], [0, 1557], [0, 0]],
        saved: [782, 0, 53, 52968],
        root: "35a3e3818cb191f289b83e80492d02d6b3d602688db0e0c106d2b7dfe439eeb4",
    },
    Pin {
        scenario: "one",
        work: [[0, 26], [0, 5], [0, 13], [0, 0], [0, 58], [0, 0]],
        saved: [5, 0, 2, 573],
        root: "70ed856c5cc7ac85d11a4710a8abe1f6e2b81f9ad25c7c2158e2fa79c11204a8",
    },
];
const POSTGRESQL: &[Pin] = &[
    Pin {
        scenario: "edits",
        work: [[79, 64], [2, 2], [10, 10], [0, 0], [92, 86], [200, 200]],
        saved: [519, 0, 5, 38478],
        root: "9bc9a227567d0c246b1338f7e47ce57828917068926b918e1eae31253e07f533",
    },
    Pin {
        scenario: "adds",
        work: [[925, 776], [9, 7], [10, 10], [4, 4], [950, 808], [0, 0]],
        saved: [782, 0, 53, 52968],
        root: "35a3e3818cb191f289b83e80492d02d6b3d602688db0e0c106d2b7dfe439eeb4",
    },
    Pin {
        scenario: "one",
        work: [[10, 10], [4, 4], [3, 3], [0, 0], [18, 27], [0, 0]],
        saved: [5, 0, 2, 573],
        root: "70ed856c5cc7ac85d11a4710a8abe1f6e2b81f9ad25c7c2158e2fa79c11204a8",
    },
];

#[test]
fn sqlite_commit_batched_reads() {
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
    let measured = scenarios(&dir, StoreSelection::Sqlite(config), branch);
    std::fs::remove_dir_all(&dir).unwrap();
    check("sqlite", &measured, SQLITE);
}

#[test]
fn postgresql_commit_batched_reads() {
    let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
        return;
    };
    let dir = scratch("postgresql");
    let schema = format!("lfs810_cbr_{}", std::process::id());
    let selection = PostgresSelection::new(&url, &schema);
    let result = std::panic::catch_unwind(|| {
        let handles =
            PostgresHandles::create(&selection, StoragePolicy::frozen_default(), &catalog())
                .unwrap();
        let storage = Storage::new(handles.storage.clone()).unwrap();
        let branch = initialize(&dir, &storage, &handles.history, &handles.acquisition);
        drop(storage);
        drop(handles);
        scenarios(&dir, StoreSelection::Postgres(selection.clone()), branch)
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

/// The Branch every session binds.
fn branch_id() -> BranchId {
    BranchId::from_authority([82; 16])
}

fn scratch(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs810-cbr-{label}-{}", std::process::id()));
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

/// Every leaf directory, `/d0/d1/d2/d3`.
fn leaves() -> Vec<String> {
    let mut out = vec![String::new()];
    for _ in 0..4 {
        out = out
            .iter()
            .flat_map(|parent| (0..FANOUT).map(move |n| format!("{parent}/d{n}")))
            .collect();
    }
    out
}

/// File `n`'s name: long, so a leaf directory's listing fills about one
/// page and the directory pages a Commit reads pass the read caches.
fn name(n: usize) -> String {
    format!("{}{n:04}", "long-file-name-".repeat(7))
}

fn body(path: &str, round: u32) -> Vec<u8> {
    format!("{path} round {round}\n").repeat(3).into_bytes()
}

/// Inits the tree and forks Branch `work` from its Layer.
fn initialize(
    dir: &Path,
    storage: &Storage,
    history: &dyn layerfs_history::HistoryCatalog,
    acquisition: &dyn layerfs_storage::port::acquisition::Acquisition,
) -> BranchId {
    let source = dir.join("source");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("seed"), b"seed\n").unwrap();
    // Fixed times, so the Init root is the same in every run.
    let at = std::time::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
    for path in [source.join("seed"), source.clone()] {
        std::fs::File::open(&path)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_accessed(at).set_modified(at))
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
                name: HistoryName::new("cbr").unwrap(),
                scope_seed: [81; 32],
                deadline: Instant::now() + Duration::from_secs(60),
            },
            timing,
        )
    })
    .0
    .unwrap();
    let branch = branch_id();
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

/// Runs `body` on a newly opened Store (cold caches, as a fresh daemon) with
/// a newly bound Workspace of the Branch head, then closes both.
fn session<T>(
    dir: &Path,
    selection: &StoreSelection,
    n: u8,
    body: impl FnOnce(&StoreService, &BoundWorkspace) -> T,
) -> T {
    let opened = open_store_observed(
        selection.clone(),
        BINDING,
        CURSOR,
        OwnerConfig::default().namespaces,
        64 * 1024 * 1024,
        ReservationBlocks::default(),
    )
    .unwrap();
    let overlay = dir.join(format!("overlay{n}"));
    std::fs::create_dir_all(&overlay).unwrap();
    let owner = Owner::start(
        &overlay.join("overlay.sqlite"),
        layerfs_overlay::ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let service = StoreService::new(opened, &owner);
    service.opened.prepare_mounts().unwrap();
    let workspace = service
        .store
        .bind(
            owner.client(),
            BindRequest {
                branch: branch_id(),
                workspace: WorkspaceId::from_authority([n; 32]).unwrap(),
            },
        )
        .unwrap()
        .workspace;
    let value = body(&service, &workspace);
    let namespace = workspace.route().namespace();
    workspace.close().unwrap();
    owner.client().retire_namespace(namespace).unwrap();
    drop(service);
    owner.stop().unwrap();
    value
}

/// A warm-up Commit of a one-file change, then `change` and the counted
/// Commit.
fn counted(
    service: &StoreService,
    workspace: &BoundWorkspace,
    scenario: &'static str,
    change_with: impl FnOnce(&mut Mutation<'_>),
) -> Measured {
    change(workspace, |m| {
        m.write("/warm", scenario.as_bytes(), None).unwrap()
    });
    commit(workspace);
    change(workspace, change_with);
    measure(service, workspace, scenario)
}

fn scenarios(dir: &Path, selection: StoreSelection, branch: BranchId) -> Vec<Measured> {
    assert_eq!(branch, branch_id());
    let leaves = leaves();
    // The tree, then rounds of one new file in every leaf directory, so each
    // directory page has a delta chain.
    session(dir, &selection, 1, |_, workspace| {
        change(workspace, |m| {
            let mut made = std::collections::BTreeSet::new();
            for leaf in &leaves {
                for (at, _) in leaf.match_indices('/').skip(1) {
                    if made.insert(leaf[..at].to_owned()) {
                        m.mkdir(&leaf[..at], None).unwrap();
                    }
                }
                m.mkdir(leaf, None).unwrap();
                for n in 0..FILES {
                    let path = format!("{leaf}/{}", name(n));
                    m.write(&path, &body(&path, 0), None).unwrap();
                }
            }
        });
        commit(workspace);
        for round in 0..ADD_ROUNDS {
            change(workspace, |m| {
                for leaf in &leaves {
                    let path = format!("{leaf}/{}", name(100 + round));
                    m.write(&path, &body(&path, 2), None).unwrap();
                }
            });
            commit(workspace);
        }
    });
    let mut out = Vec::new();
    out.push(session(dir, &selection, 2, |service, workspace| {
        counted(service, workspace, "edits", |m| {
            for leaf in &leaves {
                let path = format!("{leaf}/{}", name(3));
                m.write(&path, &body(&path, 1), None).unwrap();
            }
        })
    }));
    out.push(session(dir, &selection, 3, |service, workspace| {
        counted(service, workspace, "adds", |m| {
            for leaf in &leaves {
                let path = format!("{leaf}/{}", name(100 + ADD_ROUNDS));
                m.write(&path, &body(&path, 2), None).unwrap();
            }
        })
    }));
    out.push(session(dir, &selection, 4, |service, workspace| {
        counted(service, workspace, "one", |m| {
            let path = format!("{}/{}", leaves[7], name(5));
            m.write(&path, &body(&path, 3), None).unwrap();
        })
    }));
    out
}

/// One change, stamped with a fixed clock so the same change gives the same
/// root in every run and on both backends.
fn change(workspace: &BoundWorkspace, body: impl FnOnce(&mut Mutation<'_>)) {
    let mut m = Mutation::new(workspace).unwrap();
    m.now = layerfs_workspace::Time {
        seconds: 1_800_000_000,
        nanoseconds: 0,
    };
    body(&mut m);
    m.finish().unwrap();
}

fn commit(workspace: &BoundWorkspace) -> layerfs_store_service::store::CommitSuccess {
    workspace
        .commit_captured(&CommitOptions {
            force: false,
            message: Some("batched reads".to_owned()),
            expected: None,
        })
        .unwrap_or_else(|failure| panic!("commit: {failure}"))
}

fn pair(work: &SqlWork) -> [u64; 2] {
    [work.round_trips, work.statements]
}

fn measure(service: &StoreService, workspace: &BoundWorkspace, scenario: &'static str) -> Measured {
    let sessions = service.opened.sql_sessions();
    let (writer, readers, _) = sessions.totals();
    let (writer, readers) = (writer.unwrap_or_default(), readers.unwrap_or_default());
    let recording = Recording::start();
    let probe = service.opened.sql_sessions();
    attribute_sql(Some(Box::new(move || probe.totals().0.unwrap_or_default())));
    let success = commit(workspace);
    let mut phases: BTreeMap<&str, SqlWork> = BTreeMap::new();
    for (name, work) in phase_sql() {
        phases.entry(name).or_default().accumulate(&work);
    }
    attribute_sql(None);
    drop(recording.finish());
    let (writer_after, readers_after, _) = sessions.totals();
    let mut work: Work = [[0; 2]; 6];
    for (slot, name) in work.iter_mut().zip(PHASES) {
        *slot = pair(&phases.get(name).copied().unwrap_or_default());
    }
    work[4] = pair(&writer_after.unwrap_or_default().since(&writer));
    work[5] = pair(&readers_after.unwrap_or_default().since(&readers));
    let root = match success.history.expect("a published Commit") {
        CommitOutcome::Committed { record, .. } => record.root.to_string(),
        other => panic!("{scenario}: {other:?}"),
    };
    let saved = success.saved;
    Measured {
        scenario,
        work,
        saved: [
            saved.inserted,
            saved.reused,
            saved.packs,
            saved.stored_bytes,
        ],
        root,
    }
}

fn check(backend: &str, measured: &[Measured], pins: &[Pin]) {
    for m in measured {
        println!(
            "BATCHED_READS {{\"backend\":\"{backend}\",\"scenario\":\"{}\",\"work\":{:?},\"saved\":{:?},\"root\":\"{}\"}}",
            m.scenario, m.work, m.saved, m.root
        );
    }
    assert_eq!(measured.len(), pins.len(), "{backend}: pins");
    for (m, pin) in measured.iter().zip(pins) {
        assert_eq!(m.scenario, pin.scenario);
        assert_eq!(
            m.work, pin.work,
            "{backend} {}: writer [round_trips, statements] for construct, finish, logical.count, bindings, writer total, Workspace readers",
            m.scenario
        );
        assert_eq!(
            m.saved, pin.saved,
            "{backend} {}: [inserted, reused, packs, stored_bytes]",
            m.scenario
        );
        assert_eq!(m.root, pin.root, "{backend} {}: root", m.scenario);
    }
}
