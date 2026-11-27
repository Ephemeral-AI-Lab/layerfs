//! T1.4: a Store written before the position state opens writable and gains
//! `history_meta.next_history_serial`, the backfilled `commit`/`layer`
//! serials and one `project_epoch` row per project. A Store that already has
//! the column is left alone, and a serial already recorded is never lowered.
#![cfg(any(target_os = "macos", target_os = "linux"))]
#[path = "../support/mod.rs"]
mod support;

use layerfs_history::{
    AddLayerOutcome, AddLayerRequest, BranchId, CommitMeta, CommitOutcome, CommitRecord,
    CommitRequest, CommitVia, ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig,
    LayerRecord, LayerStackId, StackInitialization,
};
use layerfs_persistence::{PersistenceConfig, SqliteHandles, SqlitePersistenceProfile};
use layerfs_storage::StoragePolicy;
use support::{branch_id, name, root, stack, Temp, BINDING};

/// The `history_meta` position column as `history.sql` writes it.
const NEXT_SERIAL: &str = "    next_history_serial INTEGER NOT NULL DEFAULT 1,\n";

fn config(path: &std::path::Path) -> PersistenceConfig {
    PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Normal)
}

fn create(path: &std::path::Path) -> SqliteHandles {
    SqliteHandles::create(
        config(path),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: [71; 32],
            incarnation: 7,
        },
    )
    .expect("Store creation")
}

fn writable(
    path: &std::path::Path,
) -> Result<SqliteHandles, layerfs_storage::port::PersistenceError> {
    SqliteHandles::open_writable(config(path), BINDING, [71; 32])
}

fn definition(sql: &rusqlite::Connection) -> String {
    let mut statement = sql
        .prepare("SELECT type,name,sql FROM sqlite_schema WHERE substr(name,1,7)!='sqlite_' ORDER BY type,name")
        .unwrap();
    let rows = statement
        .query_map([], |r| Ok([r.get::<_, String>(0)?, r.get(1)?, r.get(2)?]))
        .unwrap();
    rows.map(|r| r.unwrap().map(|v| v + "\n").concat())
        .collect()
}

/// Rewrites a fresh Store into the format written before T1.4: no
/// `history_meta.next_history_serial`, every Commit and Layer at serial 0 and
/// no `project_epoch` row, recorded as that revision wrote it. Returns the
/// fresh source string it replaced so a test can assert the migrated Store
/// records the current schema again.
fn strip_position_state(path: &std::path::Path) -> String {
    let sql = rusqlite::Connection::open(path).unwrap();
    let fresh: String = sql
        .query_row("SELECT schema_source FROM history_meta", [], |r| r.get(0))
        .unwrap();
    sql.execute_batch(
        "ALTER TABLE history_meta DROP COLUMN next_history_serial; \
         UPDATE \"commit\" SET history_serial=0; \
         UPDATE layer SET history_serial=0; \
         DELETE FROM project_epoch;",
    )
    .unwrap();
    let old = fresh.replacen(NEXT_SERIAL, "", 1);
    assert_ne!(old, fresh, "a fresh Store holds the position column");
    sql.execute(
        "UPDATE history_meta SET schema_source=?1,schema_definition=?2",
        rusqlite::params![old, definition(&sql)],
    )
    .unwrap();
    fresh
}

fn initialize(handles: &SqliteHandles, tag: u8) -> layerfs_history::LayerStackRecord {
    handles
        .history
        .initialize_layerstack(&StackInitialization {
            logical: 10,
            stack: stack(tag),
            name: name(&format!("project-{tag}")),
            scope: root(200),
            profile: root(201),
            genesis_root: root(1),
        })
        .unwrap()
}

fn fork(
    handles: &SqliteHandles,
    stack: LayerStackId,
    branch: BranchId,
    tag: u8,
    source: ForkSource,
) {
    handles
        .history
        .fork(&ForkRequest {
            stack,
            branch,
            name: name(&format!("branch-{tag}")),
            source,
        })
        .unwrap();
}

fn commit(handles: &SqliteHandles, branch: BranchId, tag: u8) -> CommitRecord {
    let snapshot = handles.history.branch(branch).unwrap().unwrap();
    let expected_root = match snapshot.head_commit {
        Some(head) => handles.history.commit(head).unwrap().unwrap().root,
        None => {
            handles
                .history
                .layer(snapshot.base_layer)
                .unwrap()
                .unwrap()
                .root
        }
    };
    match handles
        .history
        .commit_branch(&CommitRequest {
            branch,
            expected_head: snapshot.head_commit,
            expected_base: snapshot.base_layer,
            expected_root,
            candidate_root: root(tag),
            profile: root(201),
            scope: root(200),
            second_parent: None,
            meta: CommitMeta::new(CommitVia::Files),
            force: false,
            required_head: None,
        })
        .unwrap()
    {
        CommitOutcome::Committed { record, .. } => *record,
        other => panic!("commit: {other:?}"),
    }
}

