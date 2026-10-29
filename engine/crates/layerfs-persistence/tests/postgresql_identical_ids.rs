//! #506: one project on SQLite and on PostgreSQL gives identical object, root
//! and Commit ids. The project starts on SQLite (Init stays SQLite only), is
//! snapshotted and imported into PostgreSQL, then both engines take the same
//! further Saves and Commits.
//!
//! Needs a PostgreSQL server: set `LAYERFS_TEST_POSTGRES_URL` (for example
//! `host=127.0.0.1 port=55432 user=postgres password=... dbname=layerfs
//! sslmode=disable`). Without it the test reports NOT_RUN and passes nothing.
#![cfg(feature = "postgresql")]
mod support;

use layerfs_content::{construct_bytes, read_range, ObjectId};
use layerfs_history::records::{
    CommitMeta, CommitOutcome, CommitRecord, CommitVia, ForkRequest, ForkSource,
    StackInitialization,
};
use layerfs_history::{BranchId, HistoryCatalog, HistoryCatalogConfig};
use layerfs_persistence::{
    Handles, PersistenceConfig, PostgresHandles, PostgresSelection, SqliteAcquisitionSchema,
    SqlitePackLayout, SqlitePersistenceProfile, SynchronousCommit,
};
use layerfs_storage::{port::PackPersistence, Storage, StoragePolicy};
use layerfs_telemetry::timer::Timing;
use std::sync::Arc;
use support::{branch_id, name, root, stack, Temp};

const BINDING: &[u8] = b"postgresql-identical-ids";
const CURSOR: [u8; 32] = [29; 32];

fn sqlite(path: std::path::PathBuf) -> PersistenceConfig {
    PersistenceConfig::sqlite(path)
        .with_sqlite_profile(SqlitePersistenceProfile::Disposable)
        .with_sqlite_pack_layout(SqlitePackLayout::GroupRowsIndexed)
        .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables)
}

fn save(storage: &Storage, bytes: &[u8]) -> ObjectId {
    let save = storage.begin_save().unwrap();
    let root = Timing::disabled("construct", |scope| {
        construct_bytes(
            storage.policy().construction(),
            &storage.policy().construction().capacities(),
            bytes,
            &mut save.sink(),
            scope.child("content"),
        )
    })
    .0
    .unwrap()
    .root;
    save.finish().unwrap();
    root
}

fn read(storage: &Storage, id: ObjectId, len: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    Timing::disabled("read", |scope| {
        read_range(
            &storage.reader().unwrap(),
            id,
            0..len as u64,
            &mut bytes,
            scope.child("read"),
        )
    })
    .0
    .unwrap();
    bytes
}

