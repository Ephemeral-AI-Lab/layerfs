//! layerfs-project #67: the fixed statements and transactions of a Commit's
//! Store calls on SQLite and PostgreSQL: a Branch snapshot, a reservation, a
//! Save's publication and the lane-held head check and publication.
//! Each line printed `COUNT` is one call's counter delta on one session.
//!
//! Needs `LAYERFS_TEST_POSTGRES_URL` for the PostgreSQL half; NOT_RUN
//! without it.
#![cfg(feature = "postgresql")]
mod support;

use layerfs_content::{construct_bytes, ObjectId};
use layerfs_history::records::{
    CommitMeta, CommitOutcome, CommitRequest, CommitVia, ForkRequest, ForkSource,
    StackInitialization,
};
use layerfs_history::{BranchId, HistoryCatalog, HistoryCatalogConfig};
use layerfs_persistence::{
    PersistenceConfig, PostgresHandles, PostgresSelection, SqlWork, SqliteHandles,
    SqlitePersistenceProfile,
};
use layerfs_storage::{
    port::{PackPersistence, Reserve},
    Storage, StoragePolicy,
};
use layerfs_telemetry::timer::Timing;
use std::sync::Arc;
use support::{branch_id, name, root, stack, Temp};

const BINDING: &[u8] = b"postgresql-commit-round-trips";
const CURSOR: [u8; 32] = [67; 32];

fn catalog() -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: BINDING.to_vec(),
        cursor_key: CURSOR,
        incarnation: 3,
    }
}

/// Drops the test's schema when the test ends, pass or fail.
struct Schema {
    url: String,
    name: String,
}
impl Drop for Schema {
    fn drop(&mut self) {
        if let Ok(mut client) = postgres::Client::connect(&self.url, postgres::NoTls) {
            let _ = client.batch_execute(&format!("DROP SCHEMA IF EXISTS {} CASCADE", self.name));
        }
    }
}

fn bytes(seed: u32, len: usize) -> Vec<u8> {
    let mut state = seed.wrapping_mul(2654435761) | 1;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state >> 24) as u8
        })
        .collect()
}

/// One Save of `files`; returns the last root.
fn save(storage: &Storage, files: &[Vec<u8>]) -> ObjectId {
    let save = storage.begin_save().unwrap();
    let mut last = None;
    for file in files {
        let root = Timing::disabled("construct", |scope| {
            construct_bytes(
                storage.policy().construction(),
                &storage.policy().construction().capacities(),
                file,
                &mut save.sink(),
                scope.child("content"),
            )
        })
        .0
        .unwrap()
        .root;
        last = Some(root);
    }
    save.finish().unwrap();
    last.unwrap()
}

/// Counter delta of one call.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Delta {
    statements: u64,
    transactions: u64,
    commits: u64,
    rollbacks: u64,
    round_trips: u64,
    prepares: u64,
}
impl Delta {
    fn of(before: &SqlWork, after: &SqlWork) -> Self {
        Self {
            statements: after.statements - before.statements,
            transactions: after.transactions - before.transactions,
            commits: after.commits - before.commits,
            rollbacks: after.rollbacks - before.rollbacks,
            round_trips: after.round_trips - before.round_trips,
            prepares: after.prepares - before.prepares,
        }
    }
}

struct Engine<'a> {
    label: &'static str,
    storage: Storage,
    persistence: Arc<dyn PackPersistence>,
    history: &'a dyn HistoryCatalog,
    work: Box<dyn Fn() -> SqlWork + 'a>,
}

impl Engine<'_> {
    fn measure<T>(&self, phase: &str, f: impl FnOnce() -> T) -> (T, Delta) {
        let before = (self.work)();
        let value = f();
        let delta = Delta::of(&before, &(self.work)());
        // Round trips and prepares are counted on PostgreSQL only.
        eprintln!(
            "COUNT {} {phase}: statements={} transactions={} commits={} rollbacks={} \
             round_trips={} prepares={}",
            self.label,
            delta.statements,
            delta.transactions,
            delta.commits,
            delta.rollbacks,
            delta.round_trips,
            delta.prepares,
        );
        (value, delta)
    }
}