fn publish(handles: &SqliteHandles, stack: LayerStackId, branch: BranchId) -> LayerRecord {
    let branch_record = handles.history.branch(branch).unwrap().unwrap();
    let stack_record = handles.history.layer_stack(stack).unwrap().unwrap();
    match handles
        .history
        .add_layer(&AddLayerRequest {
            stack,
            branch,
            commit: branch_record.head_commit.expect("a committed Branch"),
            expected_stack_head: stack_record.head_layer,
            expected_branch_base: branch_record.base_layer,
        })
        .unwrap()
    {
        AddLayerOutcome::Added { layer } => layer,
        other => panic!("publish: {other:?}"),
    }
}

fn column_default(sql: &rusqlite::Connection, table: &str, column: &str) -> (i64, Option<String>) {
    sql.query_row(
        "SELECT \"notnull\",dflt_value FROM pragma_table_info(?1) WHERE name=?2",
        rusqlite::params![table, column],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
    .unwrap()
}

fn next_serial(sql: &rusqlite::Connection) -> i64 {
    sql.query_row("SELECT next_history_serial FROM history_meta", [], |r| {
        r.get(0)
    })
    .unwrap()
}

fn epochs(sql: &rusqlite::Connection) -> Vec<(Vec<u8>, i64)> {
    let mut statement = sql
        .prepare("SELECT layer_stack_id,epoch FROM project_epoch ORDER BY layer_stack_id")
        .unwrap();
    statement
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect()
}

#[test]
fn an_empty_store_keeps_its_next_serial_and_gains_no_epoch() {
    let t = Temp::new("gc-backfill-empty");
    let path = t.join("db");
    drop(create(&path));
    let fresh = strip_position_state(&path);

    writable(&path).expect("a pre-T1.4 Store opens writable");
    drop(writable(&path).expect("an upgraded Store reopens"));

    let sql = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(
        column_default(&sql, "history_meta", "next_history_serial"),
        (1, Some("1".to_owned())),
        "the column is appended NOT NULL DEFAULT 1"
    );
    assert_eq!(next_serial(&sql), 1, "no history keeps the serial at 1");
    assert!(
        epochs(&sql).is_empty(),
        "a Store with no project has no epoch"
    );
    let (source, recorded): (String, String) = sql
        .query_row(
            "SELECT schema_source,schema_definition FROM history_meta",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(source, fresh, "the Store records the current schema");
    assert_eq!(
        recorded,
        definition(&sql),
        "the recorded definition is the live one"
    );
    drop(sql);
    support::reopen(&path);
}

#[test]
fn commits_layers_and_epochs_are_backfilled_in_deterministic_order() {
    let t = Temp::new("gc-backfill-graph");
    let path = t.join("db");
    let fresh;
    let (stack_one, stack_two);
    {
        let handles = create(&path);
        let one = initialize(&handles, 1);
        let two = initialize(&handles, 2);
        let a = branch_id(1);
        fork(&handles, one.id, a, 1, ForkSource::Layer(one.head_layer));
        commit(&handles, a, 10);
        publish(&handles, one.id, a);
        commit(&handles, a, 11);
        publish(&handles, one.id, a);

        let b = branch_id(2);
        fork(&handles, two.id, b, 2, ForkSource::Layer(two.head_layer));
        commit(&handles, b, 20);
        publish(&handles, two.id, b);
        stack_one = one.id;
        stack_two = two.id;
        drop(handles);
        fresh = strip_position_state(&path);
    }

    writable(&path).expect("a pre-T1.4 Store opens writable");
    let sql = rusqlite::Connection::open(&path).unwrap();

    // Commits take consecutive Store serials in (created_ms, commit_id) order.
    let mut commits: Vec<(Vec<u8>, i64, i64)> = {
        let mut statement = sql
            .prepare("SELECT commit_id,created_ms,history_serial FROM \"commit\"")
            .unwrap();
        let rows = statement
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap();
        rows.map(|r| r.unwrap()).collect()
    };
    commits.sort_by(|a, b| (a.1, &a.0).cmp(&(b.1, &b.0)));
    let ordered: Vec<i64> = commits.iter().map(|c| c.2).collect();
    assert_eq!(
        ordered,
        (1..=ordered.len() as i64).collect::<Vec<_>>(),
        "serials are dense and follow the deterministic order"
    );

    // Per project the genesis Layer takes the next serial, in stack-id order.
    let genesis_one: i64 = sql
        .query_row(
            "SELECT history_serial FROM layer WHERE parent_layer_id IS NULL AND layer_stack_id=?1",
            rusqlite::params![stack_one.as_slice()],
            |r| r.get(0),
        )
        .unwrap();
    let genesis_two: i64 = sql
        .query_row(
            "SELECT history_serial FROM layer WHERE parent_layer_id IS NULL AND layer_stack_id=?1",
            rusqlite::params![stack_two.as_slice()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(genesis_one.min(genesis_two), commits.len() as i64 + 1);
    assert_eq!(genesis_one.max(genesis_two), commits.len() as i64 + 2);

    // Every republished tree carries the serial of the Commit it publishes.
    for (commit_id, _, serial) in &commits {
        let layer: i64 = sql
            .query_row(
                "SELECT history_serial FROM layer WHERE source_commit_id=?1",
                rusqlite::params![commit_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(layer, *serial, "a Layer carries its source Commit serial");
    }
    assert_eq!(
        next_serial(&sql),
        commits.len() as i64 + 3,
        "the next serial follows the highest assigned one"
    );

    // One epoch row per project, both zero.
    let rows = epochs(&sql);
    assert_eq!(rows.len(), 2);
    let mut expected = [stack_one, stack_two];
    expected.sort_by(|a, b| a.as_slice().cmp(b.as_slice()));
    for (row, expected) in rows.iter().zip(expected.iter()) {
        assert_eq!(row.0, expected.as_slice());
        assert_eq!(row.1, 0);
    }

    let before = next_serial(&sql);
    drop(sql);
    drop(writable(&path).expect("reopening changes nothing"));
    let sql = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(next_serial(&sql), before, "reopening keeps the serials");
    assert_eq!(epochs(&sql).len(), 2);
    let (source, recorded): (String, String) = sql
        .query_row(
            "SELECT schema_source,schema_definition FROM history_meta",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(source, fresh);
    assert_eq!(recorded, definition(&sql));
}

/// Spec 4.1: Commits take their serials in `(created_ms, stack, id)` order, so
/// two Commits created in the same millisecond still order by their Stack and
/// then their identity, never by identity alone.
#[test]
fn equal_created_ms_commits_order_by_stack_then_identity() {
    let t = Temp::new("gc-backfill-ties");
    let path = t.join("db");
    {
        let handles = create(&path);
        let one = initialize(&handles, 1);
        let two = initialize(&handles, 2);
        let a = branch_id(1);
        let b = branch_id(2);
        fork(&handles, one.id, a, 1, ForkSource::Layer(one.head_layer));
        fork(&handles, two.id, b, 2, ForkSource::Layer(two.head_layer));
        commit(&handles, a, 10);
        commit(&handles, b, 20);
        drop(handles);
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.execute("UPDATE \"commit\" SET created_ms=1000", [])
            .unwrap();
        drop(sql);
        strip_position_state(&path);
    }

    writable(&path).expect("a pre-T1.4 Store opens writable");
    let sql = rusqlite::Connection::open(&path).unwrap();
    let mut commits: Vec<(Vec<u8>, Vec<u8>, i64)> = {
        let mut statement = sql
            .prepare("SELECT layer_stack_id,commit_id,history_serial FROM \"commit\"")
            .unwrap();
        let rows = statement
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap();
        rows.map(|r| r.unwrap()).collect()
    };
    commits.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    let ordered: Vec<i64> = commits.iter().map(|c| c.2).collect();
    assert_eq!(
        ordered,
        (1..=ordered.len() as i64).collect::<Vec<_>>(),
        "the serials follow (created_ms, stack, id), not identity alone"
    );

    // The genesis Layers follow every Commit, one per Stack in stack order,
    // and the next serial is one above the highest assigned serial.
    let genesis: Vec<i64> = {
        let mut statement = sql
            .prepare("SELECT history_serial FROM layer WHERE parent_layer_id IS NULL ORDER BY layer_stack_id")
            .unwrap();
        let rows = statement.query_map([], |r| r.get(0)).unwrap();
        rows.map(|r| r.unwrap()).collect()
    };
    assert_eq!(
        genesis,
        (commits.len() as i64 + 1..=commits.len() as i64 + genesis.len() as i64)
            .collect::<Vec<_>>(),
        "genesis Layers take the next Store serials after the Commits"
    );
    assert_eq!(
        next_serial(&sql),
        commits.len() as i64 + genesis.len() as i64 + 1
    );
    drop(sql);
}

#[test]
fn republishing_layers_share_their_source_commit_serial() {
    let t = Temp::new("gc-backfill-republish");
    let path = t.join("db");
    let (shared_commit, first_layer, second_layer);
    {
        let handles = create(&path);
        let one = initialize(&handles, 1);
        let a = branch_id(1);
        fork(&handles, one.id, a, 1, ForkSource::Layer(one.head_layer));
        commit(&handles, a, 10);
        let first = publish(&handles, one.id, a);
        // Advance the stack so publishing the older Commit again is not the
        // no-change shortcut.
        commit(&handles, a, 11);
        let advanced = publish(&handles, one.id, a);

        let b = branch_id(2);
        fork(
            &handles,
            one.id,
            b,
            2,
            ForkSource::Commit {
                branch: a,
                commit: first.source_commit.unwrap(),
            },
        );
        // Re-publish the Commit that already has a Layer on branch `a`.
        let second = publish(&handles, one.id, b);
        assert_ne!(first.id, second.id);
        assert_ne!(advanced.id, second.id);
        assert_eq!(first.source_commit, second.source_commit);
        shared_commit = first.source_commit.unwrap();
        first_layer = first.id;
        second_layer = second.id;
        drop(handles);
        strip_position_state(&path);
    }

    writable(&path).expect("a pre-T1.4 Store opens writable");
    let sql = rusqlite::Connection::open(&path).unwrap();
    let serial: i64 = sql
        .query_row(
            "SELECT history_serial FROM \"commit\" WHERE commit_id=?1",
            rusqlite::params![shared_commit.as_slice()],
            |r| r.get(0),
        )
        .unwrap();
    for layer in [first_layer, second_layer] {
        let layer_serial: i64 = sql
            .query_row(
                "SELECT history_serial FROM layer WHERE layer_id=?1",
                rusqlite::params![layer.as_slice()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            layer_serial, serial,
            "a republishing Layer carries its source Commit serial"
        );
    }
    assert_eq!(epochs(&sql).len(), 1);
    drop(sql);
}

#[test]
fn existing_serials_are_kept_and_new_ones_start_above_the_maximum() {
    let t = Temp::new("gc-backfill-monotonic");
    let path = t.join("db");
    let (kept_commit, other_commit);
    {
        let handles = create(&path);
        let one = initialize(&handles, 1);
        let a = branch_id(1);
        fork(&handles, one.id, a, 1, ForkSource::Layer(one.head_layer));
        let kept = commit(&handles, a, 10);
        publish(&handles, one.id, a);
        let other = commit(&handles, a, 11);
        publish(&handles, one.id, a);
        kept_commit = kept.id;
        other_commit = other.id;
        drop(handles);
        strip_position_state(&path);
    }

    // A Store brought part-way forward may already hold serials; they are kept
    // and the backfill starts above the highest one.
    {
        let sql = rusqlite::Connection::open(&path).unwrap();
        sql.execute(
            "UPDATE \"commit\" SET history_serial=100 WHERE commit_id=?1",
            rusqlite::params![kept_commit.as_slice()],
        )
        .unwrap();
        // A Layer that already holds a serial is not rewritten either.
        sql.execute(
            "UPDATE layer SET history_serial=7 WHERE source_commit_id=?1",
            rusqlite::params![kept_commit.as_slice()],
        )
        .unwrap();
        drop(sql);
    }

    writable(&path).expect("a part-way Store opens writable");
    let sql = rusqlite::Connection::open(&path).unwrap();
    let kept: i64 = sql
        .query_row(
            "SELECT history_serial FROM \"commit\" WHERE commit_id=?1",
            rusqlite::params![kept_commit.as_slice()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(kept, 100, "an assigned Commit serial is never rewritten");
    let assigned: i64 = sql
        .query_row(
            "SELECT history_serial FROM \"commit\" WHERE commit_id=?1",
            rusqlite::params![other_commit.as_slice()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(assigned, 101, "new serials start above the maximum");
    let existing_layer: i64 = sql
        .query_row(
            "SELECT history_serial FROM layer WHERE source_commit_id=?1",
            rusqlite::params![kept_commit.as_slice()],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        existing_layer, 7,
        "an assigned Layer serial is never rewritten"
    );
    let genesis: i64 = sql
        .query_row(
            "SELECT history_serial FROM layer WHERE parent_layer_id IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(genesis, 102, "a genesis Layer follows the commits");
    assert_eq!(next_serial(&sql), 103);

    // A high-water written later is never lowered by another open.
    sql.execute("UPDATE history_meta SET next_history_serial=500", [])
        .unwrap();
    drop(sql);
    drop(writable(&path).expect("a forward Store reopens"));
    let sql = rusqlite::Connection::open(&path).unwrap();
    assert_eq!(next_serial(&sql), 500);
    drop(sql);
}