fn commit(history: &dyn HistoryCatalog, branch: BranchId, candidate: ObjectId) -> CommitRecord {
    let record = history.branch(branch).unwrap().unwrap();
    let current = match record.head_commit {
        Some(head) => history.commit(head).unwrap().unwrap().root,
        None => history.layer(record.base_layer).unwrap().unwrap().root,
    };
    let outcome = history
        .commit_branch(&layerfs_history::records::CommitRequest {
            branch,
            expected_head: record.head_commit,
            expected_base: record.base_layer,
            expected_root: current,
            candidate_root: candidate,
            profile: root(201),
            scope: root(200),
            second_parent: None,
            meta: CommitMeta::new(CommitVia::Files),
            force: false,
        })
        .unwrap();
    let CommitOutcome::Committed { record, .. } = outcome else {
        panic!("{outcome:?}");
    };
    *record
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

/// The same work on one engine: a new file, an edited file, a Commit held in
/// the publication lane, and a second Commit.
fn continue_project(
    storage: &Storage,
    history: &dyn HistoryCatalog,
    branch: BranchId,
) -> (Vec<ObjectId>, Vec<CommitRecord>) {
    let added = save(storage, &bytes(3, 90_000));
    let edited = {
        let mut file = bytes(2, 400_000);
        file[123_456..123_556].copy_from_slice(&[7; 100]);
        save(storage, &file)
    };
    let lane = history.publish_lane().unwrap();
    let first = commit(history, branch, added);
    drop(lane);
    let second = commit(history, branch, edited);
    (vec![added, edited], vec![first, second])
}

#[test]
fn one_project_has_identical_ids_on_sqlite_and_postgresql() {
    let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
        return;
    };
    let temp = Temp::new("postgresql-ids");
    let created = Handles::create(
        sqlite(temp.join("store")),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: CURSOR,
            incarnation: 9,
        },
    )
    .unwrap();
    let sqlite_storage = Storage::new(created.storage.clone()).unwrap();
    let genesis_root = save(&sqlite_storage, b"genesis tree of the project");
    let stack_record = created
        .history
        .initialize_layerstack(&StackInitialization {
            logical: 0,
            stack: stack(1),
            name: name("main"),
            scope: root(200),
            profile: root(201),
            genesis_root,
        })
        .unwrap();
    let branch = branch_id(1);
    created
        .history
        .fork(&ForkRequest {
            stack: stack(1),
            branch,
            name: name("work"),
            source: ForkSource::Layer(stack_record.head_layer),
        })
        .unwrap();
    let file = bytes(2, 400_000);
    let first_root = save(&sqlite_storage, &file);
    let first = commit(&created.history, branch, first_root);

    // The project moves to PostgreSQL as a snapshot of the live SQLite Store.
    let copy = created.snapshot(&temp.join("copy")).unwrap();
    let schema = Schema {
        url: url.clone(),
        name: format!(
            "lfs506_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros()
        ),
    };
    let selection = PostgresSelection::new(&url, &schema.name);
    let receipt = PostgresHandles::import(sqlite(copy.path), &selection, BINDING, CURSOR).unwrap();
    let packs = receipt.tables.iter().find(|(t, _)| *t == "pack").unwrap().1;
    assert!(packs > 0 && receipt.pack_bytes > 0, "{receipt:?}");
    // An existing schema is refused, never merged into.
    assert!(
        PostgresHandles::import(sqlite(temp.join("copy")), &selection, BINDING, CURSOR).is_err()
    );
    // A wrong binding is refused at open.
    assert!(PostgresHandles::open_writable(&selection, b"another-binding", CURSOR).is_err());

    let pg = PostgresHandles::open_writable(
        &selection
            .clone()
            .with_synchronous_commit(SynchronousCommit::Off),
        BINDING,
        CURSOR,
    )
    .unwrap();
    let pg_storage = Storage::new(pg.storage.clone() as Arc<dyn PackPersistence>).unwrap();
    // The imported project is the same project.
    assert_eq!(read(&pg_storage, first_root, file.len()), file);
    assert_eq!(
        pg.history.branch(branch).unwrap(),
        created.history.branch(branch).unwrap()
    );
    let imported = pg.history.commit(first.id).unwrap().unwrap();
    assert_eq!((imported.id, imported.root), (first.id, first.root));
    assert_eq!(
        pg.history.layer_stack(stack(1)).unwrap(),
        created.history.layer_stack(stack(1)).unwrap()
    );

    // The same further work on both engines gives the same identities.
    let (sqlite_objects, sqlite_commits) =
        continue_project(&sqlite_storage, &created.history, branch);
    let (pg_objects, pg_commits) = continue_project(&pg_storage, &pg.history, branch);
    assert_eq!(sqlite_objects, pg_objects);
    let ids = |commits: &[CommitRecord]| {
        commits
            .iter()
            .map(|c| (c.id, c.root, c.parent, c.base_layer))
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&sqlite_commits), ids(&pg_commits));
    assert_eq!(pg_commits[0].parent, Some(first.id));
    assert_eq!(
        pg.history.branch(branch).unwrap().unwrap().head_commit,
        Some(pg_commits[1].id)
    );
    assert_eq!(
        pg.history.is_ancestor(
            layerfs_history::NodeId::Commit(first.id),
            layerfs_history::NodeId::Commit(pg_commits[1].id)
        ),
        created.history.is_ancestor(
            layerfs_history::NodeId::Commit(first.id),
            layerfs_history::NodeId::Commit(sqlite_commits[1].id)
        )
    );

    // A fresh read-only session sees every committed byte.
    let reader = PostgresHandles::open_read_only(&selection, BINDING, CURSOR).unwrap();
    let reader_storage = Storage::new(reader.storage.clone() as Arc<dyn PackPersistence>).unwrap();
    assert_eq!(
        read(&reader_storage, pg_objects[0], 90_000),
        read(&sqlite_storage, sqlite_objects[0], 90_000)
    );
    assert_eq!(
        read(&reader_storage, pg_objects[1], 400_000),
        read(&sqlite_storage, sqlite_objects[1], 400_000)
    );
    // A read-only session refuses writes before any statement.
    assert!(reader
        .history
        .fork(&ForkRequest {
            stack: stack(1),
            branch: branch_id(2),
            name: name("refused"),
            source: ForkSource::Layer(stack_record.head_layer),
        })
        .is_err());
    let totals = pg.history.statistics().unwrap();
    assert!(totals.canonical > 0, "{totals:?}");
    assert!(pg.diagnostics().unwrap().write_commits > 0);
    drop(schema);
}

#[test]
fn a_held_lane_makes_another_writer_wait_then_fail_busy_once() {
    let Ok(url) = std::env::var("LAYERFS_TEST_POSTGRES_URL") else {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
        return;
    };
    let temp = Temp::new("postgresql-lane");
    let created = Handles::create(
        sqlite(temp.join("store")),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: CURSOR,
            incarnation: 9,
        },
    )
    .unwrap();
    created
        .history
        .initialize_layerstack(&StackInitialization {
            logical: 0,
            stack: stack(1),
            name: name("main"),
            scope: root(200),
            profile: root(201),
            genesis_root: root(0),
        })
        .unwrap();
    let copy = created.snapshot(&temp.join("copy")).unwrap();
    let schema = Schema {
        url: url.clone(),
        name: format!("lfs506_lane_{}", std::process::id()),
    };
    let mut selection = PostgresSelection::new(&url, &schema.name);
    PostgresHandles::import(sqlite(copy.path), &selection, BINDING, CURSOR).unwrap();
    selection.lock_timeout_ms = 200;
    let holder = PostgresHandles::open_writable(&selection, BINDING, CURSOR).unwrap();
    let other = PostgresHandles::open_writable(&selection, BINDING, CURSOR).unwrap();
    let genesis = holder
        .history
        .layer_stack(stack(1))
        .unwrap()
        .unwrap()
        .head_layer;
    let fork = |history: &dyn HistoryCatalog, n: u8| {
        history.fork(&ForkRequest {
            stack: stack(1),
            branch: branch_id(n),
            name: name(&format!("b{n}")),
            source: ForkSource::Layer(genesis),
        })
    };
    let lane = holder.history.publish_lane().unwrap();
    // The holder's own writes run inside its lane.
    fork(&holder.history, 1).unwrap();
    let started = std::time::Instant::now();
    let refused = fork(&other.history, 2);
    assert_eq!(refused.err(), Some(layerfs_history::HistoryError::Busy));
    assert!(started.elapsed() >= std::time::Duration::from_millis(150));
    drop(lane);
    // Once released, the other writer proceeds (a new call, not a retry).
    fork(&other.history, 2).unwrap();
    assert!(other.history.branch(branch_id(1)).unwrap().is_some());
    drop(schema);
}