/// The measured calls on one engine; returns each phase's delta.
fn run(engine: &Engine<'_>, branch: BranchId) -> Vec<(&'static str, Delta)> {
    let mut out = Vec::new();
    let genesis = save(&engine.storage, &[b"genesis tree".to_vec()]);
    let record = engine
        .history
        .initialize_layerstack(&StackInitialization {
            logical: 0,
            stack: stack(1),
            name: name("main"),
            scope: root(200),
            profile: root(201),
            genesis_root: genesis,
        })
        .unwrap();
    engine
        .history
        .fork(&ForkRequest {
            stack: stack(1),
            branch,
            name: name("work"),
            source: ForkSource::Layer(record.head_layer),
        })
        .unwrap();
    // Warm each statement once so prepares do not count below.
    let warm = save(&engine.storage, &[bytes(1, 5000)]);
    let commit = |candidate: ObjectId| {
        let lane = engine.history.publish_lane().unwrap();
        let now = engine.history.branch_snapshot(branch).unwrap().unwrap();
        let outcome = engine
            .history
            .commit_branch(&CommitRequest {
                branch,
                expected_head: now.branch.head_commit,
                expected_base: now.branch.base_layer,
                expected_root: now.effective_root,
                candidate_root: candidate,
                profile: root(201),
                scope: root(200),
                second_parent: None,
                meta: CommitMeta::new(CommitVia::Files),
                force: false,
            })
            .unwrap();
        drop(lane);
        assert!(matches!(outcome, CommitOutcome::Committed { .. }));
    };
    commit(warm);
    let warm = save(&engine.storage, &[bytes(2, 5000)]);
    commit(warm);

    let (snapshot, delta) = engine.measure("branch_snapshot", || {
        engine.history.branch_snapshot(branch).unwrap().unwrap()
    });
    assert!(snapshot.branch.head_commit.is_some());
    out.push(("branch_snapshot", delta));
    let (_, delta) = engine.measure("reserve", || {
        engine
            .persistence
            .reserve(Reserve {
                packs: 4,
                ordinals: 0,
            })
            .unwrap()
    });
    out.push(("reserve", delta));
    let small: Vec<_> = (0..100).map(|n| bytes(100 + n, 3000)).collect();
    let (hundred, delta) = engine.measure("save 100 files", || save(&engine.storage, &small));
    out.push(("save 100 files", delta));
    let large = [bytes(7, 8 << 20)];
    let (_, delta) = engine.measure("save 8 MiB", || save(&engine.storage, &large));
    out.push(("save 8 MiB", delta));
    let (_, delta) = engine.measure("lane + head + commit_branch", || commit(hundred));
    out.push(("lane + head + commit_branch", delta));
    let (_, delta) = engine.measure("snapshot + 1 file save", || {
        let now = engine.history.branch_snapshot(branch).unwrap().unwrap();
        let again = save(&engine.storage, &[bytes(9, 5000)]);
        (now, again)
    });
    out.push(("snapshot + 1 file save", delta));
    out
}

#[test]
fn commit_store_calls_are_counted_on_both_engines() {
    let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
        return;
    };
    let temp = Temp::new("commit-round-trips");
    let sqlite = SqliteHandles::create(
        PersistenceConfig::sqlite(temp.join("store"))
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        StoragePolicy::frozen_default(),
        &catalog(),
    )
    .unwrap();
    let sqlite_engine = Engine {
        label: "sqlite",
        storage: Storage::new(sqlite.storage.clone()).unwrap(),
        persistence: sqlite.storage.clone() as Arc<dyn PackPersistence>,
        history: &sqlite.history,
        work: Box::new(|| sqlite.diagnostics().unwrap()),
    };
    let sqlite_counts = run(&sqlite_engine, branch_id(1));

    let schema = Schema {
        url: url.clone(),
        name: format!("lfs67_{}", std::process::id()),
    };
    let selection = PostgresSelection::new(&url, &schema.name);
    drop(PostgresHandles::create(&selection, StoragePolicy::frozen_default(), &catalog()).unwrap());
    let pg = PostgresHandles::open_writable(&selection, BINDING, CURSOR).unwrap();
    let pg_engine = Engine {
        label: "postgresql",
        storage: Storage::new(pg.storage.clone() as Arc<dyn PackPersistence>).unwrap(),
        persistence: pg.storage.clone() as Arc<dyn PackPersistence>,
        history: &pg.history,
        work: Box::new(|| pg.diagnostics().unwrap()),
    };
    let pg_counts = run(&pg_engine, branch_id(1));
    assert_eq!(sqlite_counts.len(), pg_counts.len());
    let pg = |phase: &str| pg_counts.iter().find(|(p, _)| *p == phase).unwrap().1;
    // #67: a Branch snapshot is one statement outside any transaction.
    let snapshot = pg("branch_snapshot");
    assert_eq!((snapshot.round_trips, snapshot.transactions), (1, 0));
    // A reservation is BEGIN, one UPDATE … RETURNING, COMMIT.
    assert_eq!(pg("reserve").round_trips, 3);
    // The lane: its BEGIN, the head read, commit_branch's BEGIN, joined
    // snapshot, Commit insert, parent line, lineage insert, advance and
    // COMMIT, and the lane's ROLLBACK.
    assert_eq!(pg("lane + head + commit_branch").round_trips, 10);
    let sqlite = |phase: &str| sqlite_counts.iter().find(|(p, _)| *p == phase).unwrap().1;
    // SQLite runs the same joined statement inside BEGIN/COMMIT.
    assert_eq!(sqlite("branch_snapshot").statements, 3);
    drop(schema);
}
